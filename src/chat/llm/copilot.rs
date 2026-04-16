//! GitHub Copilot API provider.
//!
//! Uses GitHub Copilot's chat API authenticated via GITHUB_TOKEN.

use std::env;
use std::pin::Pin;
use std::task::{Context, Poll};

use async_trait::async_trait;
use futures_util::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::config::LlmConfig;
use crate::error::{MycryptoError, Result};

use super::provider::{LlmProvider, Message, Token, TokenStream};

const COPILOT_API_URL: &str = "https://api.githubcopilot.com/chat/completions";
const COPILOT_TOKEN_URL: &str = "https://api.github.com/copilot_internal/v2/token";

/// GitHub Copilot API provider.
pub struct CopilotProvider {
    client: Client,
    github_token: Option<String>,
}

impl CopilotProvider {
    /// Create a new Copilot provider.
    pub fn new() -> Self {
        let github_token = env::var("GITHUB_TOKEN").ok();

        Self {
            client: Client::new(),
            github_token,
        }
    }

    /// Create with explicit GitHub token.
    #[cfg(test)]
    pub fn with_token(token: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            github_token: Some(token.into()),
        }
    }

    fn get_github_token(&self, config: &LlmConfig) -> Option<String> {
        if !config.api_key.is_empty() && !config.api_key.starts_with("ENV:") {
            return Some(config.api_key.clone());
        }

        self.github_token
            .clone()
            .or_else(|| env::var("GITHUB_TOKEN").ok())
    }

    /// Get Copilot API token from GitHub token.
    async fn get_copilot_token(&self, github_token: &str) -> Result<String> {
        let response = self
            .client
            .get(COPILOT_TOKEN_URL)
            .header("Authorization", format!("token {}", github_token))
            .header("Accept", "application/json")
            .header("User-Agent", "mycrypto/0.1.0")
            .send()
            .await
            .map_err(|e| MycryptoError::LlmRequest(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(MycryptoError::ApiError {
                api: "copilot-token".to_string(),
                status,
                message: body,
            });
        }

        let token_response: CopilotTokenResponse = response
            .json()
            .await
            .map_err(|e| MycryptoError::LlmResponseParse(e.to_string()))?;

        Ok(token_response.token)
    }
}

impl Default for CopilotProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// Copilot token response.
#[derive(Debug, Deserialize)]
struct CopilotTokenResponse {
    token: String,
}

/// Copilot API request body (OpenAI-compatible).
#[derive(Debug, Serialize)]
struct CopilotRequest {
    model: String,
    messages: Vec<CopilotMessage>,
    max_tokens: u32,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct CopilotMessage {
    role: String,
    content: String,
}

/// Copilot SSE chunk.
#[derive(Debug, Deserialize)]
struct CopilotChunk {
    choices: Vec<CopilotChoice>,
}

#[derive(Debug, Deserialize)]
struct CopilotChoice {
    delta: CopilotDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CopilotDelta {
    content: Option<String>,
}

#[async_trait]
impl LlmProvider for CopilotProvider {
    fn name(&self) -> &str {
        "copilot"
    }

    fn validate_config(&self) -> Result<()> {
        if self.github_token.is_none() && env::var("GITHUB_TOKEN").is_err() {
            return Err(MycryptoError::LlmAuth("GITHUB_TOKEN not set".to_string()));
        }
        Ok(())
    }

    fn has_credentials(&self) -> bool {
        self.github_token.is_some() || env::var("GITHUB_TOKEN").is_ok()
    }

    async fn stream_completion(
        &self,
        messages: Vec<Message>,
        config: &LlmConfig,
    ) -> Result<TokenStream> {
        let github_token = self
            .get_github_token(config)
            .ok_or_else(|| MycryptoError::LlmAuth("GITHUB_TOKEN not configured".to_string()))?;

        // Get Copilot API token
        let copilot_token = self.get_copilot_token(&github_token).await?;

        let copilot_messages: Vec<CopilotMessage> = messages
            .into_iter()
            .map(|m| CopilotMessage {
                role: m.role.as_str().to_string(),
                content: m.content,
            })
            .collect();

        let request = CopilotRequest {
            model: "gpt-4o".to_string(), // Copilot uses GPT-4o
            messages: copilot_messages,
            max_tokens: config.max_tokens,
            stream: true,
        };

        let response = self
            .client
            .post(COPILOT_API_URL)
            .header("Authorization", format!("Bearer {}", copilot_token))
            .header("Content-Type", "application/json")
            .header("Editor-Version", "mycrypto/0.1.0")
            .header("Openai-Intent", "conversation-panel")
            .json(&request)
            .send()
            .await
            .map_err(|e| MycryptoError::LlmRequest(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(MycryptoError::ApiError {
                api: "copilot".to_string(),
                status,
                message: body,
            });
        }

        let byte_stream = response.bytes_stream();
        Ok(Box::pin(CopilotStream::new(byte_stream)))
    }
}

/// Stream that parses Copilot SSE events into tokens.
struct CopilotStream<S> {
    inner: S,
    buffer: String,
    finished: bool,
}

impl<S> CopilotStream<S> {
    fn new(inner: S) -> Self {
        Self {
            inner,
            buffer: String::new(),
            finished: false,
        }
    }
}

impl<S> Stream for CopilotStream<S>
where
    S: Stream<Item = std::result::Result<bytes::Bytes, reqwest::Error>> + Unpin,
{
    type Item = Result<Token>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.finished {
            return Poll::Ready(None);
        }

        loop {
            if let Some(event_end) = self.buffer.find("\n\n") {
                let event_str = self.buffer[..event_end].to_string();
                self.buffer = self.buffer[event_end + 2..].to_string();

                for line in event_str.lines() {
                    if let Some(data) = line.strip_prefix("data: ") {
                        if data == "[DONE]" {
                            self.finished = true;
                            return Poll::Ready(Some(Ok(Token::final_token("stop"))));
                        }

                        if let Ok(chunk) = serde_json::from_str::<CopilotChunk>(data) {
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
    fn test_copilot_provider_creation() {
        let provider = CopilotProvider::new();
        assert_eq!(provider.name(), "copilot");
    }

    #[test]
    fn test_copilot_with_token() {
        let provider = CopilotProvider::with_token("test-token");
        assert!(provider.github_token.is_some());
    }
}
