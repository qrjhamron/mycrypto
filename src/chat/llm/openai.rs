//! OpenAI API provider.
//!
//! Implements streaming SSE for OpenAI models via api.openai.com.

use std::env;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use async_trait::async_trait;
use futures_util::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::time::sleep;
use tracing::warn;

use crate::config::LlmConfig;
use crate::error::{MycryptoError, Result};

use super::provider::{LlmProvider, Message, Token, TokenStream};

const OPENAI_API_URL: &str = "https://api.openai.com/v1/chat/completions";

/// OpenAI API provider.
pub struct OpenAIProvider {
    client: Client,
    api_key: Option<String>,
}

impl OpenAIProvider {
    /// Create a new OpenAI provider.
    pub fn new() -> Self {
        let api_key = env::var("OPENAI_API_KEY").ok();

        Self {
            client: build_http_client(),
            api_key,
        }
    }

    /// Create with explicit API key.
    #[cfg(test)]
    pub fn with_api_key(api_key: impl Into<String>) -> Self {
        Self {
            client: build_http_client(),
            api_key: Some(api_key.into()),
        }
    }

    fn get_api_key(&self, config: &LlmConfig) -> Option<String> {
        if !config.api_key.is_empty() && !config.api_key.starts_with("ENV:") {
            return Some(config.api_key.clone());
        }

        self.api_key
            .clone()
            .or_else(|| env::var("OPENAI_API_KEY").ok())
    }
}

impl Default for OpenAIProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// OpenAI API request body.
#[derive(Debug, Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    max_tokens: u32,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct OpenAIMessage {
    role: String,
    content: String,
}

/// OpenAI SSE chunk.
#[derive(Debug, Deserialize)]
struct OpenAIChunk {
    choices: Vec<OpenAIChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    delta: OpenAIDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIDelta {
    content: Option<String>,
}

#[async_trait]
impl LlmProvider for OpenAIProvider {
    fn name(&self) -> &str {
        "openai"
    }

    fn validate_config(&self) -> Result<()> {
        if self.api_key.is_none() && env::var("OPENAI_API_KEY").is_err() {
            return Err(MycryptoError::LlmAuth("OPENAI_API_KEY not set".to_string()));
        }
        Ok(())
    }

    fn has_credentials(&self) -> bool {
        self.api_key.is_some() || env::var("OPENAI_API_KEY").is_ok()
    }

    async fn stream_completion(
        &self,
        messages: Vec<Message>,
        config: &LlmConfig,
    ) -> Result<TokenStream> {
        let api_key = self
            .get_api_key(config)
            .ok_or_else(|| MycryptoError::LlmAuth("OPENAI_API_KEY not configured".to_string()))?;

        let openai_messages: Vec<OpenAIMessage> = messages
            .into_iter()
            .map(|m| OpenAIMessage {
                role: m.role.as_str().to_string(),
                content: m.content,
            })
            .collect();

        let model = if config.model.starts_with("gpt") {
            config.model.clone()
        } else {
            "gpt-4o".to_string()
        };

        let request = OpenAIRequest {
            model,
            messages: openai_messages,
            max_tokens: config.max_tokens,
            stream: true,
        };

        let mut attempt: u8 = 0;
        let response = loop {
            let response = self
                .client
                .post(OPENAI_API_URL)
                .header("Authorization", format!("Bearer {}", api_key))
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
                .await
                .map_err(|e| MycryptoError::LlmRequest(format!("OpenAI request failed: {}", e)))?;

            if response.status().is_success() {
                break response;
            }

            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            warn!(
                "OpenAI API returned non-success status {} (attempt {}): {}",
                status,
                attempt + 1,
                body_preview(&body)
            );

            if (status == 429 || status == 503) && attempt == 0 {
                attempt += 1;
                sleep(Duration::from_secs(2)).await;
                continue;
            }

            return Err(MycryptoError::ApiError {
                api: "openai".to_string(),
                status,
                message: format!(
                    "OpenAI API request failed with status {}: {}",
                    status,
                    body_preview(&body)
                ),
            });
        };

        let byte_stream = response.bytes_stream();
        Ok(Box::pin(OpenAIStream::new(byte_stream)))
    }
}

fn build_http_client() -> Client {
    match Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(120))
        .build()
    {
        Ok(client) => client,
        Err(err) => {
            warn!(
                "Failed to build OpenAI HTTP client with timeouts: {}. Using default client",
                err
            );
            Client::new()
        }
    }
}

fn body_preview(body: &str) -> String {
    let trimmed = body.trim();
    let preview: String = trimmed.chars().take(200).collect();
    if trimmed.chars().count() > 200 {
        format!("{}...", preview)
    } else {
        preview
    }
}

/// Stream that parses OpenAI SSE events into tokens.
struct OpenAIStream<S> {
    inner: S,
    buffer: String,
    finished: bool,
}

impl<S> OpenAIStream<S> {
    fn new(inner: S) -> Self {
        Self {
            inner,
            buffer: String::new(),
            finished: false,
        }
    }
}

impl<S> Stream for OpenAIStream<S>
where
    S: Stream<Item = std::result::Result<bytes::Bytes, reqwest::Error>> + Unpin,
{
    type Item = Result<Token>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.finished {
            return Poll::Ready(None);
        }

        loop {
            // Look for complete SSE event
            if let Some(event_end) = self.buffer.find("\n\n") {
                let event_str = self.buffer[..event_end].to_string();
                self.buffer = self.buffer[event_end + 2..].to_string();

                for line in event_str.lines() {
                    if let Some(data) = line.strip_prefix("data: ") {
                        if data == "[DONE]" {
                            self.finished = true;
                            return Poll::Ready(Some(Ok(Token::final_token("stop"))));
                        }

                        if let Ok(chunk) = serde_json::from_str::<OpenAIChunk>(data) {
                            if let Some(choice) = chunk.choices.first() {
                                if let Some(reason) = &choice.finish_reason {
                                    self.finished = true;
                                    return Poll::Ready(Some(Ok(Token::final_token(reason))));
                                }
                                if let Some(content) = &choice.delta.content {
                                    if !content.is_empty() {
                                        return Poll::Ready(Some(Ok(Token::new(content))));
                                    }
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
    fn test_openai_provider_creation() {
        let provider = OpenAIProvider::new();
        assert_eq!(provider.name(), "openai");
    }

    #[test]
    fn test_openai_with_api_key() {
        let provider = OpenAIProvider::with_api_key("test-key");
        assert!(provider.api_key.is_some());
    }
}
