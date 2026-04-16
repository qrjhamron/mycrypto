# Data Pipeline Hardening Design

## Goal

Harden real-time and external-source data ingestion for stability, correctness, and signal quality without breaking existing state dispatch, engine scheduler, or TUI rendering.

## Scope

- Binance WS resilience and validation hardening
- Source aggregator resilience + strict validation + timeout concurrency
- New source integrations: CryptoPanic + NewsData (fail-soft)
- Data quality scoring in state + TUI status surface + scheduler guard
- Unit tests for outlier rejection, breaker transitions, kline dedupe, sentiment clamp

## Current Context

- `src/data/feed.rs` already reconnects with exponential backoff but has no jitter, no stale watchdog, no reconnect counter, and no overlap buffer.
- `src/data/sources/aggregator.rs` currently performs sequential source fetches, has no circuit breaker, and lacks strict per-source response validation wrappers and timeout isolation.
- `AppState` already exposes `source_health` and `engine_status`, so health/detail expansion should flow through existing `StateUpdate` channel dispatch.

## Architecture

### 1) WebSocket Reliability Layer (`src/data/feed.rs`)

Add a reliability state inside `MarketFeed`:

- `reconnect_attempt: u64`
- `last_message_at: Instant`
- `recent_prices: HashMap<String, VecDeque<Decimal>>` (max 10)
- `seen_klines: HashSet<(String, Timeframe, i64)>` with bounded cleanup
- `reconnect_buffer: VecDeque<BufferedUpdate>` with 500ms TTL window

Behavior:

- reconnect delay: `delay = min(60s, 1s * 2^attempt)` with jitter ±20%
- watchdog tick every ~1s; if `now - last_message_at > 30s`, force reconnect
- strict schema validation before decode:
  - verify top-level object has `stream` string + `data` object
  - verify event-specific required fields and primitive types
  - malformed frame -> drop + warn log
- kline dedupe key: `(pair, timeframe, open_time_ms)`
  - discard duplicates from reconnect overlap
- 500ms overlap buffering during reconnect:
  - collect incoming parsed updates into buffer
  - on successful reconnect flush chronologically
- outlier ticker filtering:
  - median(last 10 accepted prices) per pair
  - reject if `abs(new-median)/median > 0.05`
  - log warn with pair/price/median/deviation

### 2) Source Reliability Layer (`src/data/sources/aggregator.rs`)

Introduce source runtime state machine:

- `SourceRuntimeState { consecutive_failures, last_success_at, cooldown_until, probe_mode }`
- breaker transitions:
  - Healthy -> Degraded after 3 consecutive failures
  - Degraded => no polling until 5 min cooldown
  - after cooldown, one probe fetch
  - probe success -> Healthy reset
  - probe failure -> Dead with new cooldown

Mapping to existing status model:

- Healthy => `SourceStatusLevel::Ok` (or `Connected` for Binance WS helper)
- Degraded => `SourceStatusLevel::Warn`
- Dead => `SourceStatusLevel::Error`
- Missing key => `SourceStatusLevel::MissingConfig`

Each status detail string includes `state=... failures=... cooldown=...` and `last_ok` preserved.

### 3) Concurrent Fetch Execution + Validation

Run enabled sources concurrently each poll tick:

- each fetch wrapped in `tokio::time::timeout(10s, source_fetch(...))`
- collect all results, then merge sentiment/macro/news

Strict validation strategy:

- keep source fetchers responsible for typed decode
- add post-decode sanity checks in aggregator before merge:
  - finite numeric values (`is_finite`)
  - required fields non-empty for news (`title`, `published_at` reasonable)
  - range checks (`fear_greed` 0..100, sentiment -1..1)
- rejection logs always include source and reason

Sentiment hygiene:

- sanitize every sentiment input with helper:
  - reject NaN/Inf (returns None + warn)
  - clamp valid to [-1.0, 1.0]

News hygiene:

- dedupe by URL (canonicalized string)
- fallback dedupe key for missing URL: `source + normalized_title + minute_bucket`
- sort desc by `published_at`
- retain only last 6 hours for active feed

### 4) New Source Modules

Add:

- `src/data/sources/cryptopanic.rs`
- `src/data/sources/newsdata.rs`

Both support fail-soft auth:

- missing env key -> `Ok(None)` in fetch API
- aggregator marks `MissingConfig` in source_health

Both return `Vec<NewsHeadline>` and optional aggregate sentiment if available.

### 5) State + Engine + TUI Integration

State additions:

- `AppState.data_quality: HashMap<String, f32>`
- `StateUpdate::DataQualityUpdated { pair, score }`

Engine status additions:

- add reconnect counter in `EngineStatus` (`ws_reconnect_count: u64`)
- update via existing `EngineStatusUpdated` state update path

Data quality score computation (per pair, 0..1):

- `ws_component` (freshness/uptime signal)
- `price_confirmation_component` (healthy corroborating market/macro/news proxies)
- `sentiment_agreement_component` (1 - normalized stddev of sentiment sources)
- weighted blend; clamp [0,1]

Scheduler guard:

- in `src/engine/scheduler.rs`, skip pair when quality < 0.5
- continue dispatching status/log updates, do not panic or block loop

TUI status (`src/tui/pages.rs`):

- display reconnect count
- display data-quality lines per active pair

## Error Handling

- No panics on malformed/partial upstream data
- All rejected frames/payloads logged with source and compact reason
- Channel send failures remain non-fatal where current behavior is best-effort

## Testing Plan

Add/extend unit tests:

1. outlier filter rejection (`feed.rs`)
2. source circuit breaker transitions (`aggregator.rs`)
3. kline deduplication (`feed.rs`)
4. sentiment clamp + NaN/Inf rejection (`aggregator.rs` helper)

Verification commands:

- `cargo fmt`
- `cargo check`
- `cargo test`

## Non-Goals

- Rewriting engine logic beyond quality guard hook
- Changing existing command/TUI navigation behavior
- Altering portfolio or trade execution semantics
