//! Command system for the TUI.
//!
//! Handles parsing and execution of "/" commands.

use rust_decimal::Decimal;
use std::str::FromStr;

/// All supported commands.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    // Navigation commands
    Portfolio,
    Signals,
    Chart {
        pair: Option<String>,
        timeframe: Option<String>,
    },
    History {
        count: usize,
    },
    Stats,
    Customize,
    Help,
    Exit,

    // Action commands
    Pause,
    Resume,
    Buy {
        pair: String,
        size: Decimal,
    },
    Close {
        pair: String,
    },
    Add {
        pair: String,
    },
    Remove {
        pair: String,
    },
    Risk {
        percent: Decimal,
    },
    Confidence {
        threshold: u8,
    },
    Reset,

    // Model/Auth commands
    Model,
    Auth {
        action: Option<String>,
    },
    AuthDelete {
        provider: Option<String>,
    },
    Team {
        prompt: String,
    },
    TeamStatus,
    TeamHistory,

    // Utility commands
    Clear,
    Status,
    Heatmap,
    News,
    Sentiment,
    Macro,
    Log,
    Pairs,

    // Error case
    Unknown {
        input: String,
    },
}

/// Result of parsing user input.
#[derive(Debug, Clone)]
pub enum InputResult {
    /// A command was entered.
    Command(Command),
    /// A chat message was entered.
    ChatMessage(String),
    /// Empty input.
    Empty,
}

/// Parse user input into either a command or chat message.
pub fn parse_input(input: &str) -> InputResult {
    let trimmed = input.trim();

    if trimmed.is_empty() {
        return InputResult::Empty;
    }

    if trimmed.starts_with('/') {
        InputResult::Command(parse_command(trimmed))
    } else {
        InputResult::ChatMessage(trimmed.to_string())
    }
}

/// Parse a command string (starts with "/").
fn parse_command(input: &str) -> Command {
    let parts: Vec<&str> = input.split_whitespace().collect();
    let cmd = parts.first().map(|s| s.to_lowercase()).unwrap_or_default();
    let args = &parts[1..];

    match cmd.as_str() {
        // Navigation commands
        "/portfolio" | "/p" => Command::Portfolio,
        "/signals" | "/sig" => Command::Signals,
        "/chart" | "/c" => {
            let pair = args.first().map(|s| s.to_uppercase());
            let timeframe = args.get(1).map(|s| s.to_lowercase());
            Command::Chart { pair, timeframe }
        }
        "/history" | "/h" => {
            let count = args.first().and_then(|s| s.parse().ok()).unwrap_or(20);
            Command::History { count }
        }
        "/stats" => Command::Stats,
        "/customize" | "/settings" | "/set" => Command::Customize,
        "/help" | "/?" => Command::Help,
        "/exit" | "/quit" | "/q" => Command::Exit,

        // Action commands
        "/pause" => Command::Pause,
        "/resume" => Command::Resume,
        "/buy" => {
            if args.len() < 2 {
                return Command::Unknown {
                    input: "Usage: /buy <pair> <size>".to_string(),
                };
            }

            let pair = args[0].to_uppercase();
            let Ok(size) = Decimal::from_str(args[1]) else {
                return Command::Unknown {
                    input: "Invalid size value for /buy".to_string(),
                };
            };

            if size <= Decimal::ZERO {
                return Command::Unknown {
                    input: "Size must be > 0 for /buy".to_string(),
                };
            }

            Command::Buy { pair, size }
        }
        "/close" => {
            if let Some(pair) = args.first() {
                Command::Close {
                    pair: pair.to_uppercase(),
                }
            } else {
                Command::Unknown {
                    input: "Usage: /close <pair>".to_string(),
                }
            }
        }
        "/add" => {
            if let Some(pair) = args.first() {
                Command::Add {
                    pair: pair.to_uppercase(),
                }
            } else {
                Command::Unknown {
                    input: "Usage: /add <pair>".to_string(),
                }
            }
        }
        "/remove" => {
            if let Some(pair) = args.first() {
                Command::Remove {
                    pair: pair.to_uppercase(),
                }
            } else {
                Command::Unknown {
                    input: "Usage: /remove <pair>".to_string(),
                }
            }
        }
        "/risk" => {
            if let Some(pct_str) = args.first() {
                if let Ok(percent) = Decimal::from_str(pct_str) {
                    if percent > Decimal::ZERO && percent <= Decimal::from(10) {
                        Command::Risk { percent }
                    } else {
                        Command::Unknown {
                            input: "Risk must be between 0.1 and 10%".to_string(),
                        }
                    }
                } else {
                    Command::Unknown {
                        input: "Invalid percentage value".to_string(),
                    }
                }
            } else {
                Command::Unknown {
                    input: "Usage: /risk <percent>".to_string(),
                }
            }
        }
        "/confidence" | "/conf" => {
            if let Some(threshold_str) = args.first() {
                if let Ok(threshold) = threshold_str.parse::<u8>() {
                    if threshold <= 100 {
                        Command::Confidence { threshold }
                    } else {
                        Command::Unknown {
                            input: "Confidence must be 0-100".to_string(),
                        }
                    }
                } else {
                    Command::Unknown {
                        input: "Invalid confidence value".to_string(),
                    }
                }
            } else {
                Command::Unknown {
                    input: "Usage: /confidence <0-100>".to_string(),
                }
            }
        }
        "/reset" => Command::Reset,

        // Model/Auth commands
        "/model" => Command::Model,
        "/auth" => {
            let action = args.first().map(|s| s.to_lowercase());
            Command::Auth { action }
        }
        "/auth-delete" | "/auth-remove" => {
            let provider = args.first().map(|s| s.to_lowercase());
            Command::AuthDelete { provider }
        }
        "/team" => {
            if args.is_empty() {
                Command::Unknown {
                    input: "Usage: /team <prompt> or /team status".to_string(),
                }
            } else if args[0].eq_ignore_ascii_case("status") {
                Command::TeamStatus
            } else if args[0].eq_ignore_ascii_case("history") {
                Command::TeamHistory
            } else {
                Command::Team {
                    prompt: args.join(" "),
                }
            }
        }

        // Utility commands
        "/clear" | "/cls" => Command::Clear,
        "/status" => Command::Status,
        "/heatmap" | "/hm" => Command::Heatmap,
        "/news" => Command::News,
        "/sentiment" => Command::Sentiment,
        "/macro" => Command::Macro,
        "/log" => Command::Log,
        "/pairs" => Command::Pairs,

        // Unknown command
        _ => Command::Unknown {
            input: format!("Unknown command '{}' — type /help", cmd),
        },
    }
}

