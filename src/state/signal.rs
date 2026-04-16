//! Signal and analysis reasoning types.
//!
//! This module defines:
//! - `Signal` - a trading signal with direction, targets, and confidence
//! - `SignalDirection` - long, short, or wait
//! - `SignalAction` - execute, skip, or watch
//! - `ReasonEntry` - individual reasoning step from analysis
//! - `ConfidenceScore` - breakdown of confidence by analysis type

use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Direction of a trading signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SignalDirection {
    /// Bullish signal - go long.
    Long,
    /// Bearish signal - go short.
    Short,
    /// No clear direction - wait.
    Wait,
}

impl std::fmt::Display for SignalDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SignalDirection::Long => write!(f, "LONG"),
            SignalDirection::Short => write!(f, "SHORT"),
            SignalDirection::Wait => write!(f, "WAIT"),
        }
    }
}

/// Action to take based on the signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SignalAction {
    /// Execute the trade.
    Execute,
    /// Skip this signal (risk guards triggered).
    Skip,
    /// Monitor but don't execute yet.
    Watch,
}

impl std::fmt::Display for SignalAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SignalAction::Execute => write!(f, "Execute"),
            SignalAction::Skip => write!(f, "Skip"),
            SignalAction::Watch => write!(f, "Watch"),
        }
    }
}

/// Analysis type for confidence scoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisType {
    /// Trend analysis (EMAs, direction).
    Trend,
    /// Momentum indicators (RSI, etc.).
    Momentum,
    /// Volume analysis.
    Volume,
    /// Market sentiment (Fear & Greed).
    Sentiment,
    /// Macro correlation (BTC dominance, etc.).
    Macro,
    /// Support/resistance levels.
    Levels,
    /// Chart patterns.
    Patterns,
}

impl std::fmt::Display for AnalysisType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnalysisType::Trend => write!(f, "Trend"),
            AnalysisType::Momentum => write!(f, "Momentum"),
            AnalysisType::Volume => write!(f, "Volume"),
            AnalysisType::Sentiment => write!(f, "Sentiment"),
            AnalysisType::Macro => write!(f, "Macro"),
            AnalysisType::Levels => write!(f, "Levels"),
            AnalysisType::Patterns => write!(f, "Patterns"),
        }
    }
}

/// A single reasoning step from the analysis pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasonEntry {
    /// Type of analysis that produced this reason.
    pub analysis_type: AnalysisType,

    /// Step name (e.g., "EMA Alignment").
    pub step_name: String,

    /// Score contribution from this step (-100 to +100).
    pub score: i8,

    /// Human-readable explanation.
    pub detail: String,

    /// Whether this step flagged a warning.
    pub is_warning: bool,

    /// Whether this step triggered a blacklist.
    pub is_blacklist: bool,
}

impl ReasonEntry {
    /// Creates a new reasoning entry.
    pub fn new(
        analysis_type: AnalysisType,
        step_name: impl Into<String>,
        score: i8,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            analysis_type,
            step_name: step_name.into(),
            score,
            detail: detail.into(),
            is_warning: false,
            is_blacklist: false,
        }
    }

    /// Marks this entry as a warning.
    pub fn with_warning(mut self) -> Self {
        self.is_warning = true;
        self
    }

    /// Marks this entry as triggering a blacklist.
    pub fn with_blacklist(mut self) -> Self {
        self.is_blacklist = true;
        self
    }
}

/// Breakdown of confidence score by analysis type.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConfidenceBreakdown {
    /// Trend analysis score (0-30).
    pub trend: u8,
    /// Momentum score (0-25).
    pub momentum: u8,
    /// Volume score (0-20).
    pub volume: u8,
    /// Sentiment score (0-15).
    pub sentiment: u8,
    /// Macro score (0-10).
    pub macro_score: u8,
}

impl ConfidenceBreakdown {
    /// Calculates total confidence from all components.
    pub fn total(&self) -> u8 {
        (self.trend as u16
            + self.momentum as u16
            + self.volume as u16
            + self.sentiment as u16
            + self.macro_score as u16)
            .min(100) as u8
    }
}

/// A trading signal with full reasoning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    /// Unique signal identifier.
    pub id: Uuid,

    /// Trading pair.
    pub pair: String,

    /// Signal direction (long/short/wait).
    pub direction: SignalDirection,

    /// Action to take.
    pub action: SignalAction,

    /// Overall confidence score (0-100).
    pub confidence: u8,

    /// Confidence breakdown by analysis type.
    pub confidence_breakdown: ConfidenceBreakdown,

    /// Suggested entry price.
    pub entry_price: Decimal,

    /// Suggested stop loss.
    pub stop_loss: Decimal,

    /// Suggested take profit.
    pub take_profit: Decimal,

    /// Risk-reward ratio.
    pub risk_reward: Decimal,

    /// Full reasoning chain.
    pub reasoning: Vec<ReasonEntry>,

    /// Skip reason if action is Skip.
    pub skip_reason: Option<String>,

    /// When the signal was generated.
    pub generated_at: DateTime<Utc>,

    /// When the signal expires.
    pub expires_at: DateTime<Utc>,

    /// Whether this signal has been executed.
    pub executed: bool,
}

