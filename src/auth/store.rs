//! Authentication provider and status types.

use std::collections::HashMap;
use std::time::Instant;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Supported authentication providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthProvider {
    /// GitHub OAuth device flow.
    GitHub,
    /// Anthropic API key.
    Anthropic,
    /// OpenAI API key.
    OpenAI,
    /// Gemini API key.
    Gemini,
    /// OpenRouter API key.
    OpenRouter,
    /// Gradio space URL + optional token.
    Gradio,
}

impl AuthProvider {
    /// Ordered list for UI navigation.
    pub const ALL: [Self; 6] = [
        Self::GitHub,
        Self::Anthropic,
        Self::OpenAI,
        Self::Gemini,
        Self::OpenRouter,
        Self::Gradio,
    ];

    /// Human-readable provider name.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::GitHub => "GitHub Copilot",
            Self::Anthropic => "Anthropic",
            Self::OpenAI => "OpenAI",
            Self::Gemini => "Gemini",
            Self::OpenRouter => "OpenRouter",
            Self::Gradio => "Gradio",
        }
    }

    /// Authentication method description.
    pub fn auth_method(&self) -> &'static str {
        match self {
            Self::GitHub => "Device Flow (no browser redirect needed)",
            Self::Anthropic => "API key (paste directly)",
            Self::OpenAI => "API key (paste directly)",
            Self::Gemini => "API key (paste directly)",
            Self::OpenRouter => "API key (paste directly)",
            Self::Gradio => "Space URL + optional token",
        }
    }

    /// Env var associated with this provider (if applicable).
    pub fn env_var(&self) -> Option<&'static str> {
        match self {
            Self::GitHub => Some("GITHUB_TOKEN"),
            Self::Anthropic => Some("CLAUDE_API_KEY"),
            Self::OpenAI => Some("OPENAI_API_KEY"),
            Self::Gemini => Some("GEMINI_API_KEY"),
            Self::OpenRouter => Some("OPENROUTER_API_KEY"),
            Self::Gradio => Some("GRADIO_API_KEY"),
        }
    }

    /// Returns true if the provider uses direct API key input.
    pub fn is_api_key_provider(&self) -> bool {
        matches!(
            self,
            Self::Anthropic | Self::OpenAI | Self::Gemini | Self::OpenRouter
        )
    }
}

impl std::fmt::Display for AuthProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Authentication status for a provider.
#[derive(Debug, Clone, Default)]
pub enum AuthStatus {
    /// No auth configured.
    #[default]
    NotConfigured,
    /// Device flow is pending user action.
    PendingDevice {
        /// User code to enter on verification page.
        user_code: String,
        /// Verification URI to visit.
        verification_uri: String,
        /// Expiration instant.
        expires_at: Instant,
        /// Poll interval in seconds.
        interval_secs: u64,
    },
    /// GitHub auth succeeded.
    AuthenticatedGitHub {
        /// Authenticated username.
        username: String,
        /// Access token.
        token: String,
        /// Created timestamp.
        created_at: DateTime<Utc>,
    },
    /// API key configured.
    ApiKeyConfigured {
        /// Masked preview.
        masked: String,
    },
    /// Gradio configured.
    GradioConfigured {
        /// Space URL.
        space_url: String,
        /// Optional masked token preview.
        token_masked: Option<String>,
    },
    /// Most recent error.
    Error(String),
}

impl AuthStatus {
    /// Returns true when provider has usable credentials.
    pub fn is_configured(&self) -> bool {
        matches!(
            self,
            Self::AuthenticatedGitHub { .. }
                | Self::ApiKeyConfigured { .. }
                | Self::GradioConfigured { .. }
        )
    }
}

/// Build a default auth map for all providers.
pub fn default_auth_state() -> HashMap<AuthProvider, AuthStatus> {
    let mut map = HashMap::with_capacity(AuthProvider::ALL.len());
    for provider in AuthProvider::ALL {
        map.insert(provider, AuthStatus::NotConfigured);
    }
    map
}

/// Mask a secret for display.
pub fn mask_secret(secret: &str) -> String {
    if secret.is_empty() {
        return String::new();
    }

    let suffix: String = secret
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    format!("{}{}", "●".repeat(8), suffix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_secret_uses_last_four_characters_only() {
        assert_eq!(mask_secret("abcd1234efgh5678"), "●●●●●●●●5678");
        assert_eq!(mask_secret("xyz"), "●●●●●●●●xyz");
        assert_eq!(mask_secret(""), "");
    }
}
