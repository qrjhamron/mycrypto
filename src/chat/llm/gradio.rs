//! Gradio Space API provider.
//!
//! Supports HuggingFace hosted models via Gradio API endpoints.

use std::env;
use std::pin::Pin;
use std::task::{Context, Poll};

use async_trait::async_trait;
use futures_util::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::config::LlmConfig;
use crate::error::{MycryptoError, Result};

use super::provider::{LlmProvider, Message, Role, Token, TokenStream};

/// Default Gradio space for chat models.
const DEFAULT_GRADIO_SPACE: &str = "https://huggingface.co/spaces/HuggingFaceH4/zephyr-chat";

/// Gradio Space API provider.
pub struct GradioProvider {
    client: Client,
    api_key: Option<String>,
    space_url: Option<String>,
}

impl GradioProvider {
    /// Create a new Gradio provider.
    pub fn new() -> Self {
        let api_key = env::var("GRADIO_API_KEY").ok();
        let space_url = env::var("GRADIO_SPACE_URL").ok();

        Self {
            client: Client::new(),
            api_key,
            space_url,
        }
    }

    /// Create with explicit space URL.
    #[cfg(test)]
    pub fn with_space_url(space_url: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: env::var("GRADIO_API_KEY").ok(),
            space_url: Some(space_url.into()),
        }
    }

    fn get_space_url(&self, config: &LlmConfig) -> String {
        // Check config base_url first
        if let Some(base_url) = &config.base_url {
            if !base_url.is_empty() {
                return base_url.clone();
            }
        }

        // Then env var
        self.space_url
            .clone()
            .or_else(|| env::var("GRADIO_SPACE_URL").ok())
            .unwrap_or_else(|| DEFAULT_GRADIO_SPACE.to_string())
    }

    fn get_api_key(&self, config: &LlmConfig) -> Option<String> {
        if !config.api_key.is_empty() && !config.api_key.starts_with("ENV:") {
            return Some(config.api_key.clone());
        }

        self.api_key
            .clone()
            .or_else(|| env::var("GRADIO_API_KEY").ok())
    }
}

impl Default for GradioProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// Gradio API request for chat.
#[derive(Debug, Serialize)]
struct GradioRequest {
    data: Vec<serde_json::Value>,
}

/// Gradio streaming response.
#[derive(Debug, Deserialize)]
struct GradioResponse {
    data: Option<Vec<serde_json::Value>>,
    #[serde(rename = "is_generating")]
    is_generating: Option<bool>,
}

#[async_trait]
impl LlmProvider for GradioProvider {
    fn name(&self) -> &str {
        "gradio"
    }

    fn validate_config(&self) -> Result<()> {
        // Gradio doesn't always require an API key (public spaces)
        Ok(())
    }

    fn has_credentials(&self) -> bool {
        // Gradio can work without credentials for public spaces
        true
    }

