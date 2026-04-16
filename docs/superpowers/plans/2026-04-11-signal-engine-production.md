# Production Signal Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the placeholder signal path with a production-grade engine pipeline (`technical -> sentiment -> confluence -> risk -> scheduler`) that emits `StateUpdate` updates and drives signal generation safely.

**Architecture:** Introduce dedicated `src/engine/*` modules with typed outputs per stage, then wire a background scheduler that consumes `AppState` snapshots and emits `NewSignal` + `EngineStatusUpdated` through the existing channel dispatch. Keep all monetary calculations in `Decimal`, use `f64` only for intermediate indicator statistics, and preserve existing TUI/feed/chat flows.

**Tech Stack:** Rust 2021, Tokio (`interval`, `mpsc`, `watch`), `rust_decimal`, `chrono`, existing `state::Signal` builder and `data::indicators` helpers.

---

## File Structure and Responsibilities

- Create: `src/engine/technical.rs` - technical indicators and `TechnicalSignal` typing.
- Create: `src/engine/sentiment.rs` - sentiment momentum filter and `SentimentSignal` typing.
- Create: `src/engine/confluence.rs` - weighted voting merge into `ConfluenceSignal`.
- Create: `src/engine/risk.rs` - exposure/drawdown/correlation/Kelly checks and `RiskAssessment`.
- Create: `src/engine/signal_engine.rs` - stage orchestration and conversion to `state::Signal`.
- Create: `src/engine/scheduler.rs` - tokio interval loop, circuit breaker, status/log updates.
- Modify: `src/engine/mod.rs` - module exports and public `EngineStatus` type.
- Modify: `src/main.rs` - switch to real `mod engine;` module wiring.
- Modify: `src/config/schema.rs` - add `[engine]` and `[engine.weights]` config schema + validation.
- Modify: `src/config/mod.rs` - export engine config types.
- Modify: `src/state/app_state.rs` - add `engine_status` field and `StateUpdate::EngineStatusUpdated` handling.
- Modify: `src/state/mod.rs` - re-export engine status and new state update typing as needed.
- Modify: `src/tui/app.rs` - spawn scheduler and send `AppState` snapshots through watch channel.
- Modify: `src/tui/pages.rs` - show engine status and interval from engine config.

### Task 1: Wire Engine Config and Module Root

**Files:**
- Modify: `src/main.rs`
- Modify: `src/config/schema.rs`
- Modify: `src/config/mod.rs`
- Modify: `src/engine/mod.rs`
- Test: `src/config/schema.rs`

- [ ] **Step 1: Write the failing config test**

```rust
#[test]
fn test_default_engine_config_is_valid() {
    let config = Config::default();
    assert!(config.engine.tick_interval_secs >= 5);
    assert!(config.engine.min_confidence >= 0.0 && config.engine.min_confidence <= 1.0);
    assert!(config.validate().is_ok());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test test_default_engine_config_is_valid`
Expected: FAIL with missing `engine` field/types.

- [ ] **Step 3: Add engine schema and root module wiring**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    #[serde(default = "default_engine_enabled")]
    pub enabled: bool,
    #[serde(default = "default_engine_tick_interval_secs")]
    pub tick_interval_secs: u64,
    #[serde(default = "default_engine_min_confidence")]
    pub min_confidence: f32,
    #[serde(default = "default_engine_timeframe")]
    pub timeframe: String,
    #[serde(default)]
    pub weights: EngineWeights,
}
```

- [ ] **Step 4: Re-run config tests**

Run: `cargo test test_default_engine_config_is_valid test_default_config_is_valid`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/main.rs src/config/schema.rs src/config/mod.rs src/engine/mod.rs
git commit -m "feat: add engine config schema and real engine module wiring"
```

### Task 2: Add Engine Status to StateUpdate/AppState

**Files:**
- Modify: `src/state/app_state.rs`
- Modify: `src/state/mod.rs`
- Test: `src/state/app_state.rs`

- [ ] **Step 1: Write failing state update test**

