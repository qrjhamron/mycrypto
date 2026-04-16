//! LLM provider trait definition.
//!
//! This module defines the common interface that all LLM providers must implement.

use std::pin::Pin;

use async_trait::async_trait;
use futures_util::Stream;

use crate::config::LlmConfig;
use crate::error::Result;

/// A single token from a streaming response.
#[derive(Debug, Clone)]
pub struct Token {
    /// The text content of this token.
    pub text: String,
    /// Whether this is the final token (stream complete).
    pub is_final: bool,
    /// Optional finish reason (e.g., "stop", "length").
    pub finish_reason: Option<String>,
}

impl Token {
    /// Create a new token with the given text.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            is_final: false,
            finish_reason: None,
        }
    }

    /// Create a final token marking stream completion.
    pub fn final_token(reason: impl Into<String>) -> Self {
        Self {
            text: String::new(),
            is_final: true,
            finish_reason: Some(reason.into()),
        }
    }
}

/// Role in a chat conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// System message (sets AI behavior).
    System,
    /// User message.
    User,
    /// Assistant/AI response.
    Assistant,
}

impl Role {
    /// Convert to string for API calls.
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
        }
    }
}

/// A message in a chat conversation.
#[derive(Debug, Clone)]
pub struct Message {
    /// The role of the message sender.
    pub role: Role,
    /// The message content.
    pub content: String,
}

impl Message {
    /// Create a system message.
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
        }
    }

    /// Create a user message.
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
        }
    }

    /// Create an assistant message.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
        }
    }
}

/// Type alias for the streaming token stream.
pub type TokenStream = Pin<Box<dyn Stream<Item = Result<Token>> + Send>>;

/// Trait that all LLM providers must implement.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Returns the provider name (e.g., "claude", "openai").
    #[must_use]
    fn name(&self) -> &str;

    /// Validates the configuration (checks API key, etc.).
    #[must_use = "handle provider configuration validation errors"]
    fn validate_config(&self) -> Result<()>;

    /// Stream a completion response token by token.
    ///
    /// # Arguments
    /// * `messages` - The conversation history
    /// * `config` - LLM configuration settings
    ///
    /// # Returns
    /// An async stream of tokens that can be consumed as they arrive.
    #[must_use]
    async fn stream_completion(
        &self,
        messages: Vec<Message>,
        config: &LlmConfig,
    ) -> Result<TokenStream>;

    /// Check if the provider has valid credentials configured.
    #[must_use]
    fn has_credentials(&self) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_creation() {
        let token = Token::new("hello");
        assert_eq!(token.text, "hello");
        assert!(!token.is_final);
        assert!(token.finish_reason.is_none());
    }

    #[test]
    fn test_final_token() {
        let token = Token::final_token("stop");
        assert!(token.text.is_empty());
        assert!(token.is_final);
        assert_eq!(token.finish_reason, Some("stop".to_string()));
    }

    #[test]
    fn test_message_roles() {
        let sys = Message::system("You are helpful");
        assert_eq!(sys.role, Role::System);

        let user = Message::user("Hello");
        assert_eq!(user.role, Role::User);

        let asst = Message::assistant("Hi there");
        assert_eq!(asst.role, Role::Assistant);
    }

    #[test]
    fn test_role_as_str() {
        assert_eq!(Role::System.as_str(), "system");
        assert_eq!(Role::User.as_str(), "user");
        assert_eq!(Role::Assistant.as_str(), "assistant");
    }
}
