# AGENTS.md - Coding Agent Guidelines for mycrypto

This document provides guidelines for AI coding agents working on the mycrypto codebase.

## Project Overview

mycrypto is an AI-powered crypto paper trading companion for the terminal, built in Rust.
It features a TUI (terminal user interface), real-time market data via WebSocket, LLM-powered
chat, and simulated paper trading.

## Build, Test, and Lint Commands

```bash
# Build the project
cargo build

# Build release version (optimized)
cargo build --release

# Run all tests
cargo test

# Run a single test by name (substring match)
cargo test test_portfolio_open_close

# Run tests in a specific module
cargo test state::portfolio::tests

# Run tests with output visible
cargo test -- --nocapture

# Check code without building
cargo check

# Format code (required before commits)
cargo fmt

# Check formatting without modifying
cargo fmt -- --check

# Run clippy lints (fix all warnings)
cargo clippy

# Run clippy with auto-fix
cargo clippy --fix --allow-dirty

# Run the application
cargo run

# Run with mock mode (no API calls)
cargo run -- --mock
```

## Code Style Guidelines

### Imports

Order imports in groups separated by blank lines:
1. `std` library imports
2. External crate imports (alphabetical)
3. `crate::` internal imports

```rust
use std::collections::HashMap;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;

use crate::config::Config;
use crate::error::{MycryptoError, Result};
```

### Type Definitions

- Use `rust_decimal::Decimal` for ALL monetary values - never `f64`
- Use `chrono::DateTime<Utc>` for timestamps
- Use `uuid::Uuid` for unique identifiers
- Derive standard traits: `Debug, Clone, Serialize, Deserialize`
- Use `#[serde(rename_all = "snake_case")]` for enum serialization

### Naming Conventions

- **Types/Structs/Enums**: `PascalCase` - `AppState`, `PositionSide`
- **Functions/Methods**: `snake_case` - `calculate_pnl`, `get_ticker`
- **Constants**: `SCREAMING_SNAKE_CASE` - `MAX_POSITIONS`
- **Modules**: `snake_case` - `app_state`, `market_data`
- **Fields ending in percentage**: suffix with `_pct` - `unrealized_pnl_pct`

### Error Handling

- Use `thiserror` for domain errors in `src/error.rs`
- Use `anyhow` ONLY in `main.rs` for top-level error handling
- Propagate errors with `?` operator
- Add context with structured error variants, not strings

```rust
// Good - structured error
return Err(MycryptoError::ConfigValidation(
    "agent.min_confidence must be 0-100".to_string(),
));

// Bad - stringly typed
return Err("invalid config".into());
```

### Result Type Alias

The codebase uses a `Result` type alias defined in `src/error.rs`:
```rust
pub type Result<T> = std::result::Result<T, MycryptoError>;
```

### TUI Theming

All colors must come from `Theme` in `src/tui/theme.rs`. Never hardcode RGB values.

## Key Domain Types to Know

| Type | Location | Purpose |
|------|----------|---------|
| `AppState` | `state/app_state.rs` | Central application state |
| `Portfolio` | `state/portfolio.rs` | Virtual trading account |
| `Position` | `state/portfolio.rs` | Open paper trade |
| `Signal` | `state/signal.rs` | Trading signal with confidence |
| `Ticker` | `state/market.rs` | Real-time price data |
| `Config` | `config/schema.rs` | Application configuration |
| `MycryptoError` | `error.rs` | Unified error type |
| `Theme` | `tui/theme.rs` | UI color/style definitions |

## Common Gotchas

1. **Ticker fields**: Use `price` not `last_price`, use `price_change_pct_24h` not `price_change_pct`
2. **Config paths**: Use `config.agent.min_confidence` and `config.agent.max_open_trades`
3. **Portfolio metrics**: Use `calculate_metrics()` method, not direct field access
4. **Position side**: Use `position.side == PositionSide::Long` not `position.is_long`
5. **LlmProvider**: Not `Copy`, requires `.clone()` when passing by value
6. **Signal reasoning**: Is `Vec<ReasonEntry>`, use `reasoning_summary()` for display
7. **CandleBuffer**: No `.iter()`, use `.candles.iter()` instead
8. **Operator precedence**: Use parens with casts: `(i as u16) < x` not `i as u16 < x`
9. **Lifetimes in TUI**: Return `Vec<Line<'static>>` requires owned strings (`.clone()`)

## File Organization

```
src/
├── main.rs          # Entry point, CLI args, logging setup
├── error.rs         # MycryptoError enum, Result type alias
├── auth/            # Authentication (GitHub OAuth)
├── chat/            # LLM chat engine and providers
├── config/          # Configuration loading and schema
├── data/            # Market data, indicators, WebSocket feed
├── engine/          # Trading engine, analysis, signals
├── paper/           # Paper trading execution
├── state/           # Domain types (Portfolio, Signal, etc.)
└── tui/             # Terminal UI (ratatui-based)
    ├── app.rs       # Main TUI application loop
    ├── command.rs   # Command parsing
    ├── pages.rs     # Page rendering functions
    ├── theme.rs     # Color/style definitions
    └── widgets/     # Custom widgets (autocomplete, etc.)
```

## Testing Conventions

- Place tests in a `tests` submodule at the bottom of each file
- Use `#[cfg(test)]` for test modules
- Name test functions descriptively: `test_portfolio_open_close`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_pnl_long() {
        // ...
    }
}
```

## Performance Notes

- Release builds use LTO and single codegen unit for optimization
- TUI renders at ~60fps - avoid expensive operations in render loop
- Market data arrives via WebSocket - buffer appropriately
