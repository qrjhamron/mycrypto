//! Context builder for LLM conversations.
//!
//! Builds the system prompt and message context from application state.

use rust_decimal::Decimal;

use crate::state::AppState;

use super::llm::Message;

/// Maximum number of recent signals to include in context.
const MAX_SIGNALS_IN_CONTEXT: usize = 5;

/// Builds the system prompt with current market context.
pub fn build_system_prompt(state: &AppState) -> String {
    let mut prompt = String::new();

    // Core identity
    prompt.push_str("You are mycrypto, an AI-powered crypto paper trading assistant.\n");
    prompt.push_str("You help users understand market conditions, analyze signals, and manage their paper trading portfolio.\n\n");

    // Current status
    prompt.push_str("CURRENT STATUS:\n");
    prompt.push_str(&format!("- Agent: {}\n", state.agent_status));
    prompt.push_str(&format!(
        "- Portfolio: {} {} (cash), {} {} (positions)\n",
        format_decimal(state.portfolio.cash),
        state.config.portfolio.currency,
        format_decimal(state.portfolio.total_position_value()),
        state.config.portfolio.currency
    ));
    prompt.push_str(&format!(
        "- Open positions: {}\n",
        state.portfolio.positions.len()
    ));
    prompt.push_str(&format!(
        "- Total trades: {}\n",
        state.portfolio.trade_history.len()
    ));

    // Portfolio P/L
    let total_pnl = state.portfolio.total_unrealized_pnl();
    let pnl_str = if total_pnl >= Decimal::ZERO {
        format!("+{}", format_decimal(total_pnl))
    } else {
        format_decimal(total_pnl)
    };
    prompt.push_str(&format!(
        "- Unrealized P/L: {} {}\n",
        pnl_str, state.config.portfolio.currency
    ));

    // Watchlist and prices
    prompt.push_str("\nWATCHLIST PRICES:\n");
    for pair in &state.config.pairs.watchlist {
        if let Some(ticker) = state.get_ticker(pair) {
            let arrow = if ticker.is_up() { "↑" } else { "↓" };
            prompt.push_str(&format!(
                "- {}: ${} {} ({:+.2}% 24h)\n",
                pair,
                format_decimal(ticker.price),
                arrow,
                ticker.price_change_pct_24h
            ));
        }
    }

    // Sentiment context
    prompt.push_str("\nSENTIMENT CONTEXT:\n");
    if let Some(sentiment) = &state.sentiment_score {
        prompt.push_str(&format!(
            "- Composite: {:+.2} (updated: {})\n",
            sentiment.composite,
            sentiment.updated_at.format("%Y-%m-%d %H:%M UTC")
        ));
        prompt.push_str(&format!(
            "- Fear & Greed: {}\n",
            sentiment
                .fear_greed
                .map(|v| {
                    if let Some(label) = &sentiment.fear_greed_label {
                        format!("{} ({})", v, label)
                    } else {
                        v.to_string()
                    }
                })
                .unwrap_or_else(|| "n/a".to_string())
        ));
        prompt.push_str(&format!(
            "- Reddit: {}  X/Twitter: {}  News: {}\n",
            sentiment
                .reddit_score
                .map(|v| format!("{:+.2}", v))
                .unwrap_or_else(|| "n/a".to_string()),
            sentiment
                .twitter_score
                .map(|v| format!("{:+.2}", v))
                .unwrap_or_else(|| "n/a".to_string()),
            sentiment
                .news_score
                .map(|v| format!("{:+.2}", v))
                .unwrap_or_else(|| "n/a".to_string())
        ));
        prompt.push_str(&format!(
            "- Sources contributing: {}\n",
            if sentiment.sources_available.is_empty() {
                "none".to_string()
            } else {
                sentiment.sources_available.join(", ")
            }
        ));
    } else {
        prompt.push_str("- Sentiment data unavailable\n");
    }

    // Macro context
    prompt.push_str("\nMACRO CONTEXT:\n");
    prompt.push_str(&format!(
        "- SPY: {}\n",
        state
            .macro_context
            .spy_change_pct
            .map(|v| format!("{:+.2}% ({})", v, direction_label(v, true)))
            .unwrap_or_else(|| "n/a".to_string())
    ));
    prompt.push_str(&format!(
        "- DXY: {}\n",
        state
            .macro_context
            .dxy_change_pct
            .map(|v| format!("{:+.2}% ({})", v, direction_label(v, false)))
            .unwrap_or_else(|| "n/a".to_string())
    ));
    prompt.push_str(&format!(
        "- VIX: {}\n",
        state
            .macro_context
            .vix
            .map(|v| format!("{:.2}", v))
            .unwrap_or_else(|| "n/a".to_string())
    ));
    prompt.push_str(&format!(
        "- BTC dominance: {}\n",
        state
            .macro_context
            .btc_dominance
            .map(|v| format!("{:.2}%", v))
            .unwrap_or_else(|| "n/a".to_string())
    ));
    prompt.push_str(&format!(
        "- Total market cap: {}\n",
        state
            .macro_context
            .total_market_cap
            .map(format_large_usd)
            .unwrap_or_else(|| "n/a".to_string())
    ));

    if state.macro_context.upcoming_events.is_empty() {
        prompt.push_str("- Upcoming events: none\n");
    } else {
        prompt.push_str("- Upcoming events:\n");
        for event in state.macro_context.upcoming_events.iter().take(5) {
            prompt.push_str(&format!(
                "  • {} [{}] {} ({})\n",
                event.time.format("%Y-%m-%d"),
                event.impact,
                event.title,
                event.country
            ));
        }
    }

    // Latest news
    prompt.push_str("\nLATEST NEWS (TOP 5):\n");
    if state.news_headlines.is_empty() {
        prompt.push_str("- No recent headlines available\n");
    } else {
        for headline in state.news_headlines.iter().take(5) {
            prompt.push_str(&format!("- [{}] {}\n", headline.source, headline.title));
        }
    }

    // Unavailable data sources
    let mut unavailable_sources = state
        .source_health
        .values()
        .filter(|source| {
            !matches!(
                source.level,
                crate::state::SourceStatusLevel::Connected | crate::state::SourceStatusLevel::Ok
            )
        })
        .collect::<Vec<_>>();
    unavailable_sources.sort_by(|a, b| a.name.cmp(&b.name));

    prompt.push_str("\nDATA SOURCE AVAILABILITY:\n");
    if unavailable_sources.is_empty() {
        prompt.push_str("- All configured sources healthy\n");
    } else {
        for source in unavailable_sources {
            prompt.push_str(&format!(
                "- {}: {} ({})\n",
                source.name, source.level, source.detail
            ));
        }
    }

    // Open positions detail
    if !state.portfolio.positions.is_empty() {
        prompt.push_str("\nOPEN POSITIONS:\n");
        for pos in &state.portfolio.positions {
            let side = if pos.side == crate::state::PositionSide::Long {
                "LONG"
            } else {
                "SHORT"
            };
            let pnl_pct = pos.unrealized_pnl_pct;
            prompt.push_str(&format!(
                "- {} {}: entry ${}, current ${}, P/L {:+.2}%\n",
                pos.pair,
                side,
                format_decimal(pos.entry_price),
                format_decimal(pos.current_price),
                pnl_pct
            ));
        }
    }

    // Recent signals
    let recent_signals: Vec<_> = state
        .signal_history
        .recent(MAX_SIGNALS_IN_CONTEXT)
        .iter()
        .collect();
    if !recent_signals.is_empty() {
        prompt.push_str("\nRECENT SIGNALS:\n");
        for signal in recent_signals {
            prompt.push_str(&format!(
                "- {} {:?}: {} @ ${} (confidence: {}%)\n",
                signal.pair,
                signal.direction,
                signal.action,
                format_decimal(signal.entry_price),
                signal.confidence
            ));
        }
    }

    // Config summary
    prompt.push_str("\nCONFIGURATION:\n");
    prompt.push_str(&format!(
        "- Min confidence: {}%\n",
        state.config.agent.min_confidence
    ));
    prompt.push_str(&format!(
        "- Max open trades: {}\n",
        state.config.agent.max_open_trades
    ));
    prompt.push_str(&format!(
        "- Risk per trade: {}%\n",
        state.config.risk.risk_per_trade_pct
    ));

    // Instructions for commands
    prompt.push_str("\nCOMMAND INJECTION:\n");
    prompt.push_str("When the user asks you to perform an action, include the command in your response using [COMMAND:action] format.\n");
    prompt.push_str("Available commands:\n");
    prompt.push_str("- [COMMAND:pause] - Pause the trading agent\n");
    prompt.push_str("- [COMMAND:resume] - Resume the trading agent\n");
    prompt.push_str("- [COMMAND:status] - Show current status\n");
    prompt.push_str("- [COMMAND:portfolio] - Show portfolio page\n");
    prompt.push_str("- [COMMAND:signals] - Show signals page\n");
    prompt.push_str(
        "- [COMMAND:close PAIR] - Close position for PAIR (e.g., [COMMAND:close BTCUSDT])\n",
    );
    prompt.push_str(
        "\nRespond conversationally. Be helpful but concise. This is paper trading only.\n",
    );

    prompt
}