impl Signal {
    /// Default signal expiration time in minutes.
    const DEFAULT_EXPIRY_MINUTES: i64 = 30;

    /// Creates a new signal builder.
    pub fn builder(pair: impl Into<String>) -> SignalBuilder {
        SignalBuilder::new(pair.into())
    }

    /// Returns true if the signal has expired.
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    /// Returns true if this signal can be executed.
    pub fn is_actionable(&self) -> bool {
        !self.is_expired()
            && !self.executed
            && self.action == SignalAction::Execute
            && self.direction != SignalDirection::Wait
    }

    /// Gets a summary of the reasoning.
    pub fn reasoning_summary(&self) -> String {
        self.reasoning
            .iter()
            .filter(|r| r.score.abs() >= 10)
            .map(|r| format!("{}: {} ({})", r.step_name, r.detail, r.score))
            .collect::<Vec<_>>()
            .join("; ")
    }

    /// Gets the strongest positive reason.
    pub fn strongest_bullish_reason(&self) -> Option<&ReasonEntry> {
        self.reasoning
            .iter()
            .filter(|r| r.score > 0)
            .max_by_key(|r| r.score)
    }

    /// Gets the strongest negative reason.
    pub fn strongest_bearish_reason(&self) -> Option<&ReasonEntry> {
        self.reasoning
            .iter()
            .filter(|r| r.score < 0)
            .min_by_key(|r| r.score)
    }

    /// Gets all warnings.
    pub fn warnings(&self) -> Vec<&ReasonEntry> {
        self.reasoning.iter().filter(|r| r.is_warning).collect()
    }

    /// Gets the calculated risk (distance from entry to stop).
    pub fn risk_amount(&self) -> Decimal {
        (self.entry_price - self.stop_loss).abs()
    }

    /// Gets the calculated reward (distance from entry to TP).
    pub fn reward_amount(&self) -> Decimal {
        (self.take_profit - self.entry_price).abs()
    }

    /// Marks the signal as executed.
    pub fn mark_executed(&mut self) {
        self.executed = true;
    }
}

/// Builder for creating signals.
pub struct SignalBuilder {
    pair: String,
    direction: SignalDirection,
    action: SignalAction,
    confidence: u8,
    confidence_breakdown: ConfidenceBreakdown,
    entry_price: Decimal,
    stop_loss: Decimal,
    take_profit: Decimal,
    reasoning: Vec<ReasonEntry>,
    skip_reason: Option<String>,
    expiry_minutes: i64,
}

impl SignalBuilder {
    /// Creates a new signal builder for the given pair.
    pub fn new(pair: String) -> Self {
        Self {
            pair,
            direction: SignalDirection::Wait,
            action: SignalAction::Watch,
            confidence: 0,
            confidence_breakdown: ConfidenceBreakdown::default(),
            entry_price: Decimal::ZERO,
            stop_loss: Decimal::ZERO,
            take_profit: Decimal::ZERO,
            reasoning: Vec::new(),
            skip_reason: None,
            expiry_minutes: Signal::DEFAULT_EXPIRY_MINUTES,
        }
    }

    /// Sets the signal direction.
    pub fn direction(mut self, direction: SignalDirection) -> Self {
        self.direction = direction;
        self
    }

    /// Sets the action.
    pub fn action(mut self, action: SignalAction) -> Self {
        self.action = action;
        self
    }

    /// Sets the confidence score.
    pub fn confidence(mut self, confidence: u8) -> Self {
        self.confidence = confidence.min(100);
        self
    }

    /// Sets the confidence breakdown.
    pub fn confidence_breakdown(mut self, breakdown: ConfidenceBreakdown) -> Self {
        self.confidence_breakdown = breakdown;
        self
    }

    /// Sets the entry price.
    pub fn entry_price(mut self, price: Decimal) -> Self {
        self.entry_price = price;
        self
    }

    /// Sets the stop loss.
    pub fn stop_loss(mut self, price: Decimal) -> Self {
        self.stop_loss = price;
        self
    }

    /// Sets the take profit.
    pub fn take_profit(mut self, price: Decimal) -> Self {
        self.take_profit = price;
        self
    }

    /// Adds a reasoning entry.
    pub fn add_reason(mut self, reason: ReasonEntry) -> Self {
        self.reasoning.push(reason);
        self
    }

    /// Sets the skip reason.
    pub fn skip_reason(mut self, reason: impl Into<String>) -> Self {
        self.skip_reason = Some(reason.into());
        self.action = SignalAction::Skip;
        self
    }

