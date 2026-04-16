//! Configuration schema for mycrypto.
//!
//! This module defines all configuration structs that are deserialized
//! from `config.toml`. Each struct has sensible defaults and validation.
//!
//! # Environment Variable Resolution
//!
//! String fields that start with `ENV:` are resolved to environment
//! variables at load time. For example, `api_key = "ENV:CLAUDE_API_KEY"`
//! will look up the `CLAUDE_API_KEY` environment variable.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::error::{MycryptoError, Result};

/// Root configuration structure containing all sections.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    /// Agent behavior configuration.
    #[serde(default)]
    pub agent: AgentConfig,

    /// Portfolio settings.
    #[serde(default)]
    pub portfolio: PortfolioConfig,

    /// Risk management rules.
    #[serde(default)]
    pub risk: RiskConfig,

    /// Signal engine configuration.
    #[serde(default)]
    pub engine: EngineConfig,

    /// Trading pairs configuration.
    #[serde(default)]
    pub pairs: PairsConfig,

    /// Market data source configuration.
    #[serde(default)]
    pub data: DataConfig,

    /// LLM provider configuration.
    #[serde(default)]
    pub llm: LlmConfig,

    /// TUI appearance and behavior.
    #[serde(default)]
    pub tui: TuiConfig,
}

impl Config {
    /// Validates the entire configuration.
    ///
    /// Returns an error if any values are invalid or incompatible.
    /// This should be called after loading and env resolution.
    pub fn validate(&self) -> Result<()> {
        // Agent validation
        if self.agent.min_confidence > 100 {
            return Err(MycryptoError::ConfigValidation(
                "agent.min_confidence must be 0-100".to_string(),
            ));
        }
        if self.agent.max_open_trades == 0 {
            return Err(MycryptoError::ConfigValidation(
                "agent.max_open_trades must be > 0".to_string(),
            ));
        }
        if self.agent.scan_interval_sec < 60 {
            return Err(MycryptoError::ConfigValidation(
                "agent.scan_interval_sec must be >= 60 seconds".to_string(),
            ));
        }

        // Portfolio validation
        if self.portfolio.virtual_balance <= Decimal::ZERO {
            return Err(MycryptoError::ConfigValidation(
                "portfolio.virtual_balance must be > 0".to_string(),
            ));
        }

        // Risk validation
        if self.risk.risk_per_trade_pct <= Decimal::ZERO
            || self.risk.risk_per_trade_pct > Decimal::from(100)
        {
            return Err(MycryptoError::ConfigValidation(
                "risk.risk_per_trade_pct must be > 0 and <= 100".to_string(),
            ));
        }
        if self.risk.max_position_pct <= Decimal::ZERO
            || self.risk.max_position_pct > Decimal::from(100)
        {
            return Err(MycryptoError::ConfigValidation(
                "risk.max_position_pct must be > 0 and <= 100".to_string(),
            ));
        }
        if self.risk.min_risk_reward < Decimal::ONE {
            return Err(MycryptoError::ConfigValidation(
                "risk.min_risk_reward must be >= 1.0".to_string(),
            ));
        }
        if self.risk.max_drawdown_pct <= Decimal::ZERO
            || self.risk.max_drawdown_pct > Decimal::from(100)
        {
            return Err(MycryptoError::ConfigValidation(
                "risk.max_drawdown_pct must be > 0 and <= 100".to_string(),
            ));
        }

        // Engine validation
        if self.engine.tick_interval_secs < 5 {
            return Err(MycryptoError::ConfigValidation(
                "engine.tick_interval_secs must be >= 5 seconds".to_string(),
            ));
        }
        if !(0.0..=1.0).contains(&self.engine.min_confidence) {
            return Err(MycryptoError::ConfigValidation(
                "engine.min_confidence must be between 0.0 and 1.0".to_string(),
            ));
        }
        if self.engine.total_exposure_limit_pct <= Decimal::ZERO
            || self.engine.total_exposure_limit_pct > Decimal::from(100)
        {
            return Err(MycryptoError::ConfigValidation(
                "engine.total_exposure_limit_pct must be > 0 and <= 100".to_string(),
            ));
        }
        if !self.engine.correlation_threshold.is_finite()
            || !(0.0..=1.0).contains(&self.engine.correlation_threshold)
        {
            return Err(MycryptoError::ConfigValidation(
                "engine.correlation_threshold must be between 0.0 and 1.0".to_string(),
            ));
        }
        for (pair, row) in &self.engine.pair_correlation {
            for (other, corr) in row {
                if !corr.is_finite() || !(0.0..=1.0).contains(corr) {
                    return Err(MycryptoError::ConfigValidation(format!(
                        "engine.pair_correlation invalid value for {}->{}, expected 0.0..=1.0",
                        pair, other
                    )));
                }
            }
        }
        if self.engine.weights.ema_crossover < 0.0
            || self.engine.weights.rsi < 0.0
            || self.engine.weights.macd < 0.0
            || self.engine.weights.bb < 0.0
            || self.engine.weights.atr_regime < 0.0
            || self.engine.weights.vwap < 0.0
            || self.engine.weights.volume_anomaly < 0.0
            || self.engine.weights.sentiment < 0.0
        {
            return Err(MycryptoError::ConfigValidation(
                "engine.weights values must be non-negative".to_string(),
            ));
        }

        // Pairs validation
        if self.pairs.watchlist.is_empty() {
            return Err(MycryptoError::ConfigValidation(
                "pairs.watchlist must contain at least one pair".to_string(),
            ));
        }

        // LLM validation
        if self.llm.max_tokens == 0 {
            return Err(MycryptoError::ConfigValidation(
                "llm.max_tokens must be > 0".to_string(),
            ));
        }

        // TUI validation
        if self.tui.refresh_rate_ms < 100 {
            return Err(MycryptoError::ConfigValidation(
                "tui.refresh_rate_ms must be >= 100".to_string(),
            ));
        }
        if self.tui.split_ratio == 0 || self.tui.split_ratio > 100 {
            return Err(MycryptoError::ConfigValidation(
                "tui.split_ratio must be 1-100".to_string(),
            ));
        }

        Ok(())
    }
}

