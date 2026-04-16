//! Application state management.
//!
//! This module contains:
//! - `AppState` - the master state structure owned by the main thread
//! - `StateUpdate` - enum for all state mutations (sent via channels)
//! - `LogEntry` - structured log entries for the TUI
//!
//! # Architecture
//!
//! The main thread owns `AppState` and is the ONLY writer. Background tasks
//! send `StateUpdate` messages through mpsc channels. The main loop receives
//! these updates and applies them atomically to `AppState`.
//!
//! This design ensures:
//! - No shared mutable state (no Mutex contention)
//! - Predictable state transitions
//! - Easy debugging (all mutations are explicit messages)

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use uuid::Uuid;

use crate::auth::{default_auth_state, AuthProvider, AuthStatus};
use crate::config::{AgentStatus, Config};
use crate::engine::EngineStatus;
use crate::state::market::{CandleBuffer, MarketData, Ticker, Timeframe, OHLCV};
use crate::state::portfolio::{CloseReason, Portfolio, Position};
use crate::state::signal::{AnalysisType, ReasonEntry, Signal, SignalAction, SignalHistory};

const CHAT_HISTORY_CAP: usize = 200;
const NEWS_HISTORY_CAP: usize = 500;
const CHART_SERIES_CAP: usize = 200;
const CHART_CACHE_KEY_CAP: usize = 50;

fn push_bounded<T>(buf: &mut VecDeque<T>, value: T, cap: usize) {
    buf.push_back(value);
    while buf.len() > cap {
        let _ = buf.pop_front();
    }
}

/// Log level for TUI display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    /// Debug information.
    Debug,
    /// General information.
    Info,
    /// Warning conditions.
    Warn,
    /// Error conditions.
    Error,
    /// Trade-related events.
    Trade,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Debug => write!(f, "DEBUG"),
            LogLevel::Info => write!(f, "INFO"),
            LogLevel::Warn => write!(f, "WARN"),
            LogLevel::Error => write!(f, "ERROR"),
            LogLevel::Trade => write!(f, "TRADE"),
        }
    }
}

/// A log entry for display in the TUI.
#[derive(Debug, Clone)]
pub struct LogEntry {
    /// Timestamp of the log entry.
    pub timestamp: DateTime<Utc>,
    /// Log level.
    pub level: LogLevel,
    /// Log message.
    pub message: String,
}

impl LogEntry {
    /// Creates a new log entry.
    pub fn new(level: LogLevel, message: impl Into<String>) -> Self {
        Self {
            timestamp: Utc::now(),
            level,
            message: message.into(),
        }
    }

    /// Creates an info log entry.
    pub fn info(message: impl Into<String>) -> Self {
        Self::new(LogLevel::Info, message)
    }

    /// Creates a warning log entry.
    pub fn warn(message: impl Into<String>) -> Self {
        Self::new(LogLevel::Warn, message)
    }

    /// Creates an error log entry.
    pub fn error(message: impl Into<String>) -> Self {
        Self::new(LogLevel::Error, message)
    }

    /// Creates a trade log entry.
    pub fn trade(message: impl Into<String>) -> Self {
        Self::new(LogLevel::Trade, message)
    }
}

/// Chat message for conversation history.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    /// Unique message ID.
    pub id: Uuid,
    /// Whether this is from the user (true) or agent (false).
    pub is_user: bool,
    /// Message content.
    pub content: String,
    /// When the message was sent.
    pub timestamp: DateTime<Utc>,
    /// Whether the message is still streaming (agent only).
    pub is_streaming: bool,
}

impl ChatMessage {
    /// Creates a new user message.
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            is_user: true,
            content: content.into(),
            timestamp: Utc::now(),
            is_streaming: false,
        }
    }

    /// Creates a new agent message.
    pub fn agent(content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            is_user: false,
            content: content.into(),
            timestamp: Utc::now(),
            is_streaming: false,
        }
    }

    /// Creates a streaming agent message placeholder.
    pub fn agent_streaming() -> Self {
        Self {
            id: Uuid::new_v4(),
            is_user: false,
            content: String::new(),
            timestamp: Utc::now(),
            is_streaming: true,
        }
    }
}

/// Team discussion role identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TeamRole {
    /// Technical analysis specialist.
    Analyst,
    /// Execution and timing specialist.
    Trader,
    /// Downside/risk specialist.
    RiskManager,
    /// On-chain and macro/fundamental specialist.
    Researcher,
    /// Final decision synthesizer.
    Leader,
    /// Contrarian perspective specialist.
    DevilsAdvocate,
}

impl TeamRole {
    /// Ordered role list for UI rendering.
    pub const ALL: [Self; 6] = [
        Self::Analyst,
        Self::Trader,
        Self::RiskManager,
        Self::Researcher,
        Self::Leader,
        Self::DevilsAdvocate,
    ];

    /// Display label for the role.
    pub fn label(self) -> &'static str {
        match self {
            Self::Analyst => "Analyst",
            Self::Trader => "Trader",
            Self::RiskManager => "Risk Manager",
            Self::Researcher => "Researcher",
            Self::Leader => "Leader",
            Self::DevilsAdvocate => "Devil's Advocate",
        }
    }

    /// Emoji marker for the role.
    pub fn emoji(self) -> &'static str {
        match self {
            Self::Analyst => "📊",
            Self::Trader => "📈",
            Self::RiskManager => "🛡",
            Self::Researcher => "🔬",
            Self::Leader => "👑",
            Self::DevilsAdvocate => "😈",
        }
    }

    /// Short graph label for the role.
    pub fn short(self) -> &'static str {
        match self {
            Self::Analyst => "A",
            Self::Trader => "T",
            Self::RiskManager => "R",
            Self::Researcher => "S",
            Self::Leader => "L",
            Self::DevilsAdvocate => "D",
        }
    }

    /// Stable role key used in machine-readable markers.
    pub fn key(self) -> &'static str {
        match self {
            Self::Analyst => "ANALYST",
            Self::Trader => "TRADER",
            Self::RiskManager => "RISK_MANAGER",
            Self::Researcher => "RESEARCHER",
            Self::Leader => "LEADER",
            Self::DevilsAdvocate => "DEVILS_ADVOCATE",
        }
    }

    /// Parse role from machine-readable key.
    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim().to_ascii_uppercase().as_str() {
            "ANALYST" => Some(Self::Analyst),
            "TRADER" => Some(Self::Trader),
            "RISK_MANAGER" => Some(Self::RiskManager),
            "RESEARCHER" => Some(Self::Researcher),
            "LEADER" => Some(Self::Leader),
            "DEVILS_ADVOCATE" => Some(Self::DevilsAdvocate),
            _ => None,
        }
    }
}

impl std::fmt::Display for TeamRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// Team agent status shown in Team Discussion page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeamAgentStatus {
    /// Agent waiting for a new task.
    Idle,
    /// Agent is currently waiting for LLM response.
    Thinking,
    /// Agent response for current phase is complete.
    Done,
}

/// One agent status row for team view.
#[derive(Debug, Clone)]
pub struct TeamAgentState {
    /// Agent role.
    pub role: TeamRole,
    /// Current status.
    pub status: TeamAgentStatus,
    /// Last status update timestamp.
    pub updated_at: DateTime<Utc>,
}

/// One message produced during team discussion.
#[derive(Debug, Clone)]
pub struct TeamThreadEntry {
    /// Role that produced the message.
    pub role: TeamRole,
    /// Discussion phase (1 = debate, 2 = leader synthesis).
    pub phase: u8,
    /// Message content.
    pub content: String,
    /// Timestamp when message was received.
    pub timestamp: DateTime<Utc>,
}

/// Relationship edge type in team graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TeamEdgeKind {
    /// Agreement edge.
    Agree,
    /// Counter/contrarian edge.
    Counter,
}

/// Weighted edge between two team agents.
#[derive(Debug, Clone)]
pub struct TeamRelationEdge {
    /// Source role.
    pub from: TeamRole,
    /// Target role.
    pub to: TeamRole,
    /// Relationship type.
    pub kind: TeamEdgeKind,
    /// Weight/frequency inside current session.
    pub weight: u32,
}