    async fn stream_completion(
        &self,
        messages: Vec<Message>,
        config: &LlmConfig,
    ) -> Result<TokenStream> {
        let space_url = self.get_space_url(config);
        let api_key = self.get_api_key(config);

        // Convert messages to chat history format
        // Most Gradio chat interfaces expect: [[user_msg, assistant_msg], ...]
        let mut history: Vec<Vec<String>> = Vec::new();
        let mut current_user_msg: Option<String> = None;
        let mut system_prompt = String::new();

        for msg in &messages {
            match msg.role {
                Role::System => {
                    system_prompt = msg.content.clone();
                }
                Role::User => {
                    if let Some(user_msg) = current_user_msg.take() {
                        // Previous user message without response
                        history.push(vec![user_msg, String::new()]);
                    }
                    current_user_msg = Some(msg.content.clone());
                }
                Role::Assistant => {
                    if let Some(user_msg) = current_user_msg.take() {
                        history.push(vec![user_msg, msg.content.clone()]);
                    }
                }
            }
        }

        // Get the latest user message
        let user_input = current_user_msg.unwrap_or_default();

        // Prepend system prompt to first message if present
        let full_input = if !system_prompt.is_empty() && history.is_empty() {
            format!("{}\n\n{}", system_prompt, user_input)
        } else {
            user_input
        };

        // Gradio endpoint for streaming
        let api_url = format!("{}/api/predict", space_url.trim_end_matches('/'));

        let request = GradioRequest {
            data: vec![serde_json::json!(full_input), serde_json::json!(history)],
        };

        let mut req = self
            .client
            .post(&api_url)
            .header("Content-Type", "application/json");

        if let Some(key) = api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        let response = req
            .json(&request)
            .send()
            .await
            .map_err(|e| MycryptoError::LlmRequest(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(MycryptoError::ApiError {
                api: "gradio".to_string(),
                status,
                message: body,
            });
        }

        let byte_stream = response.bytes_stream();
        Ok(Box::pin(GradioStream::new(byte_stream)))
    }
}

/// Stream that parses Gradio responses into tokens.
struct GradioStream<S> {
    inner: S,
    buffer: String,
    finished: bool,
    last_text: String,
}

impl<S> GradioStream<S> {
    fn new(inner: S) -> Self {
        Self {
            inner,
            buffer: String::new(),
            finished: false,
            last_text: String::new(),
        }
    }
}

impl<S> Stream for GradioStream<S>
where
    S: Stream<Item = std::result::Result<bytes::Bytes, reqwest::Error>> + Unpin,
{
    type Item = Result<Token>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.finished {
            return Poll::Ready(None);
        }

        loop {
            // Try to parse JSON from buffer
            if let Some(event_end) = self.buffer.find('\n') {
                let line = self.buffer[..event_end].to_string();
                self.buffer = self.buffer[event_end + 1..].to_string();

                if line.trim().is_empty() {
                    continue;
                }

                // Handle SSE format
                let data = if let Some(d) = line.strip_prefix("data: ") {
                    d.to_string()
                } else {
                    line
                };

                if let Ok(response) = serde_json::from_str::<GradioResponse>(&data) {
                    // Check if generation is complete
                    if response.is_generating == Some(false) {
                        self.finished = true;
                        return Poll::Ready(Some(Ok(Token::final_token("stop"))));
                    }

                    // Extract text from response
                    if let Some(data_arr) = response.data {
                        if let Some(text_val) = data_arr.first() {
                            if let Some(text) = text_val.as_str() {
                                // Gradio often sends full text, so compute delta
                                let delta = if text.len() > self.last_text.len() {
                                    text[self.last_text.len()..].to_string()
                                } else {
                                    text.to_string()
                                };
                                self.last_text = text.to_string();

                                if !delta.is_empty() {
                                    return Poll::Ready(Some(Ok(Token::new(delta))));
                                }
                            }
                        }
                    }
                }
                continue;
            }

            break;
        }

        match Pin::new(&mut self.inner).poll_next(cx) {
            Poll::Ready(Some(Ok(bytes))) => {
                if let Ok(text) = String::from_utf8(bytes.to_vec()) {
                    self.buffer.push_str(&text);
                }
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Poll::Ready(Some(Err(e))) => {
                self.finished = true;
                Poll::Ready(Some(Err(MycryptoError::LlmRequest(e.to_string()))))
            }
            Poll::Ready(None) => {
                // Stream ended, emit any remaining text
                if !self.last_text.is_empty() {
                    self.finished = true;
                    return Poll::Ready(Some(Ok(Token::final_token("stop"))));
                }
                self.finished = true;
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gradio_provider_creation() {
        let provider = GradioProvider::new();
        assert_eq!(provider.name(), "gradio");
        assert!(provider.has_credentials()); // Always true for public spaces
    }

    #[test]
    fn test_gradio_with_space_url() {
        let provider = GradioProvider::with_space_url("https://example.com/space");
        assert_eq!(
            provider.space_url,
            Some("https://example.com/space".to_string())
        );
    }
}