/// Agent status - running or paused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    /// Agent is actively scanning and trading.
    #[default]
    Running,
    /// Agent is paused, no new signals will be generated.
    Paused,
}

impl std::fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentStatus::Running => write!(f, "running"),
            AgentStatus::Paused => write!(f, "paused"),
        }
    }
}

/// Agent behavior configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Current status of the agent.
    #[serde(default)]
    pub status: AgentStatus,

    /// Minimum confidence score (0-100) required to execute a signal.
    #[serde(default = "default_min_confidence")]
    pub min_confidence: u8,

    /// Maximum number of concurrent open positions.
    #[serde(default = "default_max_open_trades")]
    pub max_open_trades: u8,

    /// How often the signal engine runs, in seconds.
    #[serde(default = "default_scan_interval")]
    pub scan_interval_sec: u64,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            status: AgentStatus::Running,
            min_confidence: default_min_confidence(),
            max_open_trades: default_max_open_trades(),
            scan_interval_sec: default_scan_interval(),
        }
    }
}

fn default_min_confidence() -> u8 {
    70
}
fn default_max_open_trades() -> u8 {
    3
}
fn default_scan_interval() -> u64 {
    300
}

/// Portfolio settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioConfig {
    /// Starting virtual balance in quote currency.
    #[serde(default = "default_virtual_balance")]
    pub virtual_balance: Decimal,

    /// Quote currency symbol.
    #[serde(default = "default_currency")]
    pub currency: String,
}

impl Default for PortfolioConfig {
    fn default() -> Self {
        Self {
            virtual_balance: default_virtual_balance(),
            currency: default_currency(),
        }
    }
}

fn default_virtual_balance() -> Decimal {
    Decimal::from(10_000)
}
fn default_currency() -> String {
    "USDT".to_string()
}