/// Action card category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeamActionKind {
    /// Open long position.
    Buy,
    /// Open short position.
    Sell,
    /// Close an existing position.
    Close,
    /// No-op / stay flat.
    Hold,
}

/// Final recommendation that requires user confirmation.
#[derive(Debug, Clone)]
pub struct TeamActionCard {
    /// Recommended action category.
    pub kind: TeamActionKind,
    /// Target trading pair (if applicable).
    pub pair: Option<String>,
    /// Portfolio allocation percentage.
    pub allocation_pct: Decimal,
    /// Short headline summary (e.g. BUY BTCUSDT 10%).
    pub summary: String,
    /// Supporting rationale text from Leader.
    pub rationale: String,
}

/// Agent stance classification for summary scorecard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeamStance {
    /// Positive directional bias.
    Bullish,
    /// Negative directional bias.
    Bearish,
    /// No directional bias.
    Neutral,
}

/// Per-agent scorecard entry.
#[derive(Debug, Clone)]
pub struct TeamAgentScore {
    /// Agent role.
    pub role: TeamRole,
    /// Agent stance.
    pub stance: TeamStance,
    /// Confidence (0-100).
    pub confidence: u8,
    /// Word count of contribution.
    pub word_count: usize,
}

/// Session summary displayed after discussion completes.
#[derive(Debug, Clone)]
pub struct TeamSessionSummary {
    /// Original session topic/prompt.
    pub topic: String,
    /// Session completion time.
    pub timestamp: DateTime<Utc>,
    /// Final leader verdict headline.
    pub leader_verdict: String,
    /// Per-agent scorecard.
    pub scorecard: Vec<TeamAgentScore>,
}

/// History entry for previous sessions.
#[derive(Debug, Clone)]
pub struct TeamHistoryEntry {
    /// Original topic.
    pub topic: String,
    /// Completion time.
    pub timestamp: DateTime<Utc>,
    /// Final verdict.
    pub leader_verdict: String,
    /// User decision (Executed, Dismissed, Re-analyzed, Pending).
    pub user_decision: String,
}

/// Team discussion state rendered in Team page.
#[derive(Debug, Clone)]
pub struct TeamDiscussionState {
    /// Current prompt for active/last session.
    pub prompt: Option<String>,
    /// Agent statuses.
    pub agents: Vec<TeamAgentState>,
    /// Conversation thread.
    pub thread: Vec<TeamThreadEntry>,
    /// Relationship graph edges.
    pub edges: Vec<TeamRelationEdge>,
    /// Leader recommendation awaiting confirmation.
    pub pending_action: Option<TeamActionCard>,
    /// Whether a team session is running.
    pub active: bool,
    /// Last transient error.
    pub last_error: Option<String>,
    /// Latest completed-session summary (for scorecard panel).
    pub session_summary: Option<TeamSessionSummary>,
    /// Last 5 discussion history entries.
    pub history: Vec<TeamHistoryEntry>,
    /// Active session id used to ignore stale team updates.
    pub active_session_id: Option<u64>,
}

impl Default for TeamDiscussionState {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            prompt: None,
            agents: TeamRole::ALL
                .iter()
                .map(|role| TeamAgentState {
                    role: *role,
                    status: TeamAgentStatus::Idle,
                    updated_at: now,
                })
                .collect(),
            thread: Vec::new(),
            edges: Vec::new(),
            pending_action: None,
            active: false,
            last_error: None,
            session_summary: None,
            history: Vec::new(),
            active_session_id: None,
        }
    }
}

/// Connection status for external services.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStatus {
    /// Not connected.
    Disconnected,
    /// Connecting in progress.
    Connecting,
    /// Successfully connected.
    Connected,
    /// Connection error.
    Error,
}

/// Which panel is currently focused in the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FocusedPanel {
    /// Trading/portfolio panel (left).
    #[default]
    Trading,
    /// Chat panel (right).
    Chat,
}

/// Which overlay is currently shown (if any).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveOverlay {
    /// No overlay, normal view.
    None,
    /// Help overlay.
    Help,
    /// Trade history overlay.
    History,
    /// Settings overlay.
    Settings,
    /// Confirmation dialog.
    Confirm,
}

/// Macro context data (Fear & Greed, BTC dominance).
#[derive(Debug, Clone)]
pub struct EconomicEvent {
    /// Event title.
    pub title: String,
    /// Event timestamp.
    pub time: DateTime<Utc>,
    /// Event impact level (high/medium/low).
    pub impact: String,
    /// Event country/region.
    pub country: String,
}

impl Default for EconomicEvent {
    fn default() -> Self {
        Self {
            title: String::new(),
            time: Utc::now(),
            impact: "unknown".to_string(),
            country: "US".to_string(),
        }
    }
}

/// News headline item for UI and chat context.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NewsHeadline {
    /// News source.
    pub source: String,
    /// Headline text.
    pub title: String,
    /// Optional canonical URL.
    pub url: Option<String>,
    /// Publish time.
    pub published_at: DateTime<Utc>,
    /// Optional simple sentiment estimate (-1..1).
    pub sentiment: Option<f32>,
}

/// Persistent news cache payload stored on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsCache {
    /// Cached headlines sorted newest-first.
    pub headlines: Vec<NewsHeadline>,
    /// Last successful network refresh timestamp.
    pub last_fetch_at: Option<DateTime<Utc>>,
    /// Cache write timestamp.
    pub cached_at: DateTime<Utc>,
}

/// Persistent chart cache payload stored on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartCache {
    /// Per key cached candles where key is `PAIR|INTERVAL`.
    #[serde(default)]
    pub series: HashMap<String, Vec<OHLCV>>,
    /// Last successful chart refresh timestamp by `PAIR|INTERVAL` key.
    #[serde(default)]
    pub last_fetch_at: HashMap<String, DateTime<Utc>>,
    /// LRU key order persisted oldest-to-newest.
    #[serde(default)]
    pub lru_order: Vec<String>,
    /// Cache write timestamp.
    #[serde(default = "Utc::now")]
    pub cached_at: DateTime<Utc>,
}

/// Composite sentiment output from all sources.
#[derive(Debug, Clone)]
pub struct SentimentScore {
    /// Fear & Greed index.
    pub fear_greed: Option<u8>,
    /// Fear & Greed label.
    pub fear_greed_label: Option<String>,
    /// Reddit sentiment score (-1..1).
    pub reddit_score: Option<f32>,
    /// X/Twitter sentiment score (-1..1).
    pub twitter_score: Option<f32>,
    /// RSS/Finnhub headline sentiment score (-1..1).
    pub news_score: Option<f32>,
    /// Weighted composite score from available sources.
    pub composite: f32,
    /// Source names that contributed.
    pub sources_available: Vec<String>,
    /// Last update time.
    pub updated_at: DateTime<Utc>,
}

impl Default for SentimentScore {
    fn default() -> Self {
        Self {
            fear_greed: None,
            fear_greed_label: None,
            reddit_score: None,
            twitter_score: None,
            news_score: None,
            composite: 0.0,
            sources_available: Vec::new(),
            updated_at: Utc::now(),
        }
    }
}

/// Source health level used on `/status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceStatusLevel {
    /// Connected realtime stream.
    Connected,
    /// Healthy polling source.
    Ok,
    /// Degraded source.
    Warn,
    /// Failed source.
    Error,
    /// Missing configuration.
    MissingConfig,
    /// Disabled in config.
    Disabled,
}

impl std::fmt::Display for SourceStatusLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceStatusLevel::Connected => write!(f, "Connected"),
            SourceStatusLevel::Ok => write!(f, "OK"),
            SourceStatusLevel::Warn => write!(f, "Warn"),
            SourceStatusLevel::Error => write!(f, "Error"),
            SourceStatusLevel::MissingConfig => write!(f, "No API key"),
            SourceStatusLevel::Disabled => write!(f, "Disabled"),
        }
    }
}

/// Health details for one data source.
#[derive(Debug, Clone)]
pub struct SourceStatus {
    /// Source display name.
    pub name: String,
    /// Current health status.
    pub level: SourceStatusLevel,
    /// Additional detail (cache age, reason, etc).
    pub detail: String,
    /// Last successful update.
    pub last_ok: Option<DateTime<Utc>>,
    /// Consecutive failure count tracked by source runtime.
    pub consecutive_failures: u32,
    /// Stable runtime status label (Healthy/Degraded/Dead/etc).
    pub runtime_status: String,
}

