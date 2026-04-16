//! Model selector state container.
//!
//! The Model page rendering is handled in `pages.rs`.

use crate::config::LlmProvider;

/// Models available for each provider.
pub mod models {
    pub const CLAUDE: &[&str] = &["claude-opus-4-5", "claude-sonnet-4-5", "claude-haiku-4-5"];
    pub const OPENAI: &[&str] = &["gpt-4o", "gpt-4o-mini", "gpt-4-turbo", "o1", "o1-mini"];
    pub const GEMINI: &[&str] = &["gemini-2.0-flash", "gemini-1.5-pro", "gemini-1.5-flash"];
    pub const COPILOT: &[&str] = &["gpt-4o"];
    pub const OPENROUTER: &[&str] = &[
        "anthropic/claude-sonnet-4-5",
        "openai/gpt-4o",
        "google/gemini-2.0-flash-001",
    ];
    pub const GRADIO: &[&str] = &[
        "https://huggingface.co/spaces/HuggingFaceH4/zephyr-chat",
        "https://huggingface.co/spaces/Qwen/Qwen2.5-Max-Demo",
        "https://huggingface.co/spaces/microsoft/Phi-4-multimodal-instruct",
    ];
    pub const MOCK: &[&str] = &["mock-v1"];
}

/// Model descriptions.
pub fn model_description(model: &str) -> &'static str {
    match model {
        "claude-opus-4-5" => "(most capable)",
        "claude-sonnet-4-5" => "(balanced)",
        "claude-haiku-4-5" => "(fast)",
        "gpt-4o" => "(flagship)",
        "gpt-4o-mini" => "(fast)",
        "gpt-4-turbo" => "(powerful)",
        "o1" => "(reasoning)",
        "o1-mini" => "(reasoning fast)",
        "gemini-2.0-flash" => "(newest)",
        "gemini-1.5-pro" => "(powerful)",
        "gemini-1.5-flash" => "(fast)",
        "mock-v1" => "(offline testing)",
        _ => "",
    }
}

/// Model selector state.
#[derive(Debug, Clone)]
pub struct ModelSelector {
    /// Currently selected provider.
    pub provider: LlmProvider,
    /// Currently selected model.
    pub model: String,
    /// Provider selection index.
    pub provider_index: usize,
    /// Model selection index.
    pub model_index: usize,
    /// API key status.
    pub api_key_set: bool,
    /// Connection status.
    pub connected: bool,
}

impl Default for ModelSelector {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelSelector {
    /// Create a new model selector.
    pub fn new() -> Self {
        Self {
            provider: LlmProvider::Mock,
            model: "mock-v1".to_string(),
            provider_index: 6,
            model_index: 0,
            api_key_set: false,
            connected: false,
        }
    }

    /// Initialize from current config.
    pub fn from_config(provider: LlmProvider, model: &str, api_key_set: bool) -> Self {
        let provider_index = match provider {
            LlmProvider::Claude => 0,
            LlmProvider::OpenAI => 1,
            LlmProvider::Gemini => 2,
            LlmProvider::OpenRouter => 3,
            LlmProvider::Gradio => 4,
            LlmProvider::Copilot => 5,
            LlmProvider::Mock => 6,
        };

        let model_index = Self::get_models_for_provider(&provider)
            .iter()
            .position(|&m| m == model)
            .unwrap_or(0);

        let connected = api_key_set || matches!(provider, LlmProvider::Mock);

        Self {
            provider,
            model: model.to_string(),
            provider_index,
            model_index,
            api_key_set,
            connected,
        }
    }

    /// Get all providers.
    pub fn providers() -> &'static [LlmProvider] {
        &[
            LlmProvider::Claude,
            LlmProvider::OpenAI,
            LlmProvider::Gemini,
            LlmProvider::OpenRouter,
            LlmProvider::Gradio,
            LlmProvider::Copilot,
            LlmProvider::Mock,
        ]
    }

    /// Get models for a provider.
    pub fn get_models_for_provider(provider: &LlmProvider) -> &'static [&'static str] {
        match provider {
            LlmProvider::Claude => models::CLAUDE,
            LlmProvider::OpenAI => models::OPENAI,
            LlmProvider::Gemini => models::GEMINI,
            LlmProvider::OpenRouter => models::OPENROUTER,
            LlmProvider::Gradio => models::GRADIO,
            LlmProvider::Copilot => models::COPILOT,
            LlmProvider::Mock => models::MOCK,
        }
    }

    /// Get current provider's models.
    pub fn current_models(&self) -> &'static [&'static str] {
        Self::get_models_for_provider(&self.provider)
    }

    /// Move to next provider.
    pub fn next_provider(&mut self) {
        let providers = Self::providers();
        self.provider_index = (self.provider_index + 1) % providers.len();
        self.provider = providers[self.provider_index].clone();
        self.model_index = 0;
        self.update_model();
    }

    /// Move to previous provider.
    pub fn prev_provider(&mut self) {
        let providers = Self::providers();
        if self.provider_index == 0 {
            self.provider_index = providers.len() - 1;
        } else {
            self.provider_index -= 1;
        }
        self.provider = providers[self.provider_index].clone();
        self.model_index = 0;
        self.update_model();
    }

    /// Move to next model.
    pub fn next_model(&mut self) {
        let models = self.current_models();
        if !models.is_empty() {
            self.model_index = (self.model_index + 1) % models.len();
            self.update_model();
        }
    }

    /// Move to previous model.
    pub fn prev_model(&mut self) {
        let models = self.current_models();
        if !models.is_empty() {
            if self.model_index == 0 {
                self.model_index = models.len() - 1;
            } else {
                self.model_index -= 1;
            }
            self.update_model();
        }
    }

    /// Update model from current selection.
    fn update_model(&mut self) {
        let models = self.current_models();
        if !models.is_empty() {
            self.model = models[self.model_index].to_string();
        }
    }

    /// Update auth status for currently selected provider.
    pub fn set_provider_authenticated(&mut self, authenticated: bool) {
        self.api_key_set = authenticated;
        self.connected = authenticated || matches!(self.provider, LlmProvider::Mock);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_selector_creation() {
        let ms = ModelSelector::new();
        assert_eq!(ms.provider, LlmProvider::Mock);
    }

    #[test]
    fn test_provider_navigation() {
        let mut ms = ModelSelector::new();
        let initial = ms.provider_index;
        ms.next_provider();
        assert_ne!(ms.provider_index, initial);
        ms.prev_provider();
        assert_eq!(ms.provider_index, initial);
    }
}