/// Risk management configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskConfig {
    /// Percentage of portfolio to risk per trade.
    #[serde(default = "default_risk_per_trade")]
    pub risk_per_trade_pct: Decimal,

    /// Maximum percentage of portfolio in a single position.
    #[serde(default = "default_max_position_pct")]
    pub max_position_pct: Decimal,

    /// Maximum daily drawdown percentage before auto-pause.
    #[serde(default = "default_max_daily_drawdown")]
    pub max_daily_drawdown_pct: Decimal,

    /// Maximum drawdown percentage before blocking new signals.
    #[serde(default = "default_max_daily_drawdown")]
    pub max_drawdown_pct: Decimal,

    /// Minimum risk-reward ratio required to execute.
    #[serde(default = "default_min_risk_reward")]
    pub min_risk_reward: Decimal,

    /// Whether trailing stop is enabled.
    #[serde(default)]
    pub trailing_stop_enabled: bool,

    /// Trailing stop offset as percentage of price.
    #[serde(default = "default_trailing_stop_pct")]
    pub trailing_stop_pct: Decimal,

    /// Funding rate threshold (per 8h) above which to skip trades.
    #[serde(default = "default_funding_threshold")]
    pub funding_rate_threshold: Decimal,
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            risk_per_trade_pct: default_risk_per_trade(),
            max_position_pct: default_max_position_pct(),
            max_daily_drawdown_pct: default_max_daily_drawdown(),
            max_drawdown_pct: default_max_daily_drawdown(),
            min_risk_reward: default_min_risk_reward(),
            trailing_stop_enabled: false,
            trailing_stop_pct: default_trailing_stop_pct(),
            funding_rate_threshold: default_funding_threshold(),
        }
    }
}

fn default_risk_per_trade() -> Decimal {
    Decimal::new(15, 1) // 1.5
}
fn default_max_position_pct() -> Decimal {
    Decimal::from(20)
}
fn default_max_daily_drawdown() -> Decimal {
    Decimal::from(5)
}
fn default_min_risk_reward() -> Decimal {
    Decimal::from(2)
}
fn default_trailing_stop_pct() -> Decimal {
    Decimal::ONE
}
fn default_funding_threshold() -> Decimal {
    Decimal::new(5, 2) // 0.05
}

/// Signal engine weighting configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineWeights {
    #[serde(default = "default_weight_ema")]
    pub ema_crossover: f32,
    #[serde(default = "default_weight_rsi")]
    pub rsi: f32,
    #[serde(default = "default_weight_macd")]
    pub macd: f32,
    #[serde(default = "default_weight_bb")]
    pub bb: f32,
    #[serde(default = "default_weight_atr")]
    pub atr_regime: f32,
    #[serde(default = "default_weight_vwap")]
    pub vwap: f32,
    #[serde(default = "default_weight_volume")]
    pub volume_anomaly: f32,
    #[serde(default = "default_weight_sentiment")]
    pub sentiment: f32,
}

impl Default for EngineWeights {
    fn default() -> Self {
        Self {
            ema_crossover: default_weight_ema(),
            rsi: default_weight_rsi(),
            macd: default_weight_macd(),
            bb: default_weight_bb(),
            atr_regime: default_weight_atr(),
            vwap: default_weight_vwap(),
            volume_anomaly: default_weight_volume(),
            sentiment: default_weight_sentiment(),
        }
    }
}

/// Engine runtime configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    /// Whether the production signal engine scheduler is enabled.
    #[serde(default = "default_engine_enabled")]
    pub enabled: bool,

    /// Signal engine tick interval in seconds.
    #[serde(default = "default_engine_tick_interval")]
    pub tick_interval_secs: u64,

    /// Minimum confluence confidence (0.0 - 1.0).
    #[serde(default = "default_engine_min_confidence")]
    pub min_confidence: f32,

    /// Timeframe used for technical analysis.
    #[serde(default = "default_engine_timeframe")]
    pub timeframe: String,

    /// Maximum total portfolio exposure in percent.
    #[serde(default = "default_engine_total_exposure_limit")]
    pub total_exposure_limit_pct: Decimal,

    /// Correlation threshold above which new correlated positions are blocked.
    #[serde(default = "default_engine_correlation_threshold")]
    pub correlation_threshold: f32,

    /// Optional pair correlation matrix (0.0 - 1.0).
    #[serde(default)]
    pub pair_correlation: std::collections::HashMap<String, std::collections::HashMap<String, f32>>,

    /// Weighted voting configuration.
    #[serde(default)]
    pub weights: EngineWeights,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            enabled: default_engine_enabled(),
            tick_interval_secs: default_engine_tick_interval(),
            min_confidence: default_engine_min_confidence(),
            timeframe: default_engine_timeframe(),
            total_exposure_limit_pct: default_engine_total_exposure_limit(),
            correlation_threshold: default_engine_correlation_threshold(),
            pair_correlation: std::collections::HashMap::new(),
            weights: EngineWeights::default(),
        }
    }
}

