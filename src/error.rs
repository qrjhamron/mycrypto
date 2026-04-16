//! Unified error types for the mycrypto application.
//!
//! This module defines all domain-specific errors using `thiserror`.
//! Every error is typed, descriptive, and provides context for debugging.
//!
//! # Design Principles
//! - No stringly-typed errors (no `String` error messages without context)
//! - Each domain (config, data, trading, chat, tui) has its own error variant
//! - Errors are propagated via `?` with clear chains
//! - `anyhow` is used only in `main.rs` for top-level error handling

use thiserror::Error;

/// The unified error type for all mycrypto operations.
///
/// This enum covers all possible failure modes in the application,
/// organized by domain. Each variant wraps domain-specific errors
/// or provides structured context.
#[derive(Error, Debug)]
pub enum MycryptoError {
    // ─────────────────────────────────────────────────────────────
    // Configuration Errors
    // ─────────────────────────────────────────────────────────────
    /// Failed to read the configuration file from disk.
    #[error("failed to read config file at '{path}': {source}")]
    ConfigRead {
        path: String,
        #[source]
        source: std::io::Error,
    },

    /// Failed to parse the TOML configuration file.
    #[error("failed to parse config file: {0}")]
    ConfigParse(#[from] toml::de::Error),

    /// Configuration validation failed (e.g., invalid values, missing required fields).
    #[error("config validation failed: {0}")]
    ConfigValidation(String),

    /// Environment variable referenced in config is not set.
    #[error("environment variable '{name}' not found (referenced in config key '{config_key}')")]
    EnvVarNotFound { name: String, config_key: String },

    /// Environment variable has invalid format.
    #[error("environment variable '{name}' has invalid value: {reason}")]
    EnvVarInvalid { name: String, reason: String },

    // ─────────────────────────────────────────────────────────────
    // Market Data Errors
    // ─────────────────────────────────────────────────────────────
    /// WebSocket connection to market data feed failed.
    #[error("websocket connection failed to '{url}': {reason}")]
    WebSocketConnection { url: String, reason: String },

    /// WebSocket message parsing failed.
    #[error("failed to parse market data message: {0}")]
    MarketDataParse(String),

    /// Market data feed disconnected unexpectedly.
    #[error("market data feed disconnected: {reason}")]
    FeedDisconnected { reason: String },

    /// Subscription to a trading pair failed.
    #[error("failed to subscribe to pair '{pair}': {reason}")]
    SubscriptionFailed { pair: String, reason: String },

    // ─────────────────────────────────────────────────────────────
    // Trading Engine Errors
    // ─────────────────────────────────────────────────────────────
    /// Signal generation failed.
    #[error("signal generation failed for '{pair}': {reason}")]
    SignalGeneration { pair: String, reason: String },

    /// Risk check rejected the trade.
    #[error("risk check rejected trade: {reason}")]
    RiskRejection { reason: String },

    /// Position operation failed (open/close/update).
    #[error("position operation failed: {0}")]
    PositionOperation(String),

    /// Insufficient balance for the requested operation.
    #[error("insufficient balance: required {required}, available {available}")]
    InsufficientBalance {
        required: rust_decimal::Decimal,
        available: rust_decimal::Decimal,
    },

    /// Trading pair is blacklisted.
    #[error("pair '{pair}' is blacklisted: {reason}")]
    PairBlacklisted { pair: String, reason: String },

    // ─────────────────────────────────────────────────────────────
    // Chat & LLM Errors
    // ─────────────────────────────────────────────────────────────
    /// LLM API request failed.
    #[error("LLM API request failed: {0}")]
    LlmRequest(String),

    /// LLM response parsing failed.
    #[error("failed to parse LLM response: {0}")]
    LlmResponseParse(String),

    /// LLM API rate limit exceeded.
    #[error("LLM API rate limit exceeded, retry after {retry_after_secs}s")]
    LlmRateLimit { retry_after_secs: u64 },

    /// LLM API authentication failed.
    #[error("LLM API authentication failed: {0}")]
    LlmAuth(String),

    /// Context building for LLM failed.
    #[error("failed to build LLM context: {0}")]
    ContextBuild(String),

    /// Command parsing from LLM response failed.
    #[error("failed to parse command from response: {0}")]
    CommandParse(String),

    // ─────────────────────────────────────────────────────────────
    // TUI Errors
    // ─────────────────────────────────────────────────────────────
    /// Terminal initialization failed.
    #[error("failed to initialize terminal: {0}")]
    TerminalInit(String),

    /// Terminal rendering failed.
    #[error("rendering failed: {0}")]
    Render(String),

    /// Keyboard event handling failed.
    #[error("keyboard event error: {0}")]
    KeyboardEvent(String),

    // ─────────────────────────────────────────────────────────────
    // Channel & Concurrency Errors
    // ─────────────────────────────────────────────────────────────
    /// Channel send operation failed (receiver dropped).
    #[error("channel send failed: {channel_name}")]
    ChannelSend { channel_name: String },

    /// Channel receive operation failed (sender dropped).
    #[error("channel receive failed: {channel_name}")]
    ChannelRecv { channel_name: String },

    /// Task join failed.
    #[error("task '{task_name}' failed to join: {reason}")]
    TaskJoin { task_name: String, reason: String },

    /// Shutdown signal received.
    #[error("shutdown requested")]
    Shutdown,

    // ─────────────────────────────────────────────────────────────
    // State & Persistence Errors
    // ─────────────────────────────────────────────────────────────
    /// State file read failed.
    #[error("failed to read state file at '{path}': {source}")]
    StateRead {
        path: String,
        #[source]
        source: std::io::Error,
    },

    /// State file write failed.
    #[error("failed to write state file at '{path}': {source}")]
    StateWrite {
        path: String,
        #[source]
        source: std::io::Error,
    },

    /// State serialization/deserialization failed.
    #[error("state serialization failed: {0}")]
    StateSerde(String),

    // ─────────────────────────────────────────────────────────────
    // External API Errors
    // ─────────────────────────────────────────────────────────────
    /// HTTP request to external API failed.
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// External API returned an error response.
    #[error("API error from '{api}': status={status}, message={message}")]
    ApiError {
        api: String,
        status: u16,
        message: String,
    },

    // ─────────────────────────────────────────────────────────────
    // Generic Errors
    // ─────────────────────────────────────────────────────────────
    /// I/O operation failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON parsing failed.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Internal error (should not happen in normal operation).
    #[error("internal error: {0}")]
    Internal(String),
}

/// Result type alias using MycryptoError.
pub type Result<T> = std::result::Result<T, MycryptoError>;

impl MycryptoError {
    /// Returns true if this error is recoverable and the operation can be retried.
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            MycryptoError::WebSocketConnection { .. }
                | MycryptoError::FeedDisconnected { .. }
                | MycryptoError::LlmRateLimit { .. }
                | MycryptoError::Http(_)
        )
    }

    /// Returns true if this error indicates the application should shut down.
    pub fn is_fatal(&self) -> bool {
        matches!(
            self,
            MycryptoError::ConfigRead { .. }
                | MycryptoError::ConfigParse(_)
                | MycryptoError::ConfigValidation(_)
                | MycryptoError::TerminalInit(_)
                | MycryptoError::Shutdown
        )
    }

    /// Creates a channel send error with the given channel name.
    pub fn channel_send(name: impl Into<String>) -> Self {
        MycryptoError::ChannelSend {
            channel_name: name.into(),
        }
    }

    /// Creates a channel receive error with the given channel name.
    pub fn channel_recv(name: impl Into<String>) -> Self {
        MycryptoError::ChannelRecv {
            channel_name: name.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = MycryptoError::ConfigValidation("risk_per_trade must be > 0".to_string());
        assert!(err.to_string().contains("risk_per_trade"));
    }

    #[test]
    fn test_recoverable_errors() {
        let recoverable = MycryptoError::FeedDisconnected {
            reason: "connection reset".to_string(),
        };
        assert!(recoverable.is_recoverable());

        let fatal = MycryptoError::ConfigValidation("invalid".to_string());
        assert!(!fatal.is_recoverable());
    }

    #[test]
    fn test_fatal_errors() {
        let fatal = MycryptoError::TerminalInit("raw mode failed".to_string());
        assert!(fatal.is_fatal());

        let non_fatal = MycryptoError::LlmRequest("timeout".to_string());
        assert!(!non_fatal.is_fatal());
    }
}