/// Get help text for all commands.
pub fn help_text() -> &'static str {
    r#"
NAVIGATION COMMANDS
  /portfolio, /p              Portfolio summary + open positions
  /signals, /sig              Latest signals with confidence bars
  /chart [pair] [tf], /c      Sparkline chart, e.g. /chart ETHUSDT 1h
  /history [n], /h            Last N closed trades (default 20)
  /stats                      Performance metrics
  /customize, /settings, /set Interactive config editor
  /help, /?                   This help page
  /exit, /quit, /q            Graceful shutdown

ACTION COMMANDS
  /pause                      Pause agent (stops signal execution)
  /resume                     Resume agent
  /buy <pair> <size>          Open long position, e.g. /buy BTCUSDT 0.1
  /close <pair>               Manually close position for pair
  /add <pair>                 Add pair to watchlist
  /remove <pair>              Remove pair from watchlist
  /risk <pct>                 Set risk per trade %, e.g. /risk 1.5
  /confidence <n>, /conf      Set min confidence threshold
  /model                      Select AI model and provider
  /auth                       Authentication provider UI
  /team <prompt>              Run AI Agent Team discussion
  /team status                Open Team Discussion page
  /team history               View last 5 Team discussion summaries
  /auth status                Show auth page
  /auth github                Start GitHub device auth
  /auth-delete [provider]     Delete auth (github/openai/gemini/...)
  /reset                      Reset portfolio to starting balance

UTILITY COMMANDS
  /clear, /cls                Clear current page / chat history
  /status                     Full system + source + auth health
  /heatmap, /hm               24h market heatmap grid
  /news                       Latest Finnhub/RSS headlines
  /sentiment                  Composite sentiment breakdown
  /macro                      Macro context (SPY/DXY/VIX/events)
  /log                        Show recent log entries (last 30)
  /pairs                      Show watchlist + blacklist