impl Default for SourceStatus {
    fn default() -> Self {
        Self {
            name: String::new(),
            level: SourceStatusLevel::Warn,
            detail: String::new(),
            last_ok: None,
            consecutive_failures: 0,
            runtime_status: "Unknown".to_string(),
        }
    }
}

/// Macro context data sourced from external providers.
#[derive(Debug, Clone, Default)]
pub struct MacroContext {
    /// SPY daily change percentage.
    pub spy_change_pct: Option<f32>,
    /// DXY daily change percentage.
    pub dxy_change_pct: Option<f32>,
    /// Current VIX value.
    pub vix: Option<f32>,
    /// BTC market dominance percentage.
    pub btc_dominance: Option<f32>,
    /// Total crypto market cap (USD).
    pub total_market_cap: Option<f64>,
    /// Upcoming economic events.
    pub upcoming_events: Vec<EconomicEvent>,
    /// Last update time.
    pub updated_at: Option<DateTime<Utc>>,
}

/// The master application state.
///
/// This struct is owned exclusively by the main thread. Background tasks
/// communicate state changes via `StateUpdate` messages through channels.
/// The TUI reads from this state to render the interface.
#[derive(Debug, Clone)]
pub struct AppState {
    // ─────────────────────────────────────────────────────────────
    // Configuration (read-mostly, rarely updated)
    // ─────────────────────────────────────────────────────────────
    /// Current configuration.
    pub config: Config,

    // ─────────────────────────────────────────────────────────────
    // Agent State
    // ─────────────────────────────────────────────────────────────
    /// Agent running status.
    pub agent_status: AgentStatus,

    /// Error message if agent is in error state.
    pub agent_error: Option<String>,

    // ─────────────────────────────────────────────────────────────
    // Portfolio & Trading
    // ─────────────────────────────────────────────────────────────
    /// Virtual portfolio for paper trading.
    pub portfolio: Portfolio,

    /// Recent signals history.
    pub signal_history: SignalHistory,

    // ─────────────────────────────────────────────────────────────
    // Market Data
    // ─────────────────────────────────────────────────────────────
    /// Market data per trading pair.
    pub market_data: HashMap<String, MarketData>,

    /// Macro market context.
    pub macro_context: MacroContext,

    /// Composite sentiment data.
    pub sentiment_score: Option<SentimentScore>,

    /// Latest external headlines.
    pub news_headlines: Vec<NewsHeadline>,

    /// Extended cached news history (up to 500) for News History page.
    pub news_history: VecDeque<NewsHeadline>,

    /// Last successful news fetch timestamp.
    pub news_last_fetch_at: Option<DateTime<Utc>>,

    /// Whether an explicit news refresh is in progress.
    pub news_loading: bool,

    /// Chart cache by `PAIR|INTERVAL` key.
    pub chart_cache: HashMap<String, Vec<OHLCV>>,

    /// LRU key order for chart cache entries.
    pub chart_cache_lru: VecDeque<String>,

    /// Last chart refresh timestamp by `PAIR|INTERVAL` key.
    pub chart_last_fetch_at: HashMap<String, DateTime<Utc>>,

    /// Data source health by source key.
    pub source_health: HashMap<String, SourceStatus>,

    /// Data quality score per pair (0.0-1.0).
    pub data_quality: HashMap<String, f32>,

    // ─────────────────────────────────────────────────────────────
    // Connection Status
    // ─────────────────────────────────────────────────────────────
    /// Market data feed connection status.
    pub feed_status: ConnectionStatus,

    /// LLM API connection status.
    pub llm_status: ConnectionStatus,

    // ─────────────────────────────────────────────────────────────
    // Chat
    // ─────────────────────────────────────────────────────────────
    /// Chat message history.
    pub chat_messages: VecDeque<ChatMessage>,

    /// Current user input buffer.
    pub chat_input: String,

    /// Whether agent is currently generating a response.
    pub is_agent_thinking: bool,

    /// Team discussion state for multi-agent debates.
    pub team_discussion: TeamDiscussionState,

    /// Production signal engine runtime status.
    pub engine_status: EngineStatus,

    // ─────────────────────────────────────────────────────────────
    // TUI State
    // ─────────────────────────────────────────────────────────────
    /// Currently focused panel.
    pub focused_panel: FocusedPanel,

    /// Active overlay (if any).
    pub active_overlay: ActiveOverlay,

    /// Log entries for the log bar.
    pub log_entries: Vec<LogEntry>,

    /// Scroll position for chat.
    pub chat_scroll: usize,

    /// Scroll position for positions list.
    pub positions_scroll: usize,

    /// Selected position index (for actions).
    pub selected_position: Option<usize>,

    /// Currently displayed chart timeframe.
    pub chart_timeframe: Timeframe,

    /// Currently displayed chart pair.
    pub chart_pair: String,

    /// Horizontal chart scroll offset.
    pub chart_offset: usize,

    /// Chart zoom level (higher = fewer candles shown).
    pub chart_zoom: usize,

    /// Whether chart indicators overlay is enabled.
    pub chart_show_indicators: bool,

    /// Whether chart sentiment overlay is enabled.
    pub chart_show_sentiment: bool,

    // ─────────────────────────────────────────────────────────────
    // Authentication
    // ─────────────────────────────────────────────────────────────
    /// Authentication state keyed by provider.
    pub auth_state: HashMap<AuthProvider, AuthStatus>,

    /// Cached OpenRouter models with free prompt/completion pricing.
    pub openrouter_free_models: Vec<String>,

    // ─────────────────────────────────────────────────────────────
    // Timestamps
    // ─────────────────────────────────────────────────────────────
    /// When the app was started.
    pub started_at: DateTime<Utc>,

    /// Last state update time.
    pub updated_at: DateTime<Utc>,
}

impl AppState {
    /// Creates a new AppState from configuration.
    pub fn new(config: Config) -> Self {
        let portfolio = Portfolio::new(
            config.portfolio.virtual_balance,
            config.portfolio.currency.clone(),
        );

        // Initialize market data for watchlist pairs
        let mut market_data = HashMap::new();
        for pair in &config.pairs.watchlist {
            market_data.insert(
                pair.clone(),
                MarketData::new(pair.clone(), config.data.cache_candles),
            );
        }

        let chart_pair = config
            .pairs
            .watchlist
            .first()
            .cloned()
            .unwrap_or_else(|| "BTCUSDT".to_string());

        let now = Utc::now();

        let chart_timeframe = Timeframe::from_binance_interval(&config.tui.chart_default_timeframe)
            .unwrap_or(Timeframe::H4);

        Self {
            agent_status: config.agent.status,
            agent_error: None,
            config,
            portfolio,
            signal_history: SignalHistory::new(50),
            market_data,
            macro_context: MacroContext::default(),
            sentiment_score: None,
            news_headlines: Vec::new(),
            news_history: VecDeque::new(),
            news_last_fetch_at: None,
            news_loading: true,
            chart_cache: HashMap::new(),
            chart_cache_lru: VecDeque::new(),
            chart_last_fetch_at: HashMap::new(),
            source_health: HashMap::new(),
            data_quality: HashMap::new(),
            feed_status: ConnectionStatus::Disconnected,
            llm_status: ConnectionStatus::Disconnected,
            chat_messages: VecDeque::new(),
            chat_input: String::new(),
            is_agent_thinking: false,
            team_discussion: TeamDiscussionState::default(),
            engine_status: EngineStatus::default(),
            focused_panel: FocusedPanel::Trading,
            active_overlay: ActiveOverlay::None,
            log_entries: Vec::new(),
            chat_scroll: 0,
            positions_scroll: 0,
            selected_position: None,
            chart_timeframe,
            chart_pair,
            chart_offset: 0,
            chart_zoom: 48,
            chart_show_indicators: true,
            chart_show_sentiment: true,
            auth_state: default_auth_state(),
            openrouter_free_models: Vec::new(),
            started_at: now,
            updated_at: now,
        }
    }