```rust
#[test]
fn test_engine_status_update_applies_to_state() {
    let mut state = AppState::new(Config::default());
    let status = crate::engine::EngineStatus::default();
    state.apply_update(StateUpdate::EngineStatusUpdated(status.clone()));
    assert_eq!(state.engine_status.circuit_breaker_open, status.circuit_breaker_open);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test test_engine_status_update_applies_to_state`
Expected: FAIL with missing field/enum variant.

- [ ] **Step 3: Add status field and enum variant**

```rust
pub struct AppState {
    // ...
    pub engine_status: crate::engine::EngineStatus,
}

pub enum StateUpdate {
    // ...
    EngineStatusUpdated(crate::engine::EngineStatus),
}
```

- [ ] **Step 4: Re-run test**

Run: `cargo test test_engine_status_update_applies_to_state`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/state/app_state.rs src/state/mod.rs
git commit -m "feat: track engine runtime status in app state"
```

### Task 3: Implement Technical Module With EMA/RSI/MACD Tests First

**Files:**
- Create: `src/engine/technical.rs`
- Test: `src/engine/technical.rs`

- [ ] **Step 1: Write failing unit tests (EMA crossover, RSI signal, MACD momentum)**

```rust
#[test]
fn test_ema_crossover_long_signal() { /* synthetic uptrend candles */ }

#[test]
fn test_rsi_direction_signal() { /* oversold/overbought assertions */ }

#[test]
fn test_macd_histogram_momentum_signal() { /* histogram slope assertions */ }
```

- [ ] **Step 2: Run targeted tests and verify fail**

Run: `cargo test test_ema_crossover_long_signal test_rsi_direction_signal test_macd_histogram_momentum_signal`
Expected: FAIL until technical types/functions are added.

- [ ] **Step 3: Implement technical signal types and indicator evaluators**

```rust
pub enum TechnicalIndicatorKind { EmaCrossover, RsiDivergence, MacdMomentum, BollingerBreakout, AtrRegime, VwapDeviation, VolumeAnomaly }

pub struct TechnicalSignal {
    pub pair: String,
    pub direction: SignalDirection,
    pub strength: f32,
    pub contributors: Vec<TechnicalIndicatorKind>,
    pub details: Vec<IndicatorVote>,
}
```

- [ ] **Step 4: Re-run targeted technical tests**

Run: `cargo test test_ema_crossover_long_signal test_rsi_direction_signal test_macd_histogram_momentum_signal`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/engine/technical.rs
git commit -m "feat: add technical signal evaluator with ema rsi macd coverage"
```

### Task 4: Finish Technical Coverage (BB/ATR/VWAP/Volume Z-Score)

**Files:**
- Modify: `src/engine/technical.rs`
- Test: `src/engine/technical.rs`

- [ ] **Step 1: Write failing tests for BB squeeze-breakout and volume anomaly z-score**

```rust
#[test]
fn test_bollinger_squeeze_breakout_signal() { /* narrow bands then breakout */ }

#[test]
fn test_volume_anomaly_zscore_signal() { /* latest volume spike => anomaly */ }
```

- [ ] **Step 2: Run tests to verify fail**

Run: `cargo test test_bollinger_squeeze_breakout_signal test_volume_anomaly_zscore_signal`
Expected: FAIL.

- [ ] **Step 3: Implement BB, ATR regime, VWAP deviation, and z-score logic**

```rust
fn volume_zscore(candles: &[OHLCV], window: usize) -> Option<f64> { /* mean/stddev */ }
fn evaluate_bollinger(...) -> IndicatorVote { /* squeeze + breakout */ }
fn evaluate_atr_regime(...) -> IndicatorVote { /* low/normal/high */ }
```

- [ ] **Step 4: Run tests**