    /// Sets custom expiry time in minutes.
    pub fn expires_in_minutes(mut self, minutes: i64) -> Self {
        self.expiry_minutes = minutes;
        self
    }

    /// Builds the signal.
    pub fn build(self) -> Signal {
        let now = Utc::now();
        let risk = (self.entry_price - self.stop_loss).abs();
        let reward = (self.take_profit - self.entry_price).abs();
        let risk_reward = if risk > Decimal::ZERO {
            reward / risk
        } else {
            Decimal::ZERO
        };

        Signal {
            id: Uuid::new_v4(),
            pair: self.pair,
            direction: self.direction,
            action: self.action,
            confidence: self.confidence,
            confidence_breakdown: self.confidence_breakdown,
            entry_price: self.entry_price,
            stop_loss: self.stop_loss,
            take_profit: self.take_profit,
            risk_reward,
            reasoning: self.reasoning,
            skip_reason: self.skip_reason,
            generated_at: now,
            expires_at: now + Duration::minutes(self.expiry_minutes),
            executed: false,
        }
    }
}

/// Collection of recent signals for display.
#[derive(Debug, Clone, Default)]
pub struct SignalHistory {
    /// Recent signals, newest first.
    signals: Vec<Signal>,
    /// Maximum signals to keep.
    max_size: usize,
}

impl SignalHistory {
    /// Creates a new signal history with the given capacity.
    pub fn new(max_size: usize) -> Self {
        Self {
            signals: Vec::with_capacity(max_size),
            max_size,
        }
    }

    /// Adds a signal to the history.
    pub fn push(&mut self, signal: Signal) {
        self.signals.insert(0, signal);
        if self.signals.len() > self.max_size {
            self.signals.pop();
        }
    }

    /// Gets the most recent signal.
    pub fn latest(&self) -> Option<&Signal> {
        self.signals.first()
    }

    /// Gets the most recent signal for a pair.
    pub fn latest_for_pair(&self, pair: &str) -> Option<&Signal> {
        self.signals.iter().find(|s| s.pair == pair)
    }

    /// Gets all signals.
    pub fn all(&self) -> &[Signal] {
        &self.signals
    }

    /// Gets the n most recent signals.
    pub fn recent(&self, n: usize) -> &[Signal] {
        &self.signals[..n.min(self.signals.len())]
    }

    /// Returns the number of signals.
    pub fn len(&self) -> usize {
        self.signals.len()
    }

    /// Returns true if empty.
    pub fn is_empty(&self) -> bool {
        self.signals.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_builder() {
        let signal = Signal::builder("BTCUSDT")
            .direction(SignalDirection::Long)
            .action(SignalAction::Execute)
            .confidence(75)
            .entry_price(Decimal::from(50000))
            .stop_loss(Decimal::from(49000))
            .take_profit(Decimal::from(52000))
            .add_reason(ReasonEntry::new(
                AnalysisType::Trend,
                "EMA Alignment",
                25,
                "EMA20 > EMA50 > EMA200",
            ))
            .build();

        assert_eq!(signal.pair, "BTCUSDT");
        assert_eq!(signal.direction, SignalDirection::Long);
        assert_eq!(signal.confidence, 75);
        assert!(!signal.is_expired());
        assert!(signal.is_actionable());
    }

    #[test]
    fn test_risk_reward_calculation() {
        let signal = Signal::builder("BTCUSDT")
            .entry_price(Decimal::from(50000))
            .stop_loss(Decimal::from(49000)) // 1000 risk
            .take_profit(Decimal::from(52000)) // 2000 reward
            .build();

        assert_eq!(signal.risk_amount(), Decimal::from(1000));
        assert_eq!(signal.reward_amount(), Decimal::from(2000));
        assert_eq!(signal.risk_reward, Decimal::from(2));
    }

    #[test]
    fn test_signal_expiry() {
        let mut signal = Signal::builder("BTCUSDT").expires_in_minutes(-1).build();

        assert!(signal.is_expired());
        assert!(!signal.is_actionable());

        // Even with execute action, expired signals are not actionable
        signal.action = SignalAction::Execute;
        signal.direction = SignalDirection::Long;
        assert!(!signal.is_actionable());
    }

    #[test]
    fn test_confidence_breakdown() {
        let breakdown = ConfidenceBreakdown {
            trend: 25,
            momentum: 20,
            volume: 15,
            sentiment: 10,
            macro_score: 8,
        };

        assert_eq!(breakdown.total(), 78);
    }

    #[test]
    fn test_signal_history() {
        let mut history = SignalHistory::new(3);

        for i in 1..=5 {
            let signal = Signal::builder(format!("PAIR{}", i)).build();
            history.push(signal);
        }

        assert_eq!(history.len(), 3);
        assert_eq!(history.latest().unwrap().pair, "PAIR5");
    }
}
