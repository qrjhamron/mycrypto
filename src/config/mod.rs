//! Configuration management for mycrypto.
//!
//! This module handles all configuration loading, parsing, and validation.
//! Configuration is loaded from TOML files with environment variable resolution.
//!
//! # Usage
//! ```no_run
//! use mycrypto::config::{load_config, Config};
//!
//! let config = load_config(Some("config.toml")).expect("failed to load config");
//! println!("Watching pairs: {:?}", config.pairs.watchlist);
//! ```

mod loader;
mod schema;

pub use loader::{create_default_config, load_config};
pub use schema::{
    AgentConfig, AgentStatus, Config, DataConfig, EngineConfig, EngineWeights, LlmConfig,
    LlmProvider, PairsConfig, PortfolioConfig, RiskConfig, TuiConfig,
};