    /// Applies a state update.
    ///
    /// This is the single entry point for all state mutations.
    /// Called by the main loop when receiving updates from channels.
    pub fn apply_update(&mut self, update: StateUpdate) {
        self.updated_at = Utc::now();

        match update {
            StateUpdate::MarketTick(ticker) => {
                self.apply_market_tick(ticker);
                self.recompute_data_quality();
            }
            StateUpdate::CandleUpdate {
                pair,
                timeframe,
                candle,
            } => {
                self.apply_candle_update(&pair, timeframe, candle);
            }
            StateUpdate::NewSignal(mut signal) => {
                self.apply_context_to_signal(&mut signal);
                self.signal_history.push(signal);
            }
            StateUpdate::PositionOpened(position) => {
                if let Err(e) = self.portfolio.open_position(position) {
                    self.add_log(LogEntry::error(format!("Failed to open position: {}", e)));
                }
            }
            StateUpdate::PositionUpdated(position) => {
                if let Some(pos) = self.portfolio.get_position_mut(position.id) {
                    *pos = position;
                }
                self.portfolio.recalculate();
            }
            StateUpdate::PositionClosed {
                position_id,
                exit_price,
                reason,
            } => {
                if let Some(trade) = self
                    .portfolio
                    .close_position(position_id, exit_price, reason)
                {
                    self.add_log(LogEntry::trade(format!(
                        "{} {} closed: {} @ {} ({})",
                        trade.pair,
                        trade.side,
                        if trade.is_winner() { "WIN" } else { "LOSS" },
                        trade.exit_price,
                        trade.close_reason
                    )));
                }
            }
            StateUpdate::ChatToken(token) => {
                self.apply_chat_token(&token);
            }
            StateUpdate::ChatDone => {
                self.finish_agent_message();
            }
            StateUpdate::ChatError(error) => {
                self.is_agent_thinking = false;
                self.add_log(LogEntry::error(format!("Chat error: {}", error)));
            }
            StateUpdate::TeamSessionStarted { prompt, session_id } => {
                let now = Utc::now();
                self.team_discussion.prompt = Some(prompt);
                self.team_discussion.active_session_id = Some(session_id);
                self.team_discussion.thread.clear();
                self.team_discussion.edges.clear();
                self.team_discussion.pending_action = None;
                self.team_discussion.active = true;
                self.team_discussion.last_error = None;
                self.team_discussion.session_summary = None;
                for agent in &mut self.team_discussion.agents {
                    agent.status = TeamAgentStatus::Idle;
                    agent.updated_at = now;
                }
            }
            StateUpdate::TeamAgentStatusChanged {
                role,
                status,
                session_id,
            } => {
                if self.team_discussion.active_session_id != Some(session_id) {
                    return;
                }
                if let Some(agent) = self
                    .team_discussion
                    .agents
                    .iter_mut()
                    .find(|agent| agent.role == role)
                {
                    agent.status = status;
                    agent.updated_at = Utc::now();
                }
            }
            StateUpdate::TeamMessage {
                role,
                phase,
                content,
                session_id,
            } => {
                if self.team_discussion.active_session_id != Some(session_id) {
                    return;
                }
                self.team_discussion.thread.push(TeamThreadEntry {
                    role,
                    phase,
                    content,
                    timestamp: Utc::now(),
                });
            }
            StateUpdate::TeamRelationshipsUpdated { edges, session_id } => {
                if self.team_discussion.active_session_id != Some(session_id) {
                    return;
                }
                self.team_discussion.edges = edges;
            }
            StateUpdate::TeamActionProposed { card, session_id } => {
                if self.team_discussion.active_session_id != Some(session_id) {
                    return;
                }
                self.team_discussion.pending_action = Some(card);
            }
            StateUpdate::TeamSummary {
                summary,
                session_id,
            } => {
                if self.team_discussion.active_session_id != Some(session_id) {
                    return;
                }
                self.team_discussion.session_summary = Some(summary.clone());
                self.team_discussion.history.insert(
                    0,
                    TeamHistoryEntry {
                        topic: summary.topic.clone(),
                        timestamp: summary.timestamp,
                        leader_verdict: summary.leader_verdict.clone(),
                        user_decision: "Pending".to_string(),
                    },
                );
                if self.team_discussion.history.len() > 5 {
                    self.team_discussion.history.truncate(5);
                }
            }
            StateUpdate::TeamSessionCompleted { session_id } => {
                if self.team_discussion.active_session_id != Some(session_id) {
                    return;
                }
                self.team_discussion.active = false;
                self.team_discussion.active_session_id = None;
                for agent in &mut self.team_discussion.agents {
                    if agent.status == TeamAgentStatus::Thinking {
                        agent.status = TeamAgentStatus::Done;
                        agent.updated_at = Utc::now();
                    }
                }
            }
            StateUpdate::TeamSessionError {
                error: err,
                session_id,
            } => {
                if self.team_discussion.active_session_id != Some(session_id) {
                    return;
                }
                self.team_discussion.active = false;
                self.team_discussion.active_session_id = None;
                self.team_discussion.last_error = Some(err.clone());
                self.add_log(LogEntry::error(format!("Team discussion error: {}", err)));
                for agent in &mut self.team_discussion.agents {
                    if agent.status == TeamAgentStatus::Thinking {
                        agent.status = TeamAgentStatus::Done;
                        agent.updated_at = Utc::now();
                    }
                }
            }
            StateUpdate::TeamActionCleared => {
                self.team_discussion.pending_action = None;
            }
            StateUpdate::TeamHistoryDecisionUpdated {
                timestamp,
                decision,
            } => {
                if let Some(entry) = self
                    .team_discussion
                    .history
                    .iter_mut()
                    .find(|entry| entry.timestamp == timestamp)
                {
                    entry.user_decision = decision;
                }
            }
            StateUpdate::EngineStatusUpdated(status) => {
                self.engine_status = status;
                self.recompute_data_quality();
            }
            StateUpdate::WsFeedTelemetry {
                reconnect_count,
                last_message_at,
                uptime_ratio,
            } => {
                self.engine_status.ws_reconnect_count = reconnect_count;
                self.engine_status.ws_last_message_at = Some(last_message_at);
                self.engine_status.ws_uptime_ratio = uptime_ratio.clamp(0.0, 1.0);
                self.recompute_data_quality();
            }
            StateUpdate::AgentStatusChanged(status) => {
                self.agent_status = status;
                self.add_log(LogEntry::info(format!("Agent status: {}", status)));
            }
            StateUpdate::AgentError(error) => {
                self.agent_error = Some(error.clone());
                self.add_log(LogEntry::error(format!("Agent error: {}", error)));
            }
            StateUpdate::FeedStatusChanged(status) => {
                self.feed_status = status;
                self.recompute_data_quality();
            }
            StateUpdate::LlmStatusChanged(status) => {
                self.llm_status = status;
            }
            StateUpdate::FundingRateUpdate {
                pair,
                rate,
                next_time,
            } => {
                if let Some(market) = self.market_data.get_mut(&pair) {
                    market.funding_rate = Some(rate);
                    market.next_funding_time = Some(next_time);
                }
            }
            StateUpdate::MacroUpdate(context) => {
                self.macro_context = context;
            }
            StateUpdate::SentimentUpdate(sentiment) => {
                self.sentiment_score = Some(sentiment);
                self.recompute_data_quality();
            }
            StateUpdate::NewsUpdate(headlines) => {
                self.apply_news_update(headlines);
            }
            StateUpdate::NewsHistoryLoaded {
                headlines,
                last_fetch_at,
            } => {
                self.apply_news_history_loaded(headlines, last_fetch_at);
            }
            StateUpdate::NewsRefreshStarted => {
                self.news_loading = true;
            }
            StateUpdate::NewsRefreshCompleted { fetched_at } => {
                self.news_loading = false;
                self.news_last_fetch_at = Some(fetched_at);
            }
            StateUpdate::ChartCacheLoaded(payload) => {
                self.apply_chart_cache_loaded(payload);
            }
            StateUpdate::ChartSeriesUpdate {
                pair,
                timeframe,
                candles,
                fetched_at,
            } => {
                self.apply_chart_series_update(&pair, timeframe, candles, fetched_at);
            }
            StateUpdate::SourceHealthChanged(source) => {
                self.source_health.insert(source.name.clone(), source);
                self.recompute_data_quality();
            }
            StateUpdate::DataQualityUpdated { pair, score } => {
                self.data_quality.insert(pair, score.clamp(0.0, 1.0));
            }
            StateUpdate::ConfigChanged(config) => {
                self.config = *config;
            }
            StateUpdate::AuthStateChanged { provider, status } => {
                self.auth_state.insert(provider, status);
            }
            StateUpdate::OpenRouterFreeModelsUpdated(models) => {
                self.openrouter_free_models = models;
            }
            StateUpdate::Log(entry) => {
                self.add_log(entry);
            }
        }
    }