fn default_engine_enabled() -> bool {
    true
}
fn default_engine_tick_interval() -> u64 {
    30
}
fn default_engine_min_confidence() -> f32 {
    0.65
}
fn default_engine_timeframe() -> String {
    "5m".to_string()
}
fn default_engine_total_exposure_limit() -> Decimal {
    Decimal::from(80)
}
fn default_engine_correlation_threshold() -> f32 {
    0.8
}
fn default_weight_ema() -> f32 {
    1.5
}
fn default_weight_rsi() -> f32 {
    1.2
}
fn default_weight_macd() -> f32 {
    1.3
}
fn default_weight_bb() -> f32 {
    1.0
}
fn default_weight_atr() -> f32 {
    0.8
}
fn default_weight_vwap() -> f32 {
    1.0
}
fn default_weight_volume() -> f32 {
    1.1
}
fn default_weight_sentiment() -> f32 {
    1.4
}

/// Trading pairs configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairsConfig {
    /// List of pairs to watch and trade.
    #[serde(default = "default_watchlist")]
    pub watchlist: Vec<String>,

    /// List of pairs to explicitly skip.
    #[serde(default)]
    pub blacklist: Vec<String>,
}

impl Default for PairsConfig {
    fn default() -> Self {
        Self {
            watchlist: default_watchlist(),
            blacklist: Vec::new(),
        }
    }
}

fn default_watchlist() -> Vec<String> {
    vec![
        "BTCUSDT".to_string(),
        "ETHUSDT".to_string(),
        "SOLUSDT".to_string(),
    ]
}

/// Market data source configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataConfig {
    /// Binance WebSocket URL for market data.
    #[serde(default = "default_binance_ws_url")]
    pub binance_ws_url: String,

    /// Number of candles to keep in memory per pair/timeframe.
    #[serde(default = "default_cache_candles")]
    pub cache_candles: usize,

    /// Whether Yahoo Finance source is enabled.
    #[serde(default = "default_true")]
    pub yahoo_enabled: bool,

    /// Whether CoinGecko source is enabled.
    #[serde(default = "default_true")]
    pub coingecko_enabled: bool,

    /// Whether Fear & Greed source is enabled.
    #[serde(default = "default_true")]
    pub fear_greed_enabled: bool,

    /// Whether Reddit source is enabled.
    #[serde(default = "default_true")]
    pub reddit_enabled: bool,

    /// Whether X/Twitter source is enabled.
    #[serde(default = "default_true")]
    pub twitter_enabled: bool,

    /// Whether Reuters RSS source is enabled.
    #[serde(default = "default_true")]
    pub reuters_rss_enabled: bool,

    /// Whether Bloomberg RSS source is enabled.
    #[serde(default = "default_true")]
    pub bloomberg_rss_enabled: bool,

    /// Whether Finnhub source is enabled.
    #[serde(default = "default_true")]
    pub finnhub_enabled: bool,

    /// Whether CryptoPanic source is enabled.
    #[serde(default = "default_true")]
    pub cryptopanic_enabled: bool,

    /// Whether NewsData source is enabled.
    #[serde(default = "default_true")]
    pub newsdata_enabled: bool,

    /// Yahoo cache TTL in minutes.
    #[serde(default = "default_yahoo_ttl_minutes")]
    pub yahoo_ttl_minutes: u64,

    /// CoinGecko cache TTL in minutes.
    #[serde(default = "default_coingecko_ttl_minutes")]
    pub coingecko_ttl_minutes: u64,

    /// Fear & Greed cache TTL in minutes.
    #[serde(default = "default_fear_greed_ttl_minutes")]
    pub fear_greed_ttl_minutes: u64,

    /// Reddit cache TTL in minutes.
    #[serde(default = "default_reddit_ttl_minutes")]
    pub reddit_ttl_minutes: u64,

    /// Twitter cache TTL in minutes.
    #[serde(default = "default_twitter_ttl_minutes")]
    pub twitter_ttl_minutes: u64,

    /// RSS cache TTL in minutes.
    #[serde(default = "default_rss_ttl_minutes")]
    pub rss_ttl_minutes: u64,

    /// Finnhub cache TTL in minutes.
    #[serde(default = "default_finnhub_ttl_minutes")]
    pub finnhub_ttl_minutes: u64,

    /// CryptoPanic cache TTL in minutes.
    #[serde(default = "default_cryptopanic_ttl_minutes")]
    pub cryptopanic_ttl_minutes: u64,

    /// NewsData cache TTL in minutes.
    #[serde(default = "default_newsdata_ttl_minutes")]
    pub newsdata_ttl_minutes: u64,

    /// Global source poll interval in seconds.
    #[serde(default = "default_sources_poll_interval_sec")]
    pub sources_poll_interval_sec: u64,
}

