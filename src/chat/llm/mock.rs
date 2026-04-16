//! Mock LLM provider for testing.
//!
//! This provider returns deterministic responses without making any API calls.
//! It's useful for testing and when no API keys are configured.

use std::pin::Pin;
use std::task::{Context, Poll};

use async_trait::async_trait;
use futures_util::Stream;

use crate::config::LlmConfig;
use crate::error::Result;

use super::provider::{LlmProvider, Message, Role, Token, TokenStream};

/// Mock LLM provider that returns deterministic responses.
pub struct MockProvider {
    /// Response delay in milliseconds (simulates streaming).
    delay_ms: u64,
}

impl MockProvider {
    /// Create a new mock provider.
    pub fn new() -> Self {
        Self { delay_ms: 50 }
    }

    /// Create a mock provider with custom delay.
    #[cfg(test)]
    pub fn with_delay(delay_ms: u64) -> Self {
        Self { delay_ms }
    }

    /// Generate a mock response based on user input.
    fn generate_response(&self, messages: &[Message]) -> String {
        // Find the last user message
        let user_message = messages
            .iter()
            .rev()
            .find(|m| m.role == Role::User)
            .map(|m| m.content.as_str())
            .unwrap_or("");

        let lower = user_message.to_lowercase();

        // Pattern match for different queries
        if lower.contains("price") || lower.contains("btc") || lower.contains("bitcoin") {
            "[MOCK] Bitcoin is currently trading around $67,500. The market shows \
             moderate volatility with a slight bullish bias based on recent price action. \
             Key support at $65,000, resistance at $70,000."
                .to_string()
        } else if lower.contains("eth") || lower.contains("ethereum") {
            "[MOCK] Ethereum is trading near $3,450. Gas fees are moderate. \
             The ETH/BTC ratio has been consolidating. Watch for breakout above $3,600."
                .to_string()
        } else if lower.contains("portfolio") || lower.contains("position") {
            "[MOCK] Your portfolio is performing well. Current allocation looks balanced. \
             Consider reviewing your risk parameters if you want to adjust exposure."
                .to_string()
        } else if lower.contains("signal") || lower.contains("trade") {
            "[MOCK] Based on current market conditions, I see a potential opportunity. \
             However, always verify signals against your own analysis. \
             [COMMAND:status] to see current positions."
                .to_string()
        } else if lower.contains("help") {
            "[MOCK] I can help you with:\n\
             - Market analysis and price updates\n\
             - Portfolio review and suggestions\n\
             - Signal explanations\n\
             - Risk management advice\n\
             Just ask me anything about crypto trading!"
                .to_string()
        } else if lower.contains("pause") {
            "[MOCK] I understand you want to pause the agent. \
             [COMMAND:pause] - This will stop new signal generation."
                .to_string()
        } else if lower.contains("resume") || lower.contains("start") {
            "[MOCK] Ready to resume trading. \
             [COMMAND:resume] - This will restart signal generation."
                .to_string()
        } else if lower.is_empty() {
            "[MOCK] I'm here to help with your crypto trading analysis. \
             Ask me about prices, signals, or your portfolio!"
                .to_string()
        } else {
            format!(
                "[MOCK] I received your message about '{}'. \
                 In a real scenario, I would analyze market data and provide insights. \
                 Currently running in mock mode without API access.",
                if user_message.len() > 50 {
                    format!("{}...", &user_message[..50])
                } else {
                    user_message.to_string()
                }
            )
        }
    }
}

impl Default for MockProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LlmProvider for MockProvider {
    fn name(&self) -> &str {
        "mock"
    }

    fn validate_config(&self) -> Result<()> {
        // Mock provider always validates successfully
        Ok(())
    }

    fn has_credentials(&self) -> bool {
        // Mock provider doesn't need credentials
        true
    }

    async fn stream_completion(
        &self,
        messages: Vec<Message>,
        _config: &LlmConfig,
    ) -> Result<TokenStream> {
        let response = self.generate_response(&messages);
        let delay_ms = self.delay_ms;

        // Split response into words for streaming effect
        let words: Vec<String> = response.split_whitespace().map(|s| s.to_string()).collect();

        Ok(Box::pin(MockStream::new(words, delay_ms)))
    }
}

/// Stream that yields mock tokens.
struct MockStream {
    words: Vec<String>,
    index: usize,
    delay_ms: u64,
    finished: bool,
}

impl MockStream {
    fn new(words: Vec<String>, delay_ms: u64) -> Self {
        Self {
            words,
            index: 0,
            delay_ms,
            finished: false,
        }
    }
}

impl Stream for MockStream {
    type Item = Result<Token>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.finished {
            return Poll::Ready(None);
        }

        if self.index >= self.words.len() {
            self.finished = true;
            return Poll::Ready(Some(Ok(Token::final_token("stop"))));
        }

        // Add space before word (except first)
        let text = if self.index == 0 {
            self.words[self.index].clone()
        } else {
            format!(" {}", self.words[self.index])
        };

        self.index += 1;

        // Simulate delay (in real impl this would be async sleep)
        if self.delay_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(self.delay_ms));
        }

        Poll::Ready(Some(Ok(Token::new(text))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;

    #[tokio::test]
    async fn test_mock_provider_basics() {
        let provider = MockProvider::new();
        assert_eq!(provider.name(), "mock");
        assert!(provider.validate_config().is_ok());
        assert!(provider.has_credentials());
    }

    #[tokio::test]
    async fn test_mock_streaming() {
        let provider = MockProvider::with_delay(0);
        let config = LlmConfig::default();
        let messages = vec![Message::user("What is BTC price?")];

        let mut stream = provider.stream_completion(messages, &config).await.unwrap();

        let mut full_response = String::new();
        while let Some(result) = stream.next().await {
            let token = result.unwrap();
            if !token.is_final {
                full_response.push_str(&token.text);
            }
        }

        assert!(full_response.contains("[MOCK]"));
        assert!(full_response.to_lowercase().contains("bitcoin"));
    }

    #[tokio::test]
    async fn test_mock_command_injection() {
        let provider = MockProvider::with_delay(0);
        let config = LlmConfig::default();
        let messages = vec![Message::user("pause the agent")];

        let mut stream = provider.stream_completion(messages, &config).await.unwrap();

        let mut full_response = String::new();
        while let Some(result) = stream.next().await {
            let token = result.unwrap();
            if !token.is_final {
                full_response.push_str(&token.text);
            }
        }

        assert!(full_response.contains("[COMMAND:pause]"));
    }
}
