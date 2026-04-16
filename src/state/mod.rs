//! Application state types for mycrypto.
//!
//! This module contains all domain types and the central `AppState` structure.
//!
//! # Module Organization
//!
//! - `app_state` - Master state and `StateUpdate` enum
//! - `portfolio` - Portfolio, Position, ClosedTrade
//! - `market` - Ticker, OHLCV, OrderBook, MarketData
//! - `signal` - Signal, SignalDirection, reasoning types

mod app_state;
mod market;
mod portfolio;
mod signal;

pub use crate::engine::EngineStatus;
pub use app_state::{
    chart_cache_key, ActiveOverlay, AppState, ChartCache, ChatMessage, ConnectionStatus,
    EconomicEvent, FocusedPanel, LogEntry, LogLevel, MacroContext, NewsCache, NewsHeadline,
    SentimentScore, SourceStatus, SourceStatusLevel, StateUpdate, TeamActionCard, TeamActionKind,
    TeamAgentScore, TeamAgentState, TeamAgentStatus, TeamDiscussionState, TeamEdgeKind,
    TeamHistoryEntry, TeamRelationEdge, TeamRole, TeamSessionSummary, TeamStance, TeamThreadEntry,
};
pub use market::{
    CandleBuffer, MarketData, MarketUpdate, OrderBook, OrderBookLevel, Ticker, Timeframe, OHLCV,
};
pub use portfolio::{
    CloseReason, ClosedTrade, Portfolio, PortfolioMetrics, Position, PositionSide,
};
pub use signal::{
    AnalysisType, ConfidenceBreakdown, ReasonEntry, Signal, SignalAction, SignalBuilder,
    SignalDirection, SignalHistory,
};
