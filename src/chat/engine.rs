//! Chat engine orchestrator.
//!
//! Coordinates LLM interactions, streaming responses, and command execution.

use futures_util::StreamExt;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::config::LlmConfig;
use crate::error::Result;
use crate::state::{AppState, StateUpdate};

use super::command::{execute_intent, CommandResult};
use super::context::build_messages;
use super::intent::StreamBuffer;
use super::llm::{create_provider, LlmProvider};

/// Message types sent from the chat engine to the UI.
#[derive(Debug, Clone)]
pub enum ChatEvent {
    /// A new token from the streaming response.
    Token(String),
    /// The response is complete.
    Complete(String),
    /// A command was executed.
    CommandExecuted(CommandResult),
    /// An error occurred.
    Error(String),
}

/// Chat engine that manages LLM interactions.
pub struct ChatEngine {
    /// The LLM provider.
    provider: Box<dyn LlmProvider>,
    /// LLM configuration.
    config: LlmConfig,
}

impl ChatEngine {
    /// Create a new chat engine with the given configuration.
    pub fn new(config: &LlmConfig) -> Self {
        let provider = create_provider(config);
        info!("Chat engine initialized with {} provider", provider.name());

        Self {
            provider,
            config: config.clone(),
        }
    }

    /// Get the name of the current provider.
    pub fn provider_name(&self) -> &str {
        self.provider.name()
    }

    /// Process a user message and stream the response.
    ///
    /// Returns a channel receiver for streaming events.
    #[must_use = "consume the stream receiver to process chat events"]
    pub async fn process_message(
        &self,
        state: &AppState,
        user_input: &str,
    ) -> Result<mpsc::Receiver<ChatEvent>> {
        let (tx, rx) = mpsc::channel(100);

        // Build messages with context
        let messages = build_messages(state, user_input);

        // Start streaming
        let stream = self
            .provider
            .stream_completion(messages, &self.config)
            .await?;

        // Spawn task to handle streaming
        let tx_clone = tx.clone();
        tokio::spawn(async move {
            let mut buffer = StreamBuffer::new();
            let mut stream = stream;

            while let Some(result) = stream.next().await {
                match result {
                    Ok(token) => {
                        if token.is_final {
                            // Stream complete, parse for commands
                            let parsed = buffer.finalize();

                            // Send complete event
                            let _ = tx_clone
                                .send(ChatEvent::Complete(parsed.display_text.clone()))
                                .await;

                            // Execute any detected commands
                            // Note: We can't mutate state here, commands will be
                            // returned for the caller to execute
                            for intent in parsed.intents {
                                let _ = tx_clone
                                    .send(ChatEvent::CommandExecuted(CommandResult::success(
                                        format!(
                                            "Command detected: {} {:?}",
                                            intent.command, intent.argument
                                        ),
                                    )))
                                    .await;
                            }

                            break;
                        } else {
                            // Buffer the token
                            buffer.push(&token.text);

                            // Send safe display text
                            let safe = buffer.safe_display_text();
                            if !safe.is_empty() {
                                let _ = tx_clone.send(ChatEvent::Token(safe.to_string())).await;
                                buffer.mark_displayed(safe.len());
                            }
                        }
                    }
                    Err(e) => {
                        error!("Streaming error: {}", e);
                        let _ = tx_clone.send(ChatEvent::Error(e.to_string())).await;
                        break;
                    }
                }
            }
        });

        Ok(rx)
    }