    fn recompute_data_quality(&mut self) {
        let ws_component = match self.feed_status {
            ConnectionStatus::Connected => 1.0,
            ConnectionStatus::Connecting => 0.6,
            ConnectionStatus::Disconnected | ConnectionStatus::Error => 0.2,
        } * self.engine_status.ws_uptime_ratio.clamp(0.0, 1.0);

        let price_sources = ["Binance WS", "CoinGecko", "Yahoo Finance"];
        let mut healthy = 0.0f32;
        let mut total = 0.0f32;
        for source in price_sources {
            if let Some(status) = self.source_health.get(source) {
                total += 1.0;
                if matches!(
                    status.level,
                    SourceStatusLevel::Connected | SourceStatusLevel::Ok
                ) {
                    healthy += 1.0;
                }
            }
        }
        let price_component = if total > 0.0 { healthy / total } else { 0.5 };

        let sentiment_component = sentiment_agreement_score(self.sentiment_score.as_ref());
        let score = (ws_component * 0.5 + price_component * 0.3 + sentiment_component * 0.2)
            .clamp(0.0, 1.0);

        for pair in &self.config.pairs.watchlist {
            self.data_quality.insert(pair.clone(), score);
        }
    }

    /// Applies a market tick update.
    fn apply_market_tick(&mut self, ticker: Ticker) {
        let pair = ticker.pair.clone();

        // Update market data
        if let Some(market) = self.market_data.get_mut(&pair) {
            market.ticker = ticker.clone();
        }

        // Update open positions
        for position in &mut self.portfolio.positions {
            if position.pair == pair {
                position.update_price(ticker.price);
            }
        }

        self.portfolio.recalculate();
    }

    /// Applies a candle update.
    fn apply_candle_update(&mut self, pair: &str, timeframe: Timeframe, candle: OHLCV) {
        if let Some(market) = self.market_data.get_mut(pair) {
            if let Some(buffer) = market.get_candles_mut(timeframe) {
                // If this candle has the same timestamp as the latest, update it
                // Otherwise push a new one
                if let Some(latest) = buffer.latest_mut() {
                    if latest.timestamp == candle.timestamp {
                        *latest = candle;
                        return;
                    }
                }
                buffer.push(candle);
            }
        }
    }

    /// Applies a streaming chat token.
    fn apply_chat_token(&mut self, token: &str) {
        if let Some(msg) = self.chat_messages.back_mut() {
            if !msg.is_user && msg.is_streaming {
                msg.content.push_str(token);
            }
        }
    }

    /// Finishes the current agent message.
    fn finish_agent_message(&mut self) {
        self.is_agent_thinking = false;
        if let Some(msg) = self.chat_messages.back_mut() {
            if !msg.is_user {
                msg.is_streaming = false;
            }
        }
    }

    /// Adds a log entry.
    fn add_log(&mut self, entry: LogEntry) {
        self.log_entries.push(entry);
        // Trim to configured max
        let max = self.config.tui.log_lines;
        if self.log_entries.len() > max {
            self.log_entries.drain(0..self.log_entries.len() - max);
        }
    }

    fn apply_news_update(&mut self, headlines: Vec<NewsHeadline>) {
        self.news_loading = false;

        let mut merged: Vec<NewsHeadline> =
            std::mem::take(&mut self.news_history).into_iter().collect();
        merged.extend(headlines);
        self.news_history = normalize_news_items(merged, NEWS_HISTORY_CAP)
            .into_iter()
            .collect();
        self.news_headlines = self.news_history.iter().take(60).cloned().collect();
    }

    fn apply_news_history_loaded(
        &mut self,
        headlines: Vec<NewsHeadline>,
        last_fetch_at: Option<DateTime<Utc>>,
    ) {
        self.news_history = normalize_news_items(headlines, NEWS_HISTORY_CAP)
            .into_iter()
            .collect();
        self.news_headlines = self.news_history.iter().take(60).cloned().collect();
        self.news_last_fetch_at = last_fetch_at;
        self.news_loading = false;
    }

    fn apply_chart_cache_loaded(&mut self, payload: ChartCache) {
        self.chart_cache.clear();
        self.chart_cache_lru.clear();
        self.chart_last_fetch_at.clear();

        let order = chart_cache_load_order(&payload.series, payload.lru_order);

        for key in order {
            let Some(candles) = payload.series.get(&key).cloned() else {
                continue;
            };
            let normalized = normalize_chart_candles(candles);
            let fetched_at = payload.last_fetch_at.get(&key).cloned();
            self.insert_chart_cache_entry(key.clone(), normalized.clone(), fetched_at);

            if let Some((pair, timeframe)) = parse_chart_cache_key(&key) {
                if let Some(market) = self.market_data.get_mut(&pair) {
                    if let Some(buffer) = market.get_candles_mut(timeframe) {
                        buffer.candles.clear();
                        for candle in &normalized {
                            buffer.push(candle.clone());
                        }
                    }
                }
            }
        }
    }

    fn apply_chart_series_update(
        &mut self,
        pair: &str,
        timeframe: Timeframe,
        candles: Vec<OHLCV>,
        fetched_at: DateTime<Utc>,
    ) {
        let normalized = normalize_chart_candles(candles);
        let key = chart_cache_key(pair, timeframe);

        self.insert_chart_cache_entry(key, normalized.clone(), Some(fetched_at));

        if let Some(market) = self.market_data.get_mut(pair) {
            if let Some(buffer) = market.get_candles_mut(timeframe) {
                buffer.candles.clear();
                for candle in normalized {
                    buffer.push(candle);
                }
            }
        }
    }

    fn insert_chart_cache_entry(
        &mut self,
        key: String,
        candles: Vec<OHLCV>,
        fetched_at: Option<DateTime<Utc>>,
    ) {
        self.chart_cache.insert(key.clone(), candles);

        if let Some(ts) = fetched_at {
            self.chart_last_fetch_at.insert(key.clone(), ts);
        }

        self.touch_chart_cache_key(&key);
    }

    fn touch_chart_cache_key(&mut self, key: &str) {
        if let Some(idx) = self.chart_cache_lru.iter().position(|k| k == key) {
            self.chart_cache_lru.remove(idx);
        }
        self.chart_cache_lru.push_back(key.to_string());

        while self.chart_cache_lru.len() > CHART_CACHE_KEY_CAP {
            if let Some(evicted) = self.chart_cache_lru.pop_front() {
                self.chart_cache.remove(&evicted);
                self.chart_last_fetch_at.remove(&evicted);
            }
        }
    }

