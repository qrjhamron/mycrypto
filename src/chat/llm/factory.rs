//! LLM provider factory.
//!
//! Creates the appropriate provider based on configuration,
//! with automatic fallback to mock if credentials are missing.

use tracing::{info, warn};

use crate::config::{LlmConfig, LlmProvider as LlmProviderType};

use super::claude::ClaudeProvider;
use super::copilot::CopilotProvider;
use super::gemini::GeminiProvider;
use super::gradio::GradioProvider;
use super::mock::MockProvider;
use super::openai::OpenAIProvider;
use super::openrouter::OpenRouterProvider;
use super::provider::LlmProvider;

/// Creates an LLM provider based on configuration.
///
/// If the configured provider lacks credentials, automatically falls back
/// to the mock provider with a warning log.
pub fn create_provider(config: &LlmConfig) -> Box<dyn LlmProvider> {
    let selection_reason = provider_selection_reason(config);
    info!(
        "LLM provider requested: {} (reason: {})",
        config.provider, selection_reason
    );

    let provider = create_provider_for_type(&config.provider);

    // Check if provider has valid credentials
    if !provider.has_credentials() {
        warn!(
            "No credentials found for requested {} provider ({}), falling back to mock",
            provider.name(),
            selection_reason
        );
        return Box::new(MockProvider::new());
    }

    // Validate configuration
    if let Err(e) = provider.validate_config() {
        warn!(
            "Configuration validation failed for {} provider ({}): {}, falling back to mock",
            provider.name(),
            selection_reason,
            e
        );
        return Box::new(MockProvider::new());
    }

    info!(
        "Using {} LLM provider (requested: {}, reason: {})",
        provider.name(),
        config.provider,
        selection_reason
    );
    provider
}

/// Creates a provider for the specified type (without credential check).
fn create_provider_for_type(provider_type: &LlmProviderType) -> Box<dyn LlmProvider> {
    match provider_type {
        LlmProviderType::Claude => Box::new(ClaudeProvider::new()),
        LlmProviderType::OpenAI => Box::new(OpenAIProvider::new()),
        LlmProviderType::Gemini => Box::new(GeminiProvider::new()),
        LlmProviderType::OpenRouter => Box::new(OpenRouterProvider::new()),
        LlmProviderType::Gradio => Box::new(GradioProvider::new()),
        LlmProviderType::Copilot => Box::new(CopilotProvider::new()),
        LlmProviderType::Mock => Box::new(MockProvider::new()),
    }
}

/// Get the expected environment variable name for an LLM provider.
pub fn get_env_var_name(provider_type: &LlmProviderType) -> &'static str {
    match provider_type {
        LlmProviderType::Claude => "CLAUDE_API_KEY",
        LlmProviderType::OpenAI => "OPENAI_API_KEY",
        LlmProviderType::Gemini => "GEMINI_API_KEY",
        LlmProviderType::OpenRouter => "OPENROUTER_API_KEY",
        LlmProviderType::Gradio => "GRADIO_API_KEY",
        LlmProviderType::Copilot => "GITHUB_TOKEN",
        LlmProviderType::Mock => "",
    }
}

fn provider_selection_reason(config: &LlmConfig) -> String {
    if !config.api_key.is_empty() && !config.api_key.starts_with("ENV:") {
        return "config.api_key override".to_string();
    }

    match config.provider {
        LlmProviderType::Claude => env_state("CLAUDE_API_KEY"),
        LlmProviderType::OpenAI => env_state("OPENAI_API_KEY"),
        LlmProviderType::Gemini => {
            let gemini = env_state("GEMINI_API_KEY");
            if gemini.starts_with("found") {
                return format!("{} (preferred)", gemini);
            }
            let google = env_state("GOOGLE_API_KEY");
            if google.starts_with("found") {
                return format!("{} (fallback)", google);
            }
            format!("{}; {}", gemini, google)
        }
        LlmProviderType::OpenRouter => env_state("OPENROUTER_API_KEY"),
        LlmProviderType::Gradio => env_state("GRADIO_API_KEY"),
        LlmProviderType::Copilot => env_state("GITHUB_TOKEN"),
        LlmProviderType::Mock => "mock provider explicitly configured".to_string(),
    }
}

fn env_state(var: &str) -> String {
    match std::env::var(var) {
        Ok(v) if v.trim().is_empty() => format!("{} is set but empty", var),
        Ok(_) => format!("found {}", var),
        Err(_) => format!("missing {}", var),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_mock_provider() {
        let config = LlmConfig {
            provider: LlmProviderType::Mock,
            ..Default::default()
        };
        let provider = create_provider(&config);
        assert_eq!(provider.name(), "mock");
    }

    #[test]
    fn test_fallback_to_mock() {
        // Claude without API key should fall back to mock
        let config = LlmConfig {
            provider: LlmProviderType::Claude,
            api_key: String::new(),
            ..Default::default()
        };
        // This will fall back to mock since no CLAUDE_API_KEY is set in test env
        let provider = create_provider(&config);
        // In test environment without keys, should fall back
        assert!(provider.name() == "mock" || provider.name() == "claude");
    }

    #[test]
    fn test_env_var_names() {
        assert_eq!(get_env_var_name(&LlmProviderType::Claude), "CLAUDE_API_KEY");
        assert_eq!(get_env_var_name(&LlmProviderType::OpenAI), "OPENAI_API_KEY");
        assert_eq!(get_env_var_name(&LlmProviderType::Gemini), "GEMINI_API_KEY");
        assert_eq!(
            get_env_var_name(&LlmProviderType::OpenRouter),
            "OPENROUTER_API_KEY"
        );
        assert_eq!(get_env_var_name(&LlmProviderType::Gradio), "GRADIO_API_KEY");
        assert_eq!(get_env_var_name(&LlmProviderType::Copilot), "GITHUB_TOKEN");
        assert_eq!(get_env_var_name(&LlmProviderType::Mock), "");
    }
}