Run: `cargo test test_bollinger_squeeze_breakout_signal test_volume_anomaly_zscore_signal`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/engine/technical.rs
git commit -m "feat: add bollinger atr vwap and volume anomaly technical votes"
```

### Task 5: Implement Sentiment Module With 3-Tick Momentum

**Files:**
- Create: `src/engine/sentiment.rs`
- Test: `src/engine/sentiment.rs`

- [ ] **Step 1: Write failing sentiment momentum test**

```rust
#[test]
fn test_sentiment_delta_over_3_ticks() {
    // push 4 values and assert delta = current - value_3_ticks_ago
}
```

- [ ] **Step 2: Run test to verify fail**

Run: `cargo test test_sentiment_delta_over_3_ticks`
Expected: FAIL.

- [ ] **Step 3: Implement sentiment signal types and tracker**

```rust
pub struct SentimentSignal { pub pair: String, pub score: f32, pub delta_3tick: f32, pub direction: SignalDirection }
pub struct SentimentTracker { history: VecDeque<f32> }
```

- [ ] **Step 4: Re-run test**

Run: `cargo test test_sentiment_delta_over_3_ticks`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/engine/sentiment.rs
git commit -m "feat: add sentiment signal with 3-tick momentum filter"
```

### Task 6: Implement Confluence Weighted Voting

**Files:**
- Create: `src/engine/confluence.rs`
- Test: `src/engine/confluence.rs`

- [ ] **Step 1: Write failing weighting test**

```rust
#[test]
fn test_confluence_weighted_vote_prefers_higher_weight_direction() {
    // bearish low-weight vs bullish high-weight should resolve bullish
}
```

- [ ] **Step 2: Run test to verify fail**

Run: `cargo test test_confluence_weighted_vote_prefers_higher_weight_direction`
Expected: FAIL.

- [ ] **Step 3: Implement confluence scoring and threshold gating**

```rust
pub struct ConfluenceSignal {
    pub pair: String,
    pub direction: SignalDirection,
    pub composite_score: f32,
    pub agreed: Vec<String>,
    pub disagreed: Vec<String>,
    pub actionable: bool,
}
```

- [ ] **Step 4: Run test**

Run: `cargo test test_confluence_weighted_vote_prefers_higher_weight_direction`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/engine/confluence.rs
git commit -m "feat: add configurable confluence weighted voting"
```

### Task 7: Implement Risk Module (Exposure + Correlation + Kelly)

**Files:**
- Create: `src/engine/risk.rs`
- Test: `src/engine/risk.rs`

- [ ] **Step 1: Write failing Kelly sizing test**

```rust
#[test]
fn test_kelly_sizing_returns_positive_fraction_for_profitable_history() {
    // win rate + payoff ratio should produce positive capped kelly
}
```

- [ ] **Step 2: Run test to verify fail**

Run: `cargo test test_kelly_sizing_returns_positive_fraction_for_profitable_history`
Expected: FAIL.

- [ ] **Step 3: Implement risk checks and assessment struct**

```rust
pub struct RiskAssessment {
    pub approved: bool,
    pub suggested_size: Decimal,
    pub rejection_reason: Option<String>,
    pub kelly_fraction: Decimal,
}
```

- [ ] **Step 4: Run risk tests**

Run: `cargo test test_kelly_sizing_returns_positive_fraction_for_profitable_history`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/engine/risk.rs
git commit -m "feat: add risk gate with drawdown exposure correlation and kelly sizing"
```

### Task 8: Implement Signal Engine Orchestrator

**Files:**
- Create: `src/engine/signal_engine.rs`
- Modify: `src/engine/mod.rs`
- Test: `src/engine/signal_engine.rs`

- [ ] **Step 1: Write failing integration test for pipeline output**

```rust
#[test]
fn test_pipeline_builds_state_signal_from_stage_outputs() {
    // confluence+risk => SignalAction::Execute/Skip/Watch mapping
}
```

- [ ] **Step 2: Run test to verify fail**

Run: `cargo test test_pipeline_builds_state_signal_from_stage_outputs`
Expected: FAIL.

- [ ] **Step 3: Implement pipeline and signal conversion**

```rust
pub async fn run_pipeline_for_pair(snapshot: &AppState, pair: &str, tracker: &mut SentimentTracker) -> Result<Option<Signal>> {
    // technical + sentiment -> confluence -> risk -> Signal builder
}
```

- [ ] **Step 4: Re-run test**