impl Default for DataConfig {
    fn default() -> Self {
        Self {
            binance_ws_url: default_binance_ws_url(),
            cache_candles: default_cache_candles(),
            yahoo_enabled: default_true(),
            coingecko_enabled: default_true(),
            fear_greed_enabled: default_true(),
            reddit_enabled: default_true(),
            twitter_enabled: default_true(),
            reuters_rss_enabled: default_true(),
            bloomberg_rss_enabled: default_true(),
            finnhub_enabled: default_true(),
            cryptopanic_enabled: default_true(),
            newsdata_enabled: default_true(),
            yahoo_ttl_minutes: default_yahoo_ttl_minutes(),
            coingecko_ttl_minutes: default_coingecko_ttl_minutes(),
            fear_greed_ttl_minutes: default_fear_greed_ttl_minutes(),
            reddit_ttl_minutes: default_reddit_ttl_minutes(),
            twitter_ttl_minutes: default_twitter_ttl_minutes(),
            rss_ttl_minutes: default_rss_ttl_minutes(),
            finnhub_ttl_minutes: default_finnhub_ttl_minutes(),
            cryptopanic_ttl_minutes: default_cryptopanic_ttl_minutes(),
            newsdata_ttl_minutes: default_newsdata_ttl_minutes(),
            sources_poll_interval_sec: default_sources_poll_interval_sec(),
        }
    }
}

fn default_binance_ws_url() -> String {
    "wss://stream.binance.com:9443/stream".to_string()
}
fn default_cache_candles() -> usize {
    200
}
fn default_true() -> bool {
    true
}
fn default_yahoo_ttl_minutes() -> u64 {
    15
}
fn default_coingecko_ttl_minutes() -> u64 {
    5
}
fn default_fear_greed_ttl_minutes() -> u64 {
    10
}
fn default_reddit_ttl_minutes() -> u64 {
    10
}
fn default_twitter_ttl_minutes() -> u64 {
    5
}
fn default_rss_ttl_minutes() -> u64 {
    15
}
fn default_finnhub_ttl_minutes() -> u64 {
    15
}
fn default_cryptopanic_ttl_minutes() -> u64 {
    10
}
fn default_newsdata_ttl_minutes() -> u64 {
    10
}
fn default_sources_poll_interval_sec() -> u64 {
    60
}

/// LLM provider options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LlmProvider {
    /// Anthropic Claude API.
    #[default]
    Claude,
    /// OpenAI API.
    OpenAI,
    /// Google Gemini API.
    Gemini,
    /// OpenRouter API (OpenAI-compatible).
    OpenRouter,
    /// Gradio Space API (HuggingFace hosted).
    Gradio,
    /// GitHub Copilot API.
    Copilot,
    /// Mock provider for testing (no API calls).
    Mock,
}

impl std::fmt::Display for LlmProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LlmProvider::Claude => write!(f, "claude"),
            LlmProvider::OpenAI => write!(f, "openai"),
            LlmProvider::Gemini => write!(f, "gemini"),
            LlmProvider::OpenRouter => write!(f, "openrouter"),
            LlmProvider::Gradio => write!(f, "gradio"),
            LlmProvider::Copilot => write!(f, "copilot"),
            LlmProvider::Mock => write!(f, "mock"),
        }
    }
}

