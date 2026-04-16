//! Anthropic Claude API provider.
//!
//! Implements streaming SSE for Claude models via api.anthropic.com.

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

use super::provider::{LlmProvider, Message, Role, Token, TokenStream};

const CLAUDE_API_URL: &str = "https://api.anthropic.com/v1/messages";
const CLAUDE_API_VERSION: &str = "2023-06-01";

/// Anthropic Claude API provider.
pub struct ClaudeProvider {
    client: Client,
    api_key: Option<String>,
}

impl ClaudeProvider {
    /// Create a new Claude provider.
    pub fn new() -> Self {
        let api_key = env::var("CLAUDE_API_KEY").ok();

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
        // First try config (which may have resolved ENV: prefix)
        if !config.api_key.is_empty()
            && !config.api_key.starts_with("ENV:")
            && config.api_key != "ENV:CLAUDE_API_KEY"
        {
            return Some(config.api_key.clone());
        }

        // Fall back to cached env var or re-check
        self.api_key
            .clone()
            .or_else(|| env::var("CLAUDE_API_KEY").ok())
    }
}

impl Default for ClaudeProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// Claude API request body.
#[derive(Debug, Serialize)]
struct ClaudeRequest {
    model: String,
    max_tokens: u32,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<ClaudeMessage>,
}

#[derive(Debug, Serialize)]
struct ClaudeMessage {
    role: String,
    content: String,
}

/// Claude SSE event types.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ClaudeEvent {
    #[serde(rename = "message_start")]
    MessageStart,
    #[serde(rename = "content_block_start")]
    ContentBlockStart,
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta { delta: ContentDelta },
    #[serde(rename = "content_block_stop")]
    ContentBlockStop,
    #[serde(rename = "message_delta")]
    MessageDelta {
        delta: MessageDeltaInfo,
        _usage: Option<UsageInfo>,
    },
    #[serde(rename = "message_stop")]
    MessageStop,
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "error")]
    Error { error: ClaudeError },
}