Chart TF keys (Chart page): 1=1H, 2=4H, 3=1D, 4=7D, 5=1M
News History: press H on /news page, / to filter cached headlines

Chat: Type without "/" prefix to send message to AI agent"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_portfolio() {
        assert!(matches!(
            parse_input("/portfolio"),
            InputResult::Command(Command::Portfolio)
        ));
    }

    #[test]
    fn test_parse_chart_with_args() {
        match parse_input("/chart ETHUSDT 1h") {
            InputResult::Command(Command::Chart { pair, timeframe }) => {
                assert_eq!(pair, Some("ETHUSDT".to_string()));
                assert_eq!(timeframe, Some("1h".to_string()));
            }
            _ => unreachable!("expected chart command"),
        }
    }

    #[test]
    fn test_parse_chat_message() {
        match parse_input("Hello, how are you?") {
            InputResult::ChatMessage(msg) => {
                assert_eq!(msg, "Hello, how are you?");
            }
            _ => unreachable!("expected chat message"),
        }
    }

    #[test]
    fn test_parse_unknown_command() {
        match parse_input("/xyz") {
            InputResult::Command(Command::Unknown { input }) => {
                assert!(input.contains("Unknown command"));
            }
            _ => unreachable!("expected unknown command"),
        }
    }

    #[test]
    fn test_parse_empty() {
        assert!(matches!(parse_input("   "), InputResult::Empty));
    }

    #[test]
    fn test_parse_history_default() {
        match parse_input("/history") {
            InputResult::Command(Command::History { count }) => {
                assert_eq!(count, 20);
            }
            _ => unreachable!("expected history command"),
        }
    }

    #[test]
    fn test_parse_history_with_count() {
        match parse_input("/history 50") {
            InputResult::Command(Command::History { count }) => {
                assert_eq!(count, 50);
            }
            _ => unreachable!("expected history command"),
        }
    }

    #[test]
    fn test_parse_model() {
        assert!(matches!(
            parse_input("/model"),
            InputResult::Command(Command::Model)
        ));
    }

    #[test]
    fn test_parse_auth() {
        match parse_input("/auth") {
            InputResult::Command(Command::Auth { action }) => {
                assert_eq!(action, None);
            }
            _ => unreachable!("expected auth command"),
        }

        match parse_input("/auth logout") {
            InputResult::Command(Command::Auth { action }) => {
                assert_eq!(action, Some("logout".to_string()));
            }
            _ => unreachable!("expected auth logout command"),
        }
    }

    #[test]
    fn test_parse_team_commands() {
        match parse_input("/team status") {
            InputResult::Command(Command::TeamStatus) => {}
            _ => unreachable!("expected team status command"),
        }

        match parse_input("/team history") {
            InputResult::Command(Command::TeamHistory) => {}
            _ => unreachable!("expected team history command"),
        }

        match parse_input("/team analyze btc and eth") {
            InputResult::Command(Command::Team { prompt }) => {
                assert_eq!(prompt, "analyze btc and eth".to_string());
            }
            _ => unreachable!("expected team prompt command"),
        }

        match parse_input("/team") {
            InputResult::Command(Command::Unknown { input }) => {
                assert!(input.contains("Usage: /team"));
            }
            _ => unreachable!("expected team usage error"),
        }
    }

    #[test]
    fn test_parse_heatmap_aliases() {
        assert!(matches!(
            parse_input("/heatmap"),
            InputResult::Command(Command::Heatmap)
        ));
        assert!(matches!(
            parse_input("/hm"),
            InputResult::Command(Command::Heatmap)
        ));
    }

    #[test]
    fn test_parse_buy_command() {
        match parse_input("/buy BTCUSDT 0.1") {
            InputResult::Command(Command::Buy { pair, size }) => {
                assert_eq!(pair, "BTCUSDT");
                assert_eq!(size, Decimal::new(1, 1));
            }
            _ => unreachable!("expected buy command"),
        }
    }

    #[test]
    fn test_parse_buy_command_invalid_size() {
        match parse_input("/buy BTCUSDT abc") {
            InputResult::Command(Command::Unknown { input }) => {
                assert!(input.contains("Invalid size"));
            }
            _ => unreachable!("expected buy size validation error"),
        }
    }
}