    /// Process a message synchronously (non-streaming).
    ///
    /// Collects all tokens and returns the complete response.
    #[must_use = "handle chat processing errors and command outputs"]
    pub async fn process_message_sync(
        &self,
        state: &AppState,
        user_input: &str,
    ) -> Result<(String, Vec<CommandResult>)> {
        let messages = build_messages(state, user_input);
        let mut stream = self
            .provider
            .stream_completion(messages, &self.config)
            .await?;

        let mut buffer = StreamBuffer::new();

        while let Some(result) = stream.next().await {
            match result {
                Ok(token) => {
                    if !token.is_final {
                        buffer.push(&token.text);
                    }
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }

        let parsed = buffer.finalize();
        let mut results = Vec::new();

        // Return intents as results (caller should execute them)
        for intent in &parsed.intents {
            results.push(CommandResult::success(format!(
                "Command: {} {:?}",
                intent.command, intent.argument
            )));
        }

        Ok((parsed.display_text, results))
    }
}

/// Process a user message and update state accordingly.
///
/// This is a convenience function that handles the full flow:
/// 1. Add user message to state
/// 2. Stream LLM response
/// 3. Execute any detected commands
/// 4. Add assistant message to state
#[must_use = "handle chat processing errors and returned assistant text"]
pub async fn process_chat_message(
    engine: &ChatEngine,
    state: &mut AppState,
    user_input: &str,
) -> Result<String> {
    let state_snapshot = state.clone();
    state.send_user_message(user_input.to_string());

    // Get response
    let messages = build_messages(&state_snapshot, user_input);
    let mut stream = engine
        .provider
        .stream_completion(messages, &engine.config)
        .await?;

    let mut buffer = StreamBuffer::new();

    while let Some(result) = stream.next().await {
        match result {
            Ok(token) => {
                if !token.is_final {
                    buffer.push(&token.text);
                }
            }
            Err(e) => {
                state.apply_update(StateUpdate::ChatToken(format!("Error: {}", e)));
                state.apply_update(StateUpdate::ChatDone);
                return Err(e);
            }
        }
    }

    let parsed = buffer.finalize();

    // Execute commands
    for intent in &parsed.intents {
        let result = execute_intent(state, intent);
        if !result.success {
            warn!("Command failed: {}", result.message);
        }
    }

    state.apply_update(StateUpdate::ChatToken(parsed.display_text.clone()));
    state.apply_update(StateUpdate::ChatDone);

    Ok(parsed.display_text)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use futures_util::stream;

    use super::*;
    use crate::chat::llm::{LlmProvider, Message, Token, TokenStream};
    use crate::config::{Config, LlmProvider as LlmProviderType};

    struct RecordingProvider {
        recorded_messages: Arc<Mutex<Vec<Message>>>,
    }

    #[async_trait]
    impl LlmProvider for RecordingProvider {
        fn name(&self) -> &str {
            "recording"
        }

        fn validate_config(&self) -> Result<()> {
            Ok(())
        }

        fn has_credentials(&self) -> bool {
            true
        }

        async fn stream_completion(
            &self,
            messages: Vec<Message>,
            _config: &LlmConfig,
        ) -> Result<TokenStream> {
            let mut recorded = self
                .recorded_messages
                .lock()
                .expect("recorded_messages lock poisoned");
            *recorded = messages;
            drop(recorded);

            Ok(Box::pin(stream::iter(vec![
                Ok(Token::new("ack")),
                Ok(Token::final_token("stop")),
            ])))
        }
    }

    #[tokio::test]
    async fn test_chat_engine_creation() {
        let config = LlmConfig {
            provider: LlmProviderType::Mock,
            ..Default::default()
        };
        let engine = ChatEngine::new(&config);
        assert_eq!(engine.provider_name(), "mock");
    }

    #[tokio::test]
    async fn test_process_message_sync() {
        let config = LlmConfig {
            provider: LlmProviderType::Mock,
            ..Default::default()
        };
        let engine = ChatEngine::new(&config);
        let state = AppState::new(Config::default());

        let (response, _) = engine
            .process_message_sync(&state, "What is BTC price?")
            .await
            .unwrap();

        assert!(!response.is_empty());
        assert!(response.contains("[MOCK]"));
    }

    #[tokio::test]
    async fn test_process_chat_message() {
        let config = LlmConfig {
            provider: LlmProviderType::Mock,
            ..Default::default()
        };
        let engine = ChatEngine::new(&config);
        let mut state = AppState::new(Config::default());

        let response = process_chat_message(&engine, &mut state, "Hello!")
            .await
            .unwrap();

        assert!(!response.is_empty());
        assert_eq!(state.chat_messages.len(), 2); // User + Assistant
        assert!(state.chat_messages[0].is_user);
        assert!(!state.chat_messages[1].is_user);
    }

    #[tokio::test]
    async fn test_process_chat_message_does_not_duplicate_user_input_in_context() {
        let config = LlmConfig {
            provider: LlmProviderType::Mock,
            ..Default::default()
        };
        let recorded_messages = Arc::new(Mutex::new(Vec::new()));
        let engine = ChatEngine {
            provider: Box::new(RecordingProvider {
                recorded_messages: Arc::clone(&recorded_messages),
            }),
            config,
        };
        let mut state = AppState::new(Config::default());

        let _ = process_chat_message(&engine, &mut state, "Hello duplicate check")
            .await
            .unwrap();

        let sent = recorded_messages
            .lock()
            .expect("recorded_messages lock poisoned");
        let user_occurrences = sent
            .iter()
            .filter(|msg| msg.content == "Hello duplicate check")
            .count();
        assert_eq!(user_occurrences, 1);
    }
}