#[derive(Debug, Deserialize)]
struct ContentDelta {
    #[serde(rename = "type")]
    _delta_type: String,
    #[serde(rename = "text")]
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MessageDeltaInfo {
    #[serde(rename = "stop_reason")]
    stop_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UsageInfo {
    #[serde(rename = "output_tokens")]
    _output_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct ClaudeError {
    message: String,
}

#[async_trait]
impl LlmProvider for ClaudeProvider {
    fn name(&self) -> &str {
        "claude"
    }

    fn validate_config(&self) -> Result<()> {
        if self.api_key.is_none() && env::var("CLAUDE_API_KEY").is_err() {
            return Err(MycryptoError::LlmAuth("CLAUDE_API_KEY not set".to_string()));
        }
        Ok(())
    }

    fn has_credentials(&self) -> bool {
        self.api_key.is_some() || env::var("CLAUDE_API_KEY").is_ok()
    }

    async fn stream_completion(
        &self,
        messages: Vec<Message>,
        config: &LlmConfig,
    ) -> Result<TokenStream> {
        let api_key = self
            .get_api_key(config)
            .ok_or_else(|| MycryptoError::LlmAuth("CLAUDE_API_KEY not configured".to_string()))?;

        // Extract system message if present
        let (system_msg, chat_messages): (Option<String>, Vec<ClaudeMessage>) = {
            let mut system = None;
            let mut msgs = Vec::new();

            for msg in messages {
                match msg.role {
                    Role::System => {
                        system = Some(msg.content);
                    }
                    Role::User => {
                        msgs.push(ClaudeMessage {
                            role: "user".to_string(),
                            content: msg.content,
                        });
                    }
                    Role::Assistant => {
                        msgs.push(ClaudeMessage {
                            role: "assistant".to_string(),
                            content: msg.content,
                        });
                    }
                }
            }

            (system, msgs)
        };

        let request = ClaudeRequest {
            model: config.model.clone(),
            max_tokens: config.max_tokens,
            stream: true,
            system: system_msg,
            messages: chat_messages,
        };

        let mut attempt: u8 = 0;
        let response = loop {
            let response = self
                .client
                .post(CLAUDE_API_URL)
                .header("x-api-key", &api_key)
                .header("anthropic-version", CLAUDE_API_VERSION)
                .header("content-type", "application/json")
                .json(&request)
                .send()
                .await
                .map_err(|e| MycryptoError::LlmRequest(format!("Claude request failed: {}", e)))?;

            if response.status().is_success() {
                break response;
            }

            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            warn!(
                "Claude API returned non-success status {} (attempt {}): {}",
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
                api: "claude".to_string(),
                status,
                message: format!(
                    "Claude API request failed with status {}: {}",
                    status,
                    body_preview(&body)
                ),
            });
        };

        let byte_stream = response.bytes_stream();
        Ok(Box::pin(ClaudeStream::new(byte_stream)))
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
                "Failed to build Claude HTTP client with timeouts: {}. Using default client",
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

/// Stream that parses Claude SSE events into tokens.
struct ClaudeStream<S> {
    inner: S,
    buffer: String,
    finished: bool,
}

impl<S> ClaudeStream<S> {
    fn new(inner: S) -> Self {
        Self {
            inner,
            buffer: String::new(),
            finished: false,
        }
    }
}

impl<S> Stream for ClaudeStream<S>
where
    S: Stream<Item = std::result::Result<bytes::Bytes, reqwest::Error>> + Unpin,
{
    type Item = Result<Token>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.finished {
            return Poll::Ready(None);
        }

        // Try to parse any complete events in the buffer
        loop {
            // Look for complete SSE event (ends with double newline)
            if let Some(event_end) = self.buffer.find("\n\n") {
                let event_str = self.buffer[..event_end].to_string();
                self.buffer = self.buffer[event_end + 2..].to_string();

                // Parse SSE event
                if let Some(data) = event_str.strip_prefix("data: ") {
                    if data == "[DONE]" {
                        self.finished = true;
                        return Poll::Ready(Some(Ok(Token::final_token("stop"))));
                    }

                    match serde_json::from_str::<ClaudeEvent>(data) {
                        Ok(event) => match event {
                            ClaudeEvent::ContentBlockDelta { delta, .. } => {
                                if let Some(text) = delta.text {
                                    return Poll::Ready(Some(Ok(Token::new(text))));
                                }
                            }
                            ClaudeEvent::MessageStop => {
                                self.finished = true;
                                return Poll::Ready(Some(Ok(Token::final_token("stop"))));
                            }
                            ClaudeEvent::MessageDelta { delta, .. } => {
                                if let Some(reason) = delta.stop_reason {
                                    self.finished = true;
                                    return Poll::Ready(Some(Ok(Token::final_token(reason))));
                                }
                            }
                            ClaudeEvent::Error { error } => {
                                self.finished = true;
                                return Poll::Ready(Some(Err(MycryptoError::LlmRequest(
                                    error.message,
                                ))));
                            }
                            _ => {
                                // Skip other event types (message_start, etc.)
                                continue;
                            }
                        },
                        Err(_) => {
                            // Skip unparseable events
                            continue;
                        }
                    }
                }
                continue;
            }

            // Need more data
            break;
        }

        // Poll for more bytes
        match Pin::new(&mut self.inner).poll_next(cx) {
            Poll::Ready(Some(Ok(bytes))) => {
                if let Ok(text) = String::from_utf8(bytes.to_vec()) {
                    self.buffer.push_str(&text);
                }
                // Re-poll to process new data
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
    fn test_claude_provider_creation() {
        let provider = ClaudeProvider::new();
        assert_eq!(provider.name(), "claude");
    }

    #[test]
    fn test_claude_with_api_key() {
        let provider = ClaudeProvider::with_api_key("test-key");
        assert!(provider.api_key.is_some());
    }
}