/// Builds conversation messages for LLM context.
pub fn build_messages(state: &AppState, user_input: &str) -> Vec<Message> {
    let mut messages = Vec::new();

    // System prompt
    messages.push(Message::system(build_system_prompt(state)));

    // Add recent chat history (limited by config)
    let max_history = state
        .config
        .llm
        .context_messages
        .min(state.chat_messages.len());
    let history_len = state.chat_messages.len();
    let history_start = history_len.saturating_sub(max_history);

    for msg in state.chat_messages.iter().skip(history_start) {
        if msg.is_user {
            messages.push(Message::user(&msg.content));
        } else if !msg.is_streaming && !msg.content.is_empty() {
            messages.push(Message::assistant(&msg.content));
        }
    }

    // Add current user input
    messages.push(Message::user(user_input));

    messages
}

/// Format a decimal for display.
fn format_decimal(d: Decimal) -> String {
    // Remove trailing zeros but keep reasonable precision
    let s = format!("{:.8}", d);
    let trimmed = s.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

fn direction_label(change_pct: f32, risk_on_when_positive: bool) -> &'static str {
    if change_pct.abs() < 0.05 {
        "flat"
    } else if risk_on_when_positive {
        if change_pct > 0.0 {
            "up"
        } else {
            "down"
        }
    } else if change_pct > 0.0 {
        "risk-off"
    } else {
        "risk-on"
    }
}

