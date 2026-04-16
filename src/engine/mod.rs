//! Signal generation engine module.

use chrono::{DateTime, Utc};

pub mod confluence;
pub mod executor;
pub mod risk;
pub mod scheduler;
pub mod sentiment;
pub mod signal_engine;
pub mod technical;

/// Runtime status for the production signal engine.
#[derive(Debug, Clone)]
pub struct EngineStatus {
    /// Which indicators are currently active in the pipeline.
    pub active_indicators: Vec<String>,
    /// Last successful scheduler tick timestamp.
    pub last_tick_time: Option<DateTime<Utc>>,
    /// Number of consecutive scheduler failures.
    pub consecutive_errors: u8,
    /// Circuit breaker state.
    pub circuit_breaker_open: bool,
    /// Most recent scheduler error.
    pub last_error: Option<String>,
    /// Number of websocket reconnect attempts.
    pub ws_reconnect_count: u64,
    /// Last websocket message timestamp.
    pub ws_last_message_at: Option<DateTime<Utc>>,
    /// Websocket uptime ratio over runtime window.
    pub ws_uptime_ratio: f32,
}

impl Default for EngineStatus {
    fn default() -> Self {
        Self {
            active_indicators: vec![
                "EMA Crossover".to_string(),
                "RSI Divergence".to_string(),
                "MACD Momentum".to_string(),
                "Bollinger Breakout".to_string(),
                "ATR Regime".to_string(),
                "VWAP Deviation".to_string(),
                "Volume Anomaly".to_string(),
                "Sentiment Momentum".to_string(),
            ],
            last_tick_time: None,
            consecutive_errors: 0,
            circuit_breaker_open: false,
            last_error: None,
            ws_reconnect_count: 0,
            ws_last_message_at: None,
            ws_uptime_ratio: 1.0,
        }
    }
}