    fn apply_context_to_signal(&self, signal: &mut Signal) {
        if let Some(sentiment) = &self.sentiment_score {
            let signed = (sentiment.composite * 15.0).round().clamp(-15.0, 15.0) as i8;
            let normalized = (((sentiment.composite + 1.0) / 2.0) * 15.0)
                .round()
                .clamp(0.0, 15.0) as u8;
            signal.confidence_breakdown.sentiment = normalized;
            signal.reasoning.push(ReasonEntry::new(
                AnalysisType::Sentiment,
                "Sentiment composite",
                signed,
                format!(
                    "Composite {:+.2} from {}",
                    sentiment.composite,
                    if sentiment.sources_available.is_empty() {
                        "no sources".to_string()
                    } else {
                        sentiment.sources_available.join(", ")
                    }
                ),
            ));
        }

        let mut macro_signed = 0i8;
        let mut has_macro_input = false;

        if let Some(spy) = self.macro_context.spy_change_pct {
            has_macro_input = true;
            if spy > 0.05 {
                macro_signed += 3;
            } else if spy < -0.05 {
                macro_signed -= 3;
            }
        }
        if let Some(dxy) = self.macro_context.dxy_change_pct {
            has_macro_input = true;
            if dxy < -0.05 {
                macro_signed += 3;
            } else if dxy > 0.05 {
                macro_signed -= 3;
            }
        }
        if let Some(vix) = self.macro_context.vix {
            has_macro_input = true;
            if vix < 20.0 {
                macro_signed += 2;
            } else if vix > 25.0 {
                macro_signed -= 2;
            }
        }
        if let Some(dom) = self.macro_context.btc_dominance {
            has_macro_input = true;
            if dom >= 50.0 {
                macro_signed += 2;
            } else if dom <= 45.0 {
                macro_signed -= 1;
            }
        }

        if has_macro_input {
            let signed = macro_signed.clamp(-10, 10);
            let normalized = (((signed as f32 + 10.0) / 2.0).round()).clamp(0.0, 10.0) as u8;
            signal.confidence_breakdown.macro_score = normalized;
            signal.reasoning.push(ReasonEntry::new(
                AnalysisType::Macro,
                "Macro context",
                signed,
                format!(
                    "SPY={} DXY={} VIX={} BTC.D={}",
                    self.macro_context
                        .spy_change_pct
                        .map(|v| format!("{:+.2}%", v))
                        .unwrap_or_else(|| "n/a".to_string()),
                    self.macro_context
                        .dxy_change_pct
                        .map(|v| format!("{:+.2}%", v))
                        .unwrap_or_else(|| "n/a".to_string()),
                    self.macro_context
                        .vix
                        .map(|v| format!("{:.2}", v))
                        .unwrap_or_else(|| "n/a".to_string()),
                    self.macro_context
                        .btc_dominance
                        .map(|v| format!("{:.2}%", v))
                        .unwrap_or_else(|| "n/a".to_string()),
                ),
            ));
        }

        let adjusted_confidence = signal.confidence_breakdown.total();
        signal.confidence = adjusted_confidence;
        if signal.action == SignalAction::Execute
            && adjusted_confidence < self.config.agent.min_confidence
        {
            signal.action = SignalAction::Watch;
            signal.skip_reason = Some(format!(
                "Adjusted confidence {} below threshold {}",
                adjusted_confidence, self.config.agent.min_confidence
            ));
        }
    }

    /// Starts a new user chat message.
    pub fn send_user_message(&mut self, content: String) {
        push_bounded(
            &mut self.chat_messages,
            ChatMessage::user(content),
            CHAT_HISTORY_CAP,
        );
        push_bounded(
            &mut self.chat_messages,
            ChatMessage::agent_streaming(),
            CHAT_HISTORY_CAP,
        );
        self.is_agent_thinking = true;
        self.chat_input.clear();
    }

    /// Gets the ticker for a pair.
    pub fn get_ticker(&self, pair: &str) -> Option<&Ticker> {
        self.market_data.get(pair).map(|m| &m.ticker)
    }

    /// Gets candles for a pair and timeframe.
    pub fn get_candles(&self, pair: &str, timeframe: Timeframe) -> Option<&CandleBuffer> {
        self.market_data
            .get(pair)
            .and_then(|m| m.get_candles(timeframe))
    }

    /// Gets the current chart candles.
    pub fn current_chart_candles(&self) -> Option<&CandleBuffer> {
        self.get_candles(&self.chart_pair, self.chart_timeframe)
    }

    /// Checks if the daily drawdown limit has been hit.
    pub fn is_drawdown_limit_hit(&self) -> bool {
        let limit = self.config.risk.max_daily_drawdown_pct;
        let realized_loss = if self.portfolio.daily_realized_pnl < Decimal::ZERO {
            self.portfolio.daily_realized_pnl.abs()
        } else {
            Decimal::ZERO
        };

        let daily_loss_pct = if self.portfolio.total_value() > Decimal::ZERO {
            (realized_loss / self.portfolio.total_value()) * Decimal::from(100)
        } else {
            Decimal::ZERO
        };
        daily_loss_pct >= limit
    }
}

/// State update messages sent from background tasks to the main thread.
///
/// All state mutations flow through this enum. The main loop receives these
/// via channels and applies them to `AppState`.
#[derive(Debug, Clone)]
pub enum StateUpdate {
    // ─────────────────────────────────────────────────────────────
    // Market Data Updates
    // ─────────────────────────────────────────────────────────────
    /// New ticker data received.
    MarketTick(Ticker),

    /// Candle update for a pair/timeframe.
    CandleUpdate {
        pair: String,
        timeframe: Timeframe,
        candle: OHLCV,
    },

    // ─────────────────────────────────────────────────────────────
    // Trading Updates
    // ─────────────────────────────────────────────────────────────
    /// New signal generated.
    NewSignal(Signal),

    /// Position was opened.
    PositionOpened(Position),

    /// Position was updated (price change, trailing stop, etc).
    PositionUpdated(Position),

    /// Position was closed.
    PositionClosed {
        position_id: Uuid,
        exit_price: Decimal,
        reason: CloseReason,
    },

    // ─────────────────────────────────────────────────────────────
    // Chat Updates
    // ─────────────────────────────────────────────────────────────
    /// Streaming token from LLM.
    ChatToken(String),

    /// LLM response complete.
    ChatDone,

    /// Chat/LLM error.
    ChatError(String),

    // ─────────────────────────────────────────────────────────────
    // Team Discussion Updates
    // ─────────────────────────────────────────────────────────────
    /// Team discussion session started.
    TeamSessionStarted {
        /// Prompt entered by the user.
        prompt: String,
        /// Monotonic session id.
        session_id: u64,
    },

    /// Agent status changed in Team Discussion.
    TeamAgentStatusChanged {
        /// Role being updated.
        role: TeamRole,
        /// New role status.
        status: TeamAgentStatus,
        /// Session id this status belongs to.
        session_id: u64,
    },

    /// Team agent produced a new message.
    TeamMessage {
        /// Role speaking.
        role: TeamRole,
        /// Phase number (1 = debate, 2 = synthesis).
        phase: u8,
        /// Message body.
        content: String,
        /// Session id this message belongs to.
        session_id: u64,
    },

    /// Team relationship graph recalculated.
    TeamRelationshipsUpdated {
        /// Weighted agree/counter edges.
        edges: Vec<TeamRelationEdge>,
        /// Session id these edges belong to.
        session_id: u64,
    },

    /// Leader proposed action card (awaits popup decision).
    TeamActionProposed {
        /// Proposed action card.
        card: TeamActionCard,
        /// Session id this action belongs to.
        session_id: u64,
    },

    /// Team summary scorecard generated at session completion.
    TeamSummary {
        /// Session summary.
        summary: TeamSessionSummary,
        /// Session id this summary belongs to.
        session_id: u64,
    },

    /// Team discussion session completed.
    TeamSessionCompleted {
        /// Session id completed.
        session_id: u64,
    },

    /// Team discussion session error.
    TeamSessionError {
        /// Error text.
        error: String,
        /// Session id that failed.
        session_id: u64,
    },

    /// Clear pending team action card.
    TeamActionCleared,

    /// Update user decision for latest matching history entry.
    TeamHistoryDecisionUpdated {
        /// Session timestamp key.
        timestamp: DateTime<Utc>,
        /// Decision label.
        decision: String,
    },

    // ─────────────────────────────────────────────────────────────
    // Status Updates
    // ─────────────────────────────────────────────────────────────
    /// Agent status changed.
    AgentStatusChanged(AgentStatus),

    /// Production signal engine status update.
    EngineStatusUpdated(EngineStatus),

    /// Websocket feed telemetry updates mirrored into EngineStatus.
    WsFeedTelemetry {
        reconnect_count: u64,
        last_message_at: DateTime<Utc>,
        uptime_ratio: f32,
    },

    /// Agent encountered an error.
    AgentError(String),

    /// Market data feed connection status.
    FeedStatusChanged(ConnectionStatus),

    /// LLM connection status.
    LlmStatusChanged(ConnectionStatus),