/// LLM configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Which LLM provider to use.
    #[serde(default)]
    pub provider: LlmProvider,

    /// Model identifier.
    #[serde(default = "default_model")]
    pub model: String,

    /// API key (or ENV:VAR_NAME to resolve from environment).
    #[serde(default = "default_api_key")]
    pub api_key: String,

    /// Whether to stream responses token by token.
    #[serde(default = "default_stream")]
    pub stream: bool,

    /// Maximum tokens in response.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,

    /// Number of chat messages to keep in context.
    #[serde(default = "default_context_messages")]
    pub context_messages: usize,

    /// Base URL for API (useful for Ollama or custom endpoints).
    #[serde(default)]
    pub base_url: Option<String>,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: LlmProvider::Claude,
            model: default_model(),
            api_key: default_api_key(),
            stream: default_stream(),
            max_tokens: default_max_tokens(),
            context_messages: default_context_messages(),
            base_url: None,
        }
    }
}

fn default_model() -> String {
    "claude-opus-4-5".to_string()
}
fn default_api_key() -> String {
    "ENV:CLAUDE_API_KEY".to_string()
}
fn default_stream() -> bool {
    true
}
fn default_max_tokens() -> u32 {
    1024
}
fn default_context_messages() -> usize {
    20
}

/// TUI configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiConfig {
    /// How often to refresh the display, in milliseconds.
    #[serde(default = "default_refresh_rate")]
    pub refresh_rate_ms: u64,

    /// Color theme name.
    #[serde(default = "default_theme")]
    pub theme: String,

    /// Left panel width as percentage (1-100).
    #[serde(default = "default_split_ratio")]
    pub split_ratio: u8,

    /// Number of log lines to keep in buffer.
    #[serde(default = "default_log_lines")]
    pub log_lines: usize,

    /// Default chart timeframe shown in chart mode.
    #[serde(default = "default_chart_default_timeframe")]
    pub chart_default_timeframe: String,
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            refresh_rate_ms: default_refresh_rate(),
            theme: default_theme(),
            split_ratio: default_split_ratio(),
            log_lines: default_log_lines(),
            chart_default_timeframe: default_chart_default_timeframe(),
        }
    }
}

fn default_refresh_rate() -> u64 {
    500
}
fn default_theme() -> String {
    "dark".to_string()
}
fn default_split_ratio() -> u8 {
    50
}
fn default_log_lines() -> usize {
    200
}
fn default_chart_default_timeframe() -> String {
    "4h".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_is_valid() {
        let config = Config::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_invalid_min_confidence() {
        let mut config = Config::default();
        config.agent.min_confidence = 150;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_invalid_risk_per_trade() {
        let mut config = Config::default();
        config.risk.risk_per_trade_pct = Decimal::ZERO;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_empty_watchlist_invalid() {
        let mut config = Config::default();
        config.pairs.watchlist.clear();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_agent_status_display() {
        assert_eq!(AgentStatus::Running.to_string(), "running");
        assert_eq!(AgentStatus::Paused.to_string(), "paused");
    }

    #[test]
    fn test_default_engine_config_is_valid() {
        let config = Config::default();
        assert!(config.engine.tick_interval_secs >= 5);
        assert!((0.0..=1.0).contains(&config.engine.min_confidence));
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_llm_provider_display() {
        assert_eq!(LlmProvider::Claude.to_string(), "claude");
        assert_eq!(LlmProvider::OpenAI.to_string(), "openai");
        assert_eq!(LlmProvider::Gemini.to_string(), "gemini");
        assert_eq!(LlmProvider::OpenRouter.to_string(), "openrouter");
        assert_eq!(LlmProvider::Gradio.to_string(), "gradio");
        assert_eq!(LlmProvider::Copilot.to_string(), "copilot");
        assert_eq!(LlmProvider::Mock.to_string(), "mock");
    }

    #[test]
    fn test_invalid_correlation_threshold_rejected() {
        let mut config = Config::default();
        config.engine.correlation_threshold = 1.2;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_invalid_pair_correlation_value_rejected() {
        let mut config = Config::default();
        config.engine.pair_correlation.insert(
            "BTCUSDT".to_string(),
            std::collections::HashMap::from([("ETHUSDT".to_string(), -0.1)]),
        );
        assert!(config.validate().is_err());
    }
}
