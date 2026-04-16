//! Chat pipeline orchestration and diagnostics.
//!
//! This module centralizes provider routing diagnostics, streaming metadata,
//! and normalized stream parsing concerns so chat orchestration can emit
//! end-to-end trace points without duplicating glue logic in each provider.

use std::env;

use crate::config::{LlmConfig, LlmProvider};

/// Metadata emitted when selecting a provider route.
#[derive(Debug, Clone)]
pub struct ProviderRoute {
    /// Provider requested by configuration.
    pub requested_provider: LlmProvider,
    /// Provider selected by factory/runtime.
    pub selected_provider: String,
    /// Whether selected provider has credentials configured.
    pub has_credentials: bool,
    /// Human-readable route decision reason.
    pub selection_reason: String,
}

impl ProviderRoute {
    /// Builds route diagnostics from config and selected provider runtime state.
    #[must_use]
    pub fn from_config(
        config: &LlmConfig,
        selected_provider_name: impl Into<String>,
        has_credentials: bool,
    ) -> Self {
        Self {
            requested_provider: config.provider.clone(),
            selected_provider: selected_provider_name.into(),
            has_credentials,
            selection_reason: provider_selection_reason(config),
        }
    }
}

/// Returns a concise description of provider selection inputs.
#[must_use]
pub fn provider_selection_reason(config: &LlmConfig) -> String {
    if !config.api_key.is_empty() && !config.api_key.starts_with("ENV:") {
        return "config.api_key override".to_string();
    }

    match config.provider {
        LlmProvider::Claude => env_state("CLAUDE_API_KEY"),
        LlmProvider::OpenAI => env_state("OPENAI_API_KEY"),
        LlmProvider::Gemini => {
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
        LlmProvider::OpenRouter => env_state("OPENROUTER_API_KEY"),
        LlmProvider::Gradio => env_state("GRADIO_API_KEY"),
        LlmProvider::Copilot => env_state("GITHUB_TOKEN"),
        LlmProvider::Mock => "mock provider explicitly configured".to_string(),
    }
}

fn env_state(var: &str) -> String {
    match env::var(var) {
        Ok(v) if v.trim().is_empty() => format!("{} is set but empty", var),
        Ok(_) => format!("found {}", var),
        Err(_) => format!("missing {}", var),
    }
}

/// Finds the first complete SSE event delimiter in a buffer.
///
/// Supports LF and CRLF separators.
#[must_use]
pub fn find_sse_event_end(buffer: &str) -> Option<(usize, usize)> {
    let lf = buffer.find("\n\n").map(|pos| (pos, 2));
    let crlf = buffer.find("\r\n\r\n").map(|pos| (pos, 4));

    [lf, crlf].into_iter().flatten().min_by_key(|(pos, _)| *pos)
}

/// Extracts an SSE `data:` payload from a single event line.
#[must_use]
pub fn sse_data_payload(line: &str) -> Option<&str> {
    line.strip_prefix("data:")
        .map(|rest| rest.trim_start_matches(' '))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_sse_event_end_supports_lf_and_crlf() {
        assert_eq!(find_sse_event_end("data:x\n\nnext"), Some((6, 2)));
        assert_eq!(find_sse_event_end("data:x\r\n\r\nnext"), Some((6, 4)));
    }

    #[test]
    fn test_sse_data_payload_supports_data_with_or_without_space() {
        assert_eq!(sse_data_payload("data:{\"k\":1}"), Some("{\"k\":1}"));
        assert_eq!(sse_data_payload("data: {\"k\":1}"), Some("{\"k\":1}"));
        assert_eq!(sse_data_payload("event: message"), None);
    }
}