fn format_large_usd(value: f64) -> String {
    if value >= 1_000_000_000_000.0 {
        format!("${:.2}T", value / 1_000_000_000_000.0)
    } else if value >= 1_000_000_000.0 {
        format!("${:.2}B", value / 1_000_000_000.0)
    } else if value >= 1_000_000.0 {
        format!("${:.2}M", value / 1_000_000.0)
    } else {
        format!("${:.0}", value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::llm::Role;
    use crate::config::Config;

    #[test]
    fn test_build_system_prompt() {
        let state = AppState::new(Config::default());
        let prompt = build_system_prompt(&state);

        assert!(prompt.contains("mycrypto"));
        assert!(prompt.contains("CURRENT STATUS"));
        assert!(prompt.contains("WATCHLIST PRICES"));
        assert!(prompt.contains("COMMAND INJECTION"));
    }

    #[test]
    fn test_build_messages() {
        let state = AppState::new(Config::default());
        let messages = build_messages(&state, "What is BTC price?");

        assert!(!messages.is_empty());
        assert_eq!(messages[0].role, Role::System);
        assert_eq!(messages.last().unwrap().role, Role::User);
        assert!(messages.last().unwrap().content.contains("BTC"));
    }

    #[test]
    fn test_format_decimal() {
        assert_eq!(format_decimal(Decimal::from(100)), "100");
        assert_eq!(format_decimal(Decimal::new(12345, 2)), "123.45");
        assert_eq!(format_decimal(Decimal::new(100, 1)), "10");
    }
}
