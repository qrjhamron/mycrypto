//! mycrypto - AI-powered crypto paper trading companion for the terminal.
//!
//! This is the main entry point for the application. It handles:
//! - CLI argument parsing
//! - Configuration loading
//! - Logging initialization
//! - Runtime bootstrapping
//! - TUI initialization and event loop
//!
//! # Architecture
//!
//! The application follows an event-driven architecture:
//! - Background tasks handle market data, signals, and paper trading
//! - The main thread owns AppState and runs the TUI
//! - All communication happens via Tokio channels
//!
//! See the project README for full architecture documentation.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use mycrypto::config::{load_config, LlmProvider};
use mycrypto::state::AppState;
use mycrypto::tui;
use tracing::Level;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// CLI arguments for mycrypto.
#[derive(Parser, Debug)]
#[command(
    name = "mycrypto",
    version,
    about = "AI-powered crypto paper trading companion for the terminal",
    long_about = r#"
mycrypto is an AI-powered crypto paper trading companion that lives
entirely in your terminal. It watches the market, simulates trades
with virtual money, explains every decision it makes, and lets you
have a real conversation with it.

FEATURES:
  - Real-time market data from Binance
  - Paper trading with virtual portfolio
  - AI-powered analysis and explanations
  - Beautiful terminal UI

PAPER TRADING ONLY:
  This application is for educational and simulation purposes only.
  No real trades are ever executed.
"#
)]
struct Cli {
    /// Path to configuration file.
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Enable verbose logging.
    #[arg(short, long)]
    verbose: bool,

    /// Log level (trace, debug, info, warn, error).
    #[arg(long, default_value = "info")]
    log_level: String,

    /// Run in mock mode (no API calls).
    #[arg(long)]
    mock: bool,

    /// Path to log directory.
    #[arg(long, default_value = "~/.mycrypto/logs")]
    log_dir: String,

    /// Run without TUI (headless mode for testing).
    #[arg(long)]
    headless: bool,
}

/// Initializes the logging system.
///
/// Logs are written to rolling files (never stdout, as TUI owns the terminal).
/// The log directory is created if it doesn't exist.
fn init_logging(cli: &Cli) -> Result<()> {
    let log_dir = expand_tilde(&cli.log_dir);
    std::fs::create_dir_all(&log_dir).context("failed to create log directory")?;

    let file_appender = RollingFileAppender::new(Rotation::DAILY, &log_dir, "mycrypto.log");

    let level = if cli.verbose {
        Level::DEBUG
    } else {
        match cli.log_level.to_lowercase().as_str() {
            "trace" => Level::TRACE,
            "debug" => Level::DEBUG,
            "info" => Level::INFO,
            "warn" => Level::WARN,
            "error" => Level::ERROR,
            _ => Level::INFO,
        }
    };

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level.to_string()));

    let file_layer = fmt::layer()
        .with_writer(file_appender)
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(true)
        .json();

    tracing_subscriber::registry()
        .with(env_filter)
        .with(file_layer)
        .init();

    tracing::info!("logging initialized at level {}", level);
    Ok(())
}

/// Expands ~ to home directory.
fn expand_tilde(path: &str) -> PathBuf {
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(stripped);
        }
    }
    PathBuf::from(path)
}

/// Main entry point.
#[tokio::main]
async fn main() -> Result<()> {
    // Load .env file if present
    dotenvy::dotenv().ok();

    // Parse CLI arguments
    let cli = Cli::parse();

    // Initialize logging (to file, not stdout)
    init_logging(&cli)?;

    tracing::info!("mycrypto starting");
    tracing::info!("mock mode: {}", cli.mock);

    // Load configuration
    let config_path = cli.config.as_ref().map(|p| p.to_string_lossy().to_string());
    let mut config = load_config(config_path.as_deref()).context("failed to load configuration")?;

    // Override to mock LLM if --mock flag is set
    if cli.mock {
        config.llm.provider = LlmProvider::Mock;
        tracing::info!("using mock LLM provider");
    }

    tracing::info!("configuration loaded successfully");
    tracing::info!("watchlist: {:?}", config.pairs.watchlist);
    tracing::info!(
        "virtual balance: {} {}",
        config.portfolio.virtual_balance,
        config.portfolio.currency
    );

    // Create application state
    let state = AppState::new(config);

    tracing::info!(
        "application state initialized with {} pairs",
        state.market_data.len()
    );

    // Run in headless mode or TUI mode
    if cli.headless {
        println!("╔══════════════════════════════════════════════════════════════╗");
        println!("║                        mycrypto v0.1.0                       ║");
        println!("║        AI-powered crypto paper trading companion             ║");
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║  Running in headless mode (--headless)                       ║");
        println!("║                                                              ║");
        println!("║  Configuration loaded                                        ║");
        println!("║  State initialized                                           ║");
        println!("║  Logging active (see ~/.mycrypto/logs/)                      ║");
        println!("║                                                              ║");
        println!("║  Watchlist: {:?}", state.config.pairs.watchlist);
        println!(
            "║  Portfolio: {} {}",
            state.portfolio.cash, state.config.portfolio.currency
        );
        println!("║  Agent: {}", state.agent_status);
        println!("║                                                              ║");
        println!("╚══════════════════════════════════════════════════════════════╝");

        tracing::info!("headless mode complete, exiting");
    } else {
        // Run the TUI
        tracing::info!("starting TUI");

        if let Err(e) = tui::run(state) {
            tracing::error!("TUI error: {}", e);
            return Err(anyhow::anyhow!("TUI error: {}", e));
        }

        tracing::info!("TUI exited normally");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_tilde() {
        let result = expand_tilde("/absolute/path");
        assert_eq!(result, PathBuf::from("/absolute/path"));

        // Test with home expansion (if HOME is set)
        if std::env::var("HOME").is_ok() {
            let result = expand_tilde("~/test");
            assert!(result.to_string_lossy().contains("test"));
            assert!(!result.to_string_lossy().starts_with("~"));
        }
    }
}