Run: `cargo test test_pipeline_builds_state_signal_from_stage_outputs`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/engine/signal_engine.rs src/engine/mod.rs
git commit -m "feat: add signal engine orchestrator and signal conversion"
```

### Task 9: Implement Scheduler and Circuit Breaker

**Files:**
- Create: `src/engine/scheduler.rs`
- Test: `src/engine/scheduler.rs`

- [ ] **Step 1: Write failing circuit-breaker test**

```rust
#[test]
fn test_circuit_breaker_opens_after_more_than_three_errors() {
    // simulate 4 failures and assert breaker open
}
```

- [ ] **Step 2: Run test to verify fail**

Run: `cargo test test_circuit_breaker_opens_after_more_than_three_errors`
Expected: FAIL.

- [ ] **Step 3: Implement tokio interval scheduler**

```rust
pub fn spawn_signal_scheduler_on(
    handle: &tokio::runtime::Handle,
    snapshot_rx: tokio::sync::watch::Receiver<AppState>,
    state_tx: tokio::sync::mpsc::Sender<StateUpdate>,
) -> tokio::task::JoinHandle<()> {
    // interval loop + pipeline + breaker + status/log updates
}
```

- [ ] **Step 4: Run scheduler test**

Run: `cargo test test_circuit_breaker_opens_after_more_than_three_errors`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/engine/scheduler.rs
git commit -m "feat: add signal scheduler with circuit breaker"
```

### Task 10: Wire Scheduler Into TUI App and Update Status Rendering

**Files:**
- Modify: `src/tui/app.rs`
- Modify: `src/tui/pages.rs`
- Modify: `src/state/app_state.rs`
- Test: `src/state/app_state.rs`, `src/tui/pages.rs`

- [ ] **Step 1: Write failing status rendering test**

```rust
#[test]
fn test_status_page_shows_engine_breaker_state() {
    // construct state with breaker open and assert rendered lines include marker
}
```

- [ ] **Step 2: Run test to verify fail**

Run: `cargo test test_status_page_shows_engine_breaker_state`
Expected: FAIL.

- [ ] **Step 3: Add watch snapshot channel + scheduler spawn + UI output**

```rust
let (engine_snapshot_tx, engine_snapshot_rx) = tokio::sync::watch::channel(state.clone());
std::mem::drop(spawn_signal_scheduler_on(&runtime_handle, engine_snapshot_rx, update_tx.clone()));
```

- [ ] **Step 4: Re-run test**

Run: `cargo test test_status_page_shows_engine_breaker_state`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/tui/app.rs src/tui/pages.rs src/state/app_state.rs
git commit -m "feat: wire scheduler runtime and show engine status in tui"
```

### Task 11: Full Verification and Regression Safety

**Files:**
- Modify (if needed): any failing file from formatting/lints/tests.

- [ ] **Step 1: Run formatter**

Run: `cargo fmt`
Expected: no errors.

- [ ] **Step 2: Run compile check**

Run: `cargo check`
Expected: `Finished dev profile` with no errors.

- [ ] **Step 3: Run targeted new tests**

Run:
`cargo test test_ema_crossover_long_signal test_rsi_direction_signal test_macd_histogram_momentum_signal test_confluence_weighted_vote_prefers_higher_weight_direction test_kelly_sizing_returns_positive_fraction_for_profitable_history`

Expected: PASS.

- [ ] **Step 4: Run full test suite**

Run: `cargo test`
Expected: PASS.

- [ ] **Step 5: Final commit**

```bash
git add src/engine src/config src/state src/tui src/main.rs
git commit -m "feat: implement production signal engine pipeline with scheduler and risk gating"
```

## Self-Review

- Spec coverage check: this plan maps each required module (`technical`, `sentiment`, `confluence`, `risk`, `scheduler`), config weights/min threshold, drawdown/correlation/Kelly risk checks, scheduler circuit breaker, `EngineStatus` exposure, and required tests.
- Placeholder scan: no TODO/TBD placeholders remain; each task includes concrete files, commands, and code snippets.
- Type consistency: all tasks reference `SignalDirection`, `StateUpdate`, `AppState`, and `Decimal` consistently across modules.
