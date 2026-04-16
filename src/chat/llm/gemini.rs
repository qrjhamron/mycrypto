//! Google Gemini API provider.
//!
//! Implements streaming for Gemini models via generativelanguage.googleapis.com.

use std::collections::VecDeque;
use std::env;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use async_trait::async_trait;
use futures_util::Stream;
use reqwest::Client;
use serde::Serialize;
use serde_json::Value;
use tracing::warn;

use crate::config::LlmConfig;
use crate::error::{MycryptoError, Result};

use super::provider::{LlmProvider, Message, Role, Token, TokenStream};

const GEMINI_API_BASE: &str = "https://generativelanguage.googleapis.com/v1beta/models";
const DEFAULT_GEMINI_MODEL: &str = "gemini-2.0-flash";

/// Google Gemini API provider.
pub struct GeminiProvider {
    client: Client,
    api_key: Option<String>,
}

impl GeminiProvider {
    /// Create a new Gemini provider.
    pub fn new() -> Self {
        let api_key = preferred_gemini_key_from_env();

        Self {
            client: build_http_client(),
            api_key,
        }
    }

    /// Create with explicit API key.
    #[cfg(test)]
    pub fn with_api_key(api_key: impl Into<String>) -> Self {
        let key = normalize_key(Some(api_key.into()), "Gemini explicit API key");
        Self {
            client: build_http_client(),
            api_key: key,
        }
    }

    fn get_api_key(&self, config: &LlmConfig) -> Option<String> {
        if let Some(config_key) = normalize_config_api_key(config) {
            return Some(config_key);
        }

        self.api_key.clone().or_else(preferred_gemini_key_from_env)
    }

    fn resolve_model(&self, config: &LlmConfig) -> String {
        let config_model = config.model.trim();
        if !config_model.is_empty() && config_model != "claude-opus-4-5" {
            return config_model.to_string();
        }

        normalize_key(env::var("GEMINI_MODEL").ok(), "GEMINI_MODEL")
            .unwrap_or_else(|| DEFAULT_GEMINI_MODEL.to_string())
    }
}

impl Default for GeminiProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// Gemini API request body.
#[derive(Debug, Serialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(rename = "generationConfig")]
    generation_config: GeminiConfig,
}

#[derive(Debug, Serialize)]
struct GeminiContent {
    role: String,
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize)]
struct GeminiPart {
    text: String,
}

#[derive(Debug, Serialize)]
struct GeminiConfig {
    #[serde(rename = "maxOutputTokens")]
    max_output_tokens: u32,
}

#[async_trait]
impl LlmProvider for GeminiProvider {
    fn name(&self) -> &str {
        "gemini"
    }

    fn validate_config(&self) -> Result<()> {
        if self.api_key.is_none() && preferred_gemini_key_from_env().is_none() {
            return Err(MycryptoError::LlmAuth(
                "Gemini API key missing. Set GEMINI_API_KEY (preferred) or GOOGLE_API_KEY"
                    .to_string(),
            ));
        }
        Ok(())
    }

    fn has_credentials(&self) -> bool {
        self.api_key.is_some() || preferred_gemini_key_from_env().is_some()
    }

    async fn stream_completion(
        &self,
        messages: Vec<Message>,
        config: &LlmConfig,
    ) -> Result<TokenStream> {
        let api_key = self.get_api_key(config).ok_or_else(|| {
            MycryptoError::LlmAuth(
                "Gemini API key not configured. Set GEMINI_API_KEY or GOOGLE_API_KEY".to_string(),
            )
        })?;

        // Convert messages to Gemini format
        // Gemini uses "user" and "model" roles, and system message goes in first user message
        let mut contents = Vec::new();
        let mut system_prefix = String::new();

        for msg in messages {
            match msg.role {
                Role::System => {
                    system_prefix = format!("{}\n\n", msg.content);
                }
                Role::User => {
                    let text = if !system_prefix.is_empty() {
                        let combined = format!("{}{}", system_prefix, msg.content);
                        system_prefix.clear();
                        combined
                    } else {
                        msg.content
                    };
                    contents.push(GeminiContent {
                        role: "user".to_string(),
                        parts: vec![GeminiPart { text }],
                    });
                }
                Role::Assistant => {
                    contents.push(GeminiContent {
                        role: "model".to_string(),
                        parts: vec![GeminiPart { text: msg.content }],
                    });
                }
            }
        }

        let model = self.resolve_model(config);

        let request = GeminiRequest {
            contents,
            generation_config: GeminiConfig {
                max_output_tokens: config.max_tokens,
            },
        };

        let url = format!(
            "{}/{model}:streamGenerateContent?alt=sse&key={api_key}",
            GEMINI_API_BASE
        );

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| MycryptoError::LlmRequest(format!("Gemini request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            warn!(
                "Gemini API returned non-success status {}: {}",
                status,
                body_preview(&body)
            );
            return Err(MycryptoError::ApiError {
                api: "gemini".to_string(),
                status,
                message: format!(
                    "Gemini API request failed with status {}: {}",
                    status,
                    body_preview(&body)
                ),
            });
        }

        let byte_stream = response.bytes_stream();
        Ok(Box::pin(GeminiStream::new(byte_stream)))
    }
}

