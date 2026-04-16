//! Chat engine module.
//!
//! This module handles:
//! - User message processing
//! - Context building for LLM
//! - LLM provider abstraction (Claude, OpenAI, Gemini, etc.)
//! - Intent parsing and command execution
//! - Streaming responses

pub mod command;
pub mod context;
pub mod engine;
pub mod intent;
pub mod llm;
pub mod team;

// Re-export commonly used items
pub use engine::{process_chat_message, ChatEngine, ChatEvent};
pub use intent::{parse_response, DetectedIntent, ParsedResponse};
pub use llm::{LlmProvider, Message, Role, Token};