    /// Funding rate update.
    FundingRateUpdate {
        pair: String,
        rate: Decimal,
        next_time: DateTime<Utc>,
    },

    // ─────────────────────────────────────────────────────────────
    // Other Updates
    // ─────────────────────────────────────────────────────────────
    /// Macro context update (Fear & Greed, dominance).
    MacroUpdate(MacroContext),

    /// Sentiment score update.
    SentimentUpdate(SentimentScore),

    /// Latest headlines update.
    NewsUpdate(Vec<NewsHeadline>),

    /// Cached news history loaded from disk at startup.
    NewsHistoryLoaded {
        headlines: Vec<NewsHeadline>,
        last_fetch_at: Option<DateTime<Utc>>,
    },

    /// News refresh cycle started.
    NewsRefreshStarted,

    /// News refresh cycle completed successfully.
    NewsRefreshCompleted { fetched_at: DateTime<Utc> },

    /// Chart cache loaded from disk at startup.
    ChartCacheLoaded(ChartCache),

    /// Replace full chart series for pair/timeframe.
    ChartSeriesUpdate {
        pair: String,
        timeframe: Timeframe,
        candles: Vec<OHLCV>,
        fetched_at: DateTime<Utc>,
    },

    /// Source health update.
    SourceHealthChanged(SourceStatus),

    /// Data quality score update.
    DataQualityUpdated { pair: String, score: f32 },

    /// Configuration changed.
    ConfigChanged(Box<Config>),

    /// Authentication state changed for one provider.
    AuthStateChanged {
        /// Provider whose state changed.
        provider: AuthProvider,
        /// New state for provider.
        status: AuthStatus,
    },

    /// Refreshed list of OpenRouter free models.
    OpenRouterFreeModelsUpdated(Vec<String>),

    /// Log entry.
    Log(LogEntry),
}

/// Cache key helper for chart series persistence.
pub fn chart_cache_key(pair: &str, timeframe: Timeframe) -> String {
    format!(
        "{}|{}",
        pair.to_ascii_uppercase(),
        timeframe.as_binance_interval()
    )
}

fn parse_chart_cache_key(key: &str) -> Option<(String, Timeframe)> {
    let (pair, interval) = key.split_once('|')?;
    let timeframe = Timeframe::from_binance_interval(interval)?;
    Some((pair.to_ascii_uppercase(), timeframe))
}

fn chart_cache_load_order(
    series: &HashMap<String, Vec<OHLCV>>,
    persisted_lru_order: Vec<String>,
) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut ordered: Vec<String> = persisted_lru_order
        .into_iter()
        .filter(|key| series.contains_key(key) && seen.insert(key.clone()))
        .collect();

    if ordered.is_empty() {
        let mut fallback: Vec<String> = series.keys().cloned().collect();
        fallback.sort();
        return fallback;
    }

    let mut missing: Vec<String> = series
        .keys()
        .filter(|key| !seen.contains(*key))
        .cloned()
        .collect();
    missing.sort();
    missing.extend(ordered);
    ordered = missing;

    ordered
}

fn canonical_news_key(headline: &NewsHeadline) -> String {
    if let Some(url) = &headline.url {
        let normalized = url.trim().to_ascii_lowercase();
        if !normalized.is_empty() {
            return format!("url:{}", normalized);
        }
    }

    format!(
        "fallback:{}:{}:{}",
        headline.source.trim().to_ascii_lowercase(),
        headline.title.trim().to_ascii_lowercase(),
        headline.published_at.timestamp() / 60
    )
}

/// Deduplicates/sorts headlines newest-first and truncates to `limit`.
pub fn normalize_news_items(mut items: Vec<NewsHeadline>, limit: usize) -> Vec<NewsHeadline> {
    let mut seen = std::collections::HashSet::new();
    items.retain(|item| seen.insert(canonical_news_key(item)));
    items.sort_by_key(|item| std::cmp::Reverse(item.published_at));
    items.truncate(limit);
    items
}