/// Stream that parses Gemini SSE events into tokens.
struct GeminiStream<S> {
    inner: S,
    buffer: String,
    finished: bool,
    pending_tokens: VecDeque<Token>,
}

impl<S> GeminiStream<S> {
    fn new(inner: S) -> Self {
        Self {
            inner,
            buffer: String::new(),
            finished: false,
            pending_tokens: VecDeque::new(),
        }
    }

    fn queue_payload_tokens(&mut self, payload: &Value) -> Option<MycryptoError> {
        if let Some(message) = payload
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
        {
            return Some(MycryptoError::LlmRequest(message.to_string()));
        }

        if let Some(text) = payload
            .pointer("/candidates/0/content/parts/0/text")
            .and_then(|t| t.as_str())
        {
            if !text.is_empty() {
                self.pending_tokens.push_back(Token::new(text));
            }
        }

        if let Some(reason) = payload
            .pointer("/candidates/0/finishReason")
            .and_then(|r| r.as_str())
        {
            if !reason.is_empty() && reason != "FINISH_REASON_UNSPECIFIED" {
                self.pending_tokens.push_back(Token::final_token(reason));
                self.finished = true;
            }
        }

        None
    }
}

impl<S> Stream for GeminiStream<S>
where
    S: Stream<Item = std::result::Result<bytes::Bytes, reqwest::Error>> + Unpin,
{
    type Item = Result<Token>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(token) = self.pending_tokens.pop_front() {
            return Poll::Ready(Some(Ok(token)));
        }

        if self.finished {
            return Poll::Ready(None);
        }

        loop {
            // Look for complete SSE event
            if let Some(event_end) = self.buffer.find("\n\n") {
                let event_str = self.buffer[..event_end].to_string();
                self.buffer = self.buffer[event_end + 2..].to_string();

                for line in event_str.lines() {
                    if let Some(data) = line.strip_prefix("data:") {
                        let payload = data.trim();
                        if payload.is_empty() {
                            continue;
                        }

                        if payload == "[DONE]" {
                            self.pending_tokens.push_back(Token::final_token("stop"));
                            self.finished = true;
                            continue;
                        }

                        match serde_json::from_str::<Value>(payload) {
                            Ok(Value::Array(items)) => {
                                for item in items {
                                    if let Some(err) = self.queue_payload_tokens(&item) {
                                        self.finished = true;
                                        return Poll::Ready(Some(Err(err)));
                                    }
                                }
                            }
                            Ok(value) => {
                                if let Some(err) = self.queue_payload_tokens(&value) {
                                    self.finished = true;
                                    return Poll::Ready(Some(Err(err)));
                                }
                            }
                            Err(err) => {
                                warn!("Failed to parse Gemini SSE payload: {}", err);
                            }
                        }
                    }
                }

                if let Some(token) = self.pending_tokens.pop_front() {
                    return Poll::Ready(Some(Ok(token)));
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
                if !self.finished {
                    self.finished = true;
                    return Poll::Ready(Some(Ok(Token::final_token("stop"))));
                }
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
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
                "Failed to build Gemini HTTP client with timeouts: {}. Using default client",
                err
            );
            Client::new()
        }
    }
}

fn normalize_key(value: Option<String>, source: &str) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            warn!("{} is set but empty; treating as missing", source);
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_config_api_key(config: &LlmConfig) -> Option<String> {
    let key = config.api_key.trim();
    if key.is_empty() || key.starts_with("ENV:") {
        return None;
    }
    Some(key.to_string())
}

fn preferred_gemini_key_from_env() -> Option<String> {
    normalize_key(env::var("GEMINI_API_KEY").ok(), "GEMINI_API_KEY")
        .or_else(|| normalize_key(env::var("GOOGLE_API_KEY").ok(), "GOOGLE_API_KEY"))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gemini_provider_creation() {
        let provider = GeminiProvider::new();
        assert_eq!(provider.name(), "gemini");
    }

    #[test]
    fn test_gemini_with_api_key() {
        let provider = GeminiProvider::with_api_key("test-key");
        assert!(provider.api_key.is_some());
    }

    #[test]
    fn test_resolve_model_falls_back_to_default() {
        let provider = GeminiProvider::new();
        let config = LlmConfig {
            model: "claude-opus-4-5".to_string(),
            ..Default::default()
        };

        let model = provider.resolve_model(&config);
        assert_eq!(model, DEFAULT_GEMINI_MODEL);
    }
}
