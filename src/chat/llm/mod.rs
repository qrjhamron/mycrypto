//! LLM provider abstraction layer.
//!
//! This module contains:
//! - Provider trait definition
//! - All provider implementations (Claude, OpenAI, Gemini, etc.)
//! - Factory for creating providers from config

pub mod claude;
pub mod copilot;
pub mod factory;
pub mod gemini;
pub mod gradio;
pub mod mock;
pub mod openai;
pub mod openrouter;
pub mod openrouter_models;
pub mod provider;

// Re-export commonly used items
pub use factory::create_provider;
pub use provider::{LlmProvider, Message, Role, Token, TokenStream};
