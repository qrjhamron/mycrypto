# mycrypto

![Rust 2021](https://img.shields.io/badge/rust-2021-orange)
![License: MIT](https://img.shields.io/badge/license-MIT-green)
![Build](https://img.shields.io/github/actions/workflow/status/mycrypto/mycrypto/ci.yml?branch=main)

**An AI-native crypto paper-trading terminal that feels like running your own quant desk from the command line.**

[demo.gif]

> PAPER TRADING ONLY. mycrypto is for simulation, strategy development, and education. It does not place real-money orders.

## Feature Matrix

| Feature | Status |
|---|---|
| Real-time Binance WebSocket feed | ✅ |
| Multi-source news aggregation | ✅ |
| AI Agent Team discussion | ✅ |
| Signal engine with 7 indicators | ✅ |
| Paper trading with full PnL accounting | ✅ |
| Multi-provider LLM support | ✅ |
| Interactive ratatui TUI | ✅ |

## Architecture

```text
+-------------------+      +-----------+      +----------+      +--------+      +---------+      +------+
| Binance WebSocket | ---> | Feed Task | ---> | AppState | ---> | Engine | ---> | Signals | ---> | TUI  |
+-------------------+      +-----------+      +----------+      +--------+      +---------+      +------+

+----------------+      +-------------+      +--------+      +-----------+
| LLM Providers  | ---> | Chat Engine | ---> | Intent | ---> | Portfolio |
+----------------+      +-------------+      +--------+      +-----------+
```

## Quick Start

```bash
git clone https://github.com/mycrypto/mycrypto.git
cd mycrypto
cargo build --release
cp config.example.toml config.toml
cargo run --release -- --mock
```

## Configuration

### `[engine]`

| Key | Type | Default | Description |
|---|---|---:|---|
| `enabled` | `bool` | `true` | Enables or disables the production signal scheduler. |
| `tick_interval_secs` | `u64` | `30` | Scheduler tick cadence in seconds. |
| `min_confidence` | `f32` | `0.65` | Minimum confluence confidence required to emit a signal. |
| `timeframe` | `String` | `"5m"` | Technical-analysis timeframe label. |
| `total_exposure_limit_pct` | `Decimal` | `80.0` | Maximum combined portfolio exposure allowed. |
| `correlation_threshold` | `f32` | `0.8` | Blocks highly correlated same-direction exposure above this level. |
| `pair_correlation` | `HashMap<String, HashMap<String, f32>>` | `{}` | Optional pair-correlation override matrix (`0.0..=1.0`). |
| `weights.ema_crossover` | `f32` | `1.5` | Weight for EMA crossover signal component. |
| `weights.rsi` | `f32` | `1.2` | Weight for RSI signal component. |
| `weights.macd` | `f32` | `1.3` | Weight for MACD signal component. |
| `weights.bb` | `f32` | `1.0` | Weight for Bollinger-band component. |
| `weights.atr_regime` | `f32` | `0.8` | Weight for ATR volatility regime component. |
| `weights.vwap` | `f32` | `1.0` | Weight for VWAP deviation component. |
| `weights.volume_anomaly` | `f32` | `1.1` | Weight for volume anomaly component. |
| `weights.sentiment` | `f32` | `1.4` | Weight for sentiment momentum component. |

### `[risk]`

| Key | Type | Default | Description |
|---|---|---:|---|
| `risk_per_trade_pct` | `Decimal` | `1.5` | Percent of portfolio risk budget per trade. |
| `max_position_pct` | `Decimal` | `20.0` | Maximum capital allocation per single position. |
| `max_daily_drawdown_pct` | `Decimal` | `5.0` | Daily drawdown guardrail before auto-protection behavior. |
| `max_drawdown_pct` | `Decimal` | `5.0` | Global drawdown ceiling used by risk checks. |
| `min_risk_reward` | `Decimal` | `2.0` | Minimum required risk/reward ratio for entries. |
| `trailing_stop_enabled` | `bool` | `false` | Enables trailing stop logic for open positions. |
| `trailing_stop_pct` | `Decimal` | `1.0` | Trailing stop distance as a percentage of price. |
| `funding_rate_threshold` | `Decimal` | `0.05` | Funding-rate threshold used to avoid crowded/perp-risk entries. |

### `[agent_team]`

| Key | Type | Default | Description |
|---|---|---:|---|
| `(none yet)` | `-` | `-` | Team orchestration is currently driven by internal defaults (fixed role set, timeout, and debate flow) rather than user-facing config keys. |

### `[data]`

| Key | Type | Default | Description |
|---|---|---:|---|
| `binance_ws_url` | `String` | `"wss://stream.binance.com:9443/stream"` | Binance stream endpoint for ticker/kline updates. |
| `cache_candles` | `usize` | `200` | Candle depth cached per pair/timeframe. |
| `yahoo_enabled` | `bool` | `true` | Enables Yahoo macro source polling. |
| `coingecko_enabled` | `bool` | `true` | Enables CoinGecko global metrics polling. |
| `fear_greed_enabled` | `bool` | `true` | Enables Fear & Greed index polling. |
| `reddit_enabled` | `bool` | `true` | Enables Reddit sentiment polling. |
| `twitter_enabled` | `bool` | `true` | Enables X/Twitter sentiment polling. |
| `reuters_rss_enabled` | `bool` | `true` | Enables Reuters RSS headlines. |
| `bloomberg_rss_enabled` | `bool` | `true` | Enables Bloomberg RSS headlines. |
| `finnhub_enabled` | `bool` | `true` | Enables Finnhub news + macro calendar polling. |
| `cryptopanic_enabled` | `bool` | `true` | Enables CryptoPanic headlines. |
| `newsdata_enabled` | `bool` | `true` | Enables NewsData headlines. |
| `yahoo_ttl_minutes` | `u64` | `15` | Yahoo cache TTL in minutes. |
| `coingecko_ttl_minutes` | `u64` | `5` | CoinGecko cache TTL in minutes. |
| `fear_greed_ttl_minutes` | `u64` | `10` | Fear & Greed cache TTL in minutes. |
| `reddit_ttl_minutes` | `u64` | `10` | Reddit cache TTL in minutes. |
| `twitter_ttl_minutes` | `u64` | `5` | X/Twitter cache TTL in minutes. |
| `rss_ttl_minutes` | `u64` | `15` | RSS cache TTL in minutes. |
| `finnhub_ttl_minutes` | `u64` | `15` | Finnhub cache TTL in minutes. |
| `cryptopanic_ttl_minutes` | `u64` | `10` | CryptoPanic cache TTL in minutes. |
| `newsdata_ttl_minutes` | `u64` | `10` | NewsData cache TTL in minutes. |
| `sources_poll_interval_sec` | `u64` | `60` | Global poll interval for non-WebSocket sources. |

## LLM Providers

| Provider | Required env vars | Setup |
|---|---|---|
| Claude | `CLAUDE_API_KEY` | Set key, choose `provider = "claude"`, set `model` (for example `claude-opus-4-5`). |
| OpenAI | `OPENAI_API_KEY` | Set key, choose `provider = "openai"`, choose OpenAI model. |
| Gemini | `GEMINI_API_KEY` (preferred) or `GOOGLE_API_KEY` | Set one key, choose `provider = "gemini"`, choose Gemini model. |
| OpenRouter | `OPENROUTER_API_KEY` | Set key, choose `provider = "openrouter"`, pick OpenRouter model ID. |
| Gradio | `GRADIO_SPACE_URL` (optional), `GRADIO_API_KEY` (optional/private) | Choose `provider = "gradio"`; defaults to a public space when no private creds are set. |
| Copilot | `GITHUB_TOKEN` | Set token or authenticate in-app with `/auth github`, then choose `provider = "copilot"`. |
| Mock | None | Choose `provider = "mock"` or run with `--mock` for offline/dev usage. |

## Slash Commands

| Command | Description | Example |
|---|---|---|
| `/portfolio` | Open portfolio overview page | `/portfolio` |
| `/signals` | Open latest signal board | `/signals` |
| `/chart [pair] [tf]` | Open chart, optionally with pair/timeframe | `/chart ETHUSDT 1h` |
| `/history [n]` | Show latest closed trades | `/history 25` |
| `/stats` | Show portfolio performance metrics | `/stats` |
| `/customize` | Open interactive config editor | `/customize` |
| `/buy <pair> <size>` | Open paper position | `/buy BTCUSDT 0.1` |
| `/close <pair>` | Close open position by pair | `/close BTCUSDT` |
| `/add <pair>` | Add pair to watchlist | `/add SOLUSDT` |
| `/remove <pair>` | Remove pair from watchlist | `/remove SOLUSDT` |
| `/risk <pct>` | Set risk per trade percentage | `/risk 1.5` |
| `/confidence <0-100>` | Set minimum signal confidence | `/confidence 72` |
| `/team <prompt>` | Run AI Agent Team discussion | `/team Analyze BTC trend vs macro risk` |
| `/team status` | Open team discussion page | `/team status` |
| `/team history` | Show recent team sessions | `/team history` |
| `/model` | Open model/provider selector | `/model` |
| `/auth` | Open auth workflow page | `/auth` |
| `/auth github` | Start GitHub device flow auth | `/auth github` |
| `/auth-delete [provider]` | Delete stored auth material | `/auth-delete openai` |
| `/news` | Open news page | `/news` |
| `/sentiment` | Open sentiment diagnostics | `/sentiment` |
| `/macro` | Open macro snapshot page | `/macro` |
| `/status` | Open system/source health dashboard | `/status` |
| `/heatmap` | Open market heatmap | `/heatmap` |
| `/log` | Show activity log page | `/log` |
| `/pairs` | Show watchlist/pairs dashboard | `/pairs` |
| `/clear` | Clear page/chat context for the current view | `/clear` |
| `/help` | Open in-app command reference | `/help` |
| `/exit` | Graceful shutdown | `/exit` |

## Development

```bash
cargo check
cargo clippy -- -D warnings
cargo test
cargo run -- --mock
```

## Contributing

Contributions are welcome. If you want to add features, improve strategy quality, or harden reliability:

1. Fork the repository.
2. Create a focused feature branch.
3. Run `cargo fmt`, `cargo clippy -- -D warnings`, and `cargo test`.
4. Open a PR with a clear summary and verification notes.

## License

MIT
# mycrypto