fn normalize_chart_candles(mut candles: Vec<OHLCV>) -> Vec<OHLCV> {
    candles.sort_by_key(|c| c.timestamp);
    if candles.len() > CHART_SERIES_CAP {
        let keep_from = candles.len() - CHART_SERIES_CAP;
        candles = candles.split_off(keep_from);
    }
    candles
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> Config {
        Config::default()
    }

    #[test]
    fn test_app_state_creation() {
        let state = AppState::new(test_config());
        assert_eq!(state.agent_status, AgentStatus::Running);
        assert_eq!(state.portfolio.cash, Decimal::from(10000));
        assert!(!state.market_data.is_empty());
    }

    #[test]
    fn test_market_tick_update() {
        let mut state = AppState::new(test_config());

        let ticker = Ticker {
            pair: "BTCUSDT".to_string(),
            price: Decimal::from(50000),
            ..Ticker::new("BTCUSDT".to_string())
        };

        state.apply_update(StateUpdate::MarketTick(ticker));

        let stored = state.get_ticker("BTCUSDT").unwrap();
        assert_eq!(stored.price, Decimal::from(50000));
    }

    #[test]
    fn test_chat_message_flow() {
        let mut state = AppState::new(test_config());

        state.send_user_message("Hello".to_string());
        assert_eq!(state.chat_messages.len(), 2);
        assert!(state.is_agent_thinking);

        state.apply_update(StateUpdate::ChatToken("Hi ".to_string()));
        state.apply_update(StateUpdate::ChatToken("there!".to_string()));
        state.apply_update(StateUpdate::ChatDone);

        assert!(!state.is_agent_thinking);
        let agent_msg = &state.chat_messages[1];
        assert_eq!(agent_msg.content, "Hi there!");
    }

    #[test]
    fn test_log_trimming() {
        let mut config = test_config();
        config.tui.log_lines = 3;
        let mut state = AppState::new(config);

        for i in 0..5 {
            state.apply_update(StateUpdate::Log(LogEntry::info(format!("Log {}", i))));
        }

        assert_eq!(state.log_entries.len(), 3);
        assert!(state.log_entries[0].message.contains("Log 2"));
    }

    #[test]
    fn test_engine_status_update_applies_to_state() {
        let mut state = AppState::new(test_config());
        let status = EngineStatus {
            active_indicators: vec!["EMA Crossover".to_string()],
            last_tick_time: Some(Utc::now()),
            consecutive_errors: 2,
            circuit_breaker_open: true,
            last_error: Some("test".to_string()),
            ws_reconnect_count: 4,
            ws_last_message_at: Some(Utc::now()),
            ws_uptime_ratio: 0.88,
        };
        state.apply_update(StateUpdate::EngineStatusUpdated(status.clone()));
        assert_eq!(
            state.engine_status.circuit_breaker_open,
            status.circuit_breaker_open
        );
        assert_eq!(state.engine_status.consecutive_errors, 2);
        assert_eq!(state.engine_status.ws_reconnect_count, 4);
    }

    #[test]
    fn test_chart_cache_key_format() {
        assert_eq!(chart_cache_key("btcusdt", Timeframe::H1), "BTCUSDT|1h");
        assert_eq!(chart_cache_key("ETHUSDT", Timeframe::MO1), "ETHUSDT|1M");
    }

    #[test]
    fn test_parse_chart_cache_key() {
        let parsed = parse_chart_cache_key("BTCUSDT|1h").unwrap();
        assert_eq!(parsed.0, "BTCUSDT");
        assert_eq!(parsed.1, Timeframe::H1);
        assert!(parse_chart_cache_key("bad-key").is_none());
    }

    #[test]
    fn test_news_normalization_dedup_and_order() {
        let now = Utc::now();
        let old = now - chrono::Duration::minutes(20);

        let a = NewsHeadline {
            source: "A".to_string(),
            title: "Alpha".to_string(),
            url: Some("https://example.com/x".to_string()),
            published_at: old,
            sentiment: None,
        };
        let b = NewsHeadline {
            source: "B".to_string(),
            title: "Beta".to_string(),
            url: Some("https://example.com/y".to_string()),
            published_at: now,
            sentiment: None,
        };
        let dup_a = NewsHeadline {
            source: "A2".to_string(),
            title: "Alpha duplicate".to_string(),
            url: Some("https://example.com/x".to_string()),
            published_at: now,
            sentiment: None,
        };

        let normalized = normalize_news_items(vec![a, b.clone(), dup_a], 1000);
        assert_eq!(normalized.len(), 2);
        assert_eq!(normalized[0].title, b.title);
    }

    #[test]
    fn test_apply_chart_series_update_populates_market_buffer() {
        let mut state = AppState::new(test_config());
        let pair = "BTCUSDT";
        let candles = vec![
            OHLCV {
                timestamp: Utc::now() - chrono::Duration::hours(2),
                open: Decimal::from(100),
                high: Decimal::from(120),
                low: Decimal::from(90),
                close: Decimal::from(110),
                volume: Decimal::from(10),
                trades: 5,
                closed: true,
            },
            OHLCV {
                timestamp: Utc::now() - chrono::Duration::hours(1),
                open: Decimal::from(110),
                high: Decimal::from(130),
                low: Decimal::from(100),
                close: Decimal::from(120),
                volume: Decimal::from(12),
                trades: 6,
                closed: true,
            },
        ];

        state.apply_update(StateUpdate::ChartSeriesUpdate {
            pair: pair.to_string(),
            timeframe: Timeframe::H1,
            candles: candles.clone(),
            fetched_at: Utc::now(),
        });

        let key = chart_cache_key(pair, Timeframe::H1);
        assert!(state.chart_cache.contains_key(&key));
        let buffer = state.get_candles(pair, Timeframe::H1).unwrap();
        assert_eq!(buffer.len(), candles.len());
    }

    #[test]
    fn test_chart_series_truncated_to_200() {
        let mut state = AppState::new(test_config());
        let pair = state.chart_pair.clone();
        let now = Utc::now();
        let candles: Vec<OHLCV> = (0i64..350)
            .map(|i| OHLCV {
                timestamp: now - chrono::Duration::minutes((350 - i) as i64),
                open: Decimal::from(i),
                high: Decimal::from(i + 1),
                low: Decimal::from(i.saturating_sub(1)),
                close: Decimal::from(i),
                volume: Decimal::ONE,
                trades: 1,
                closed: true,
            })
            .collect();

        state.apply_update(StateUpdate::ChartSeriesUpdate {
            pair,
            timeframe: Timeframe::H1,
            candles,
            fetched_at: now,
        });

        let key = chart_cache_key(&state.chart_pair, Timeframe::H1);
        assert!(state.chart_cache.get(&key).map(|v| v.len()).unwrap_or(0) <= 200);
    }

    #[test]
    fn test_chart_cache_lru_evicts_over_50_keys() {
        let mut state = AppState::new(test_config());

        for i in 0..55 {
            let pair = format!("X{i}USDT");
            state.apply_update(StateUpdate::ChartSeriesUpdate {
                pair,
                timeframe: Timeframe::H1,
                candles: vec![OHLCV {
                    timestamp: Utc::now(),
                    open: Decimal::ONE,
                    high: Decimal::ONE,
                    low: Decimal::ONE,
                    close: Decimal::ONE,
                    volume: Decimal::ONE,
                    trades: 1,
                    closed: true,
                }],
                fetched_at: Utc::now(),
            });
        }

        assert!(state.chart_cache.len() <= 50);
        assert!(state.chart_last_fetch_at.len() <= 50);
    }

    #[test]
    fn test_chart_cache_loaded_restores_lru_and_last_fetch() {
        let mut state = AppState::new(test_config());
        let now = Utc::now();

        let mut series = HashMap::new();
        let mut last_fetch_at = HashMap::new();

        for i in 0..3 {
            let key = format!("X{i}USDT|1h");
            series.insert(
                key.clone(),
                vec![OHLCV {
                    timestamp: now,
                    open: Decimal::ONE,
                    high: Decimal::ONE,
                    low: Decimal::ONE,
                    close: Decimal::ONE,
                    volume: Decimal::ONE,
                    trades: 1,
                    closed: true,
                }],
            );
            last_fetch_at.insert(key, now);
        }

        state.apply_update(StateUpdate::ChartCacheLoaded(ChartCache {
            series,
            last_fetch_at,
            lru_order: vec!["X1USDT|1h".to_string(), "X0USDT|1h".to_string()],
            cached_at: now,
        }));

        assert_eq!(
            state
                .chart_cache_lru
                .iter()
                .cloned()
                .collect::<Vec<String>>(),
            vec![
                "X2USDT|1h".to_string(),
                "X1USDT|1h".to_string(),
                "X0USDT|1h".to_string()
            ]
        );
        assert_eq!(state.chart_last_fetch_at.len(), 3);
    }

    #[test]
    fn test_chart_cache_loaded_without_lru_uses_sorted_fallback() {
        let mut state = AppState::new(test_config());
        let now = Utc::now();

        let mut series = HashMap::new();
        series.insert("BBBUSDT|1h".to_string(), Vec::new());
        series.insert("AAAUSDT|1h".to_string(), Vec::new());

        state.apply_update(StateUpdate::ChartCacheLoaded(ChartCache {
            series,
            last_fetch_at: HashMap::new(),
            lru_order: Vec::new(),
            cached_at: now,
        }));

        assert_eq!(
            state
                .chart_cache_lru
                .iter()
                .cloned()
                .collect::<Vec<String>>(),
            vec!["AAAUSDT|1h".to_string(), "BBBUSDT|1h".to_string()]
        );
    }

    #[test]
    fn test_chat_history_capped_at_200() {
        let mut state = AppState::new(test_config());
        for i in 0..240 {
            state.send_user_message(format!("msg-{i}"));
            state.apply_update(StateUpdate::ChatDone);
        }

        assert!(state.chat_messages.len() <= 200);
    }

    #[test]
    fn test_news_history_capped_at_500() {
        let mut state = AppState::new(test_config());
        let base = Utc::now();
        let mut items = Vec::new();

        for i in 0..650 {
            items.push(NewsHeadline {
                source: "src".to_string(),
                title: format!("news-{i}"),
                url: Some(format!("https://example.com/{i}")),
                published_at: base - chrono::Duration::minutes(i as i64),
                sentiment: None,
            });
        }

        state.apply_update(StateUpdate::NewsUpdate(items));
        assert!(state.news_history.len() <= 500);
    }

    #[test]
    fn test_drawdown_limit_ignores_profitable_realized_pnl() {
        let mut state = AppState::new(test_config());
        state.config.risk.max_daily_drawdown_pct = Decimal::new(5, 0);
        state.portfolio.daily_realized_pnl = Decimal::from(500);
        state.portfolio.cash = Decimal::from(10000);

        assert!(!state.is_drawdown_limit_hit());
    }

    #[test]
    fn test_drawdown_limit_triggers_on_realized_losses_only() {
        let mut state = AppState::new(test_config());
        state.config.risk.max_daily_drawdown_pct = Decimal::new(4, 0);
        state.portfolio.daily_realized_pnl = Decimal::from(-500);
        state.portfolio.cash = Decimal::from(10000);

        assert!(state.is_drawdown_limit_hit());
    }
}

fn sentiment_agreement_score(sentiment: Option<&SentimentScore>) -> f32 {
    let Some(sentiment) = sentiment else {
        return 0.5;
    };
    let mut values: Vec<f32> = Vec::new();
    if let Some(v) = sentiment.reddit_score.filter(|v| v.is_finite()) {
        values.push(v.clamp(-1.0, 1.0));
    }
    if let Some(v) = sentiment.twitter_score.filter(|v| v.is_finite()) {
        values.push(v.clamp(-1.0, 1.0));
    }
    if let Some(v) = sentiment.news_score.filter(|v| v.is_finite()) {
        values.push(v.clamp(-1.0, 1.0));
    }
    if let Some(v) = sentiment.fear_greed {
        let mapped = (v as f32 - 50.0) / 50.0;
        if mapped.is_finite() {
            values.push(mapped.clamp(-1.0, 1.0));
        }
    }

    if values.len() < 2 {
        return if values.is_empty() { 0.5 } else { 0.6 };
    }

    let mean = values.iter().sum::<f32>() / values.len() as f32;
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / values.len() as f32;
    let stddev = variance.sqrt();
    (1.0 - stddev.min(1.0)).clamp(0.0, 1.0)
}
