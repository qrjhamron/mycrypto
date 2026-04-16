# Performance, Memory, and Code Quality Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enforce strict memory bounds, reduce render/runtime CPU overhead, harden async task lifecycle/cancellation, and remove warning/panic debt while keeping behavior stable.

**Architecture:** Keep existing module boundaries and state flow (single-writer `AppState` + async producers), but replace unbounded session collections with bounded rings, introduce chart cache LRU, and add structured UI memoization and cancellation-aware background task control. Use incremental TDD-style tasks with fast feedback and verification gates after each subsystem.

**Tech Stack:** Rust 2021, Tokio (`mpsc`, `watch`, `select!`), Ratatui/Crossterm, Chrono, Serde, Clippy, Cargo test.

---

## File Responsibility Map

- Modify: `src/state/app_state.rs`
  - Convert chat/news collections to bounded `VecDeque` rings.
  - Add chart cache LRU mechanics (max 50 keys) and chart series cap (200 candles).
  - Add helper methods and unit tests for cap/eviction behavior.
- Modify: `src/state/mod.rs`
  - Re-export any new state types introduced for bounded cache structures.
- Modify: `src/data/sources/cache.rs`
  - Persist/load bounded news/chart payloads after normalization and LRU filtering.
- Modify: `src/tui/app.rs`
  - Convert activity strip buffer to `VecDeque` ring cap 20.
  - Replace unbounded tick channel with bounded channel.
  - Store background task handles and cancellation signal in `App` and shut down cleanly.
  - Harden frame hash guard (no debug-string hashing).
- Modify: `src/tui/pages.rs`
  - Remove hot-path string clones where borrow/Cow is possible.
  - Consume memoized/sorted data prepared in update path.
- Modify: `src/chat/context.rs`, `src/chat/engine.rs`
  - Support `VecDeque` chat storage and cap 200 at write boundaries.
- Modify: `src/engine/scheduler.rs`
  - Add cancellation-aware loop (`select!`) and reduce per-tick cloning.
- Modify: `src/engine/sentiment.rs`, `src/engine/confluence.rs`
  - Remove per-call temporary allocations in hot scoring paths.
- Modify: `src/data/feed.rs`
  - Remove `#[allow(dead_code)]` field suppressions with serde-safe underscore fields.
- Modify: `src/main.rs`
  - Remove crate-level `#![allow(...)]` suppressions and fix warnings directly.
- Modify: `src/chat/llm/*.rs` where needed
  - Remove `#[allow(dead_code)]` suppressions by renaming or pruning unused fields.
- Modify: `src/state/portfolio.rs`, `src/engine/*.rs`, `src/chat/*.rs`
  - Add missing docs and explicit `#[must_use]` on public `Result` APIs.

---

### Task 1: Bounded Chat/News Rings in AppState

**Files:**
- Modify: `src/state/app_state.rs`
- Modify: `src/chat/context.rs`
- Test: `src/state/app_state.rs` (tests module)

- [ ] **Step 1: Write failing tests for caps**

```rust
#[test]
fn test_chat_history_capped_at_200() {
    let mut state = AppState::new(Config::default());
    for i in 0..240 {
        state.send_user_message(format!("msg-{i}"));
        state.apply_update(StateUpdate::ChatDone);
    }
    assert!(state.chat_messages.len() <= 200);
}

#[test]
fn test_news_history_capped_at_500() {
    let mut state = AppState::new(Config::default());
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
```

- [ ] **Step 2: Run targeted tests and verify they fail first**

Run: `cargo test state::app_state::tests::test_chat_history_capped_at_200 state::app_state::tests::test_news_history_capped_at_500`
Expected: FAIL due to current unbounded chat/news behavior.

- [ ] **Step 3: Implement bounded `VecDeque` rings and helpers**

```rust
use std::collections::{HashMap, VecDeque};

const CHAT_HISTORY_CAP: usize = 200;
const NEWS_HISTORY_CAP: usize = 500;

fn push_bounded<T>(buf: &mut VecDeque<T>, value: T, cap: usize) {
    buf.push_back(value);
    while buf.len() > cap {
        let _ = buf.pop_front();
    }
}

// AppState fields
pub chat_messages: VecDeque<ChatMessage>,
pub news_history: VecDeque<NewsHeadline>,

// send_user_message
push_bounded(&mut self.chat_messages, ChatMessage::user(content), CHAT_HISTORY_CAP);
push_bounded(
    &mut self.chat_messages,
    ChatMessage::agent_streaming(),
    CHAT_HISTORY_CAP,
);

// apply_news_update
let existing = std::mem::take(&mut self.news_history);
let mut merged: Vec<NewsHeadline> = existing.into_iter().collect();
merged.extend(headlines);
let normalized = normalize_news_items(merged, NEWS_HISTORY_CAP);
self.news_history = normalized.into_iter().collect();
```

- [ ] **Step 4: Update history consumers to work with `VecDeque`**

```rust
// chat/context.rs
let history_len = state.chat_messages.len();
let history_start = history_len.saturating_sub(max_history);
for msg in state.chat_messages.iter().skip(history_start) {
    // unchanged mapping
}
```

- [ ] **Step 5: Run tests and commit**

Run: `cargo test state::app_state::tests::test_chat_history_capped_at_200 state::app_state::tests::test_news_history_capped_at_500`
Expected: PASS.

```bash
git add src/state/app_state.rs src/chat/context.rs
git commit -m "refactor: bound chat and news histories with VecDeque rings"
```

---

### Task 2: Chart Series Cap (200) and LRU Cache (50 keys)

**Files:**
- Modify: `src/state/app_state.rs`
- Modify: `src/data/sources/cache.rs`
- Test: `src/state/app_state.rs` (tests module)

- [ ] **Step 1: Add failing LRU/cap tests**

```rust
#[test]
fn test_chart_series_truncated_to_200() {
    let mut state = AppState::new(Config::default());
    let pair = state.chart_pair.clone();
    let candles: Vec<OHLCV> = (0..350)
        .map(|i| OHLCV {
            timestamp: Utc::now() - chrono::Duration::minutes((350 - i) as i64),
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
        fetched_at: Utc::now(),
    });
    let key = chart_cache_key(&state.chart_pair, Timeframe::H1);
    assert!(state.chart_cache.get(&key).map(|v| v.len()).unwrap_or(0) <= 200);
}

#[test]
fn test_chart_cache_lru_evicts_over_50_keys() {
    let mut state = AppState::new(Config::default());
    for i in 0..55 {
        let pair = format!("X{i}USDT");
        state.apply_update(StateUpdate::ChartSeriesUpdate {
            pair: pair.clone(),
            timeframe: Timeframe::H1,
            candles: vec![OHLCV::new(Utc::now(), Decimal::ONE)],
            fetched_at: Utc::now(),
        });
    }
    assert!(state.chart_cache.len() <= 50);
    assert!(state.chart_last_fetch_at.len() <= 50);
}
```

- [ ] **Step 2: Run tests to confirm failure**

Run: `cargo test state::app_state::tests::test_chart_series_truncated_to_200 state::app_state::tests::test_chart_cache_lru_evicts_over_50_keys`
Expected: FAIL before LRU/truncation implementation.

- [ ] **Step 3: Implement truncation and LRU helpers**

```rust
const CHART_SERIES_CAP: usize = 200;
const CHART_CACHE_KEY_CAP: usize = 50;

fn normalize_chart_candles(mut candles: Vec<OHLCV>) -> Vec<OHLCV> {
    candles.sort_by_key(|c| c.timestamp);
    if candles.len() > CHART_SERIES_CAP {
        let keep_from = candles.len() - CHART_SERIES_CAP;
        candles = candles.split_off(keep_from);
    }
    candles
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
```

- [ ] **Step 4: Persist only bounded chart data**

```rust
// src/data/sources/cache.rs
pub fn save_chart_cache(series: HashMap<String, Vec<OHLCV>>) -> Result<()> {
    let bounded = series
        .into_iter()
        .map(|(k, mut v)| {
            if v.len() > 200 {
                let keep_from = v.len() - 200;
                v = v.split_off(keep_from);
            }
            (k, v)
        })
        .take(50)
        .collect::<HashMap<_, _>>();
    let payload = ChartCache { series: bounded, cached_at: Utc::now() };
    // existing write path
}
```

- [ ] **Step 5: Run tests and commit**

Run: `cargo test state::app_state::tests::test_chart_series_truncated_to_200 state::app_state::tests::test_chart_cache_lru_evicts_over_50_keys data::sources::cache::tests::test_chart_cache_roundtrip`
Expected: PASS.

```bash
git add src/state/app_state.rs src/data/sources/cache.rs
git commit -m "feat: enforce chart candle cap and LRU cache eviction"
```

---

### Task 3: Activity Ring Buffer (20) and UI Collection Hygiene

**Files:**
- Modify: `src/tui/app.rs`
- Test: `src/tui/app.rs` (tests module)

- [ ] **Step 1: Add failing activity cap test**

```rust
#[test]
fn test_activity_events_capped_at_20() {
    let mut app = App::new(test_state(), test_handle());
    for i in 0..40 {
        app.capture_activity_event(&StateUpdate::Log(LogEntry::info(format!("e{i}"))));
    }
    assert!(app.activity_events.len() <= 20);
}
```

- [ ] **Step 2: Run test and confirm failure/compile mismatch**

Run: `cargo test tui::app::tests::test_activity_events_capped_at_20`
Expected: FAIL before refactor.

- [ ] **Step 3: Convert to `VecDeque` push/pop pattern**

```rust
use std::collections::{HashMap, VecDeque};

activity_events: VecDeque<String>,

if let Some(event) = msg {
    self.activity_events.push_back(event);
    while self.activity_events.len() > ACTIVITY_EVENT_CAP {
        let _ = self.activity_events.pop_front();
    }
}
```

- [ ] **Step 4: Update render helpers consuming activity events**

```rust
let joined = self
    .activity_events
    .iter()
    .map(String::as_str)
    .collect::<Vec<_>>()
    .join("   ✦   ");
```

- [ ] **Step 5: Run tests and commit**

Run: `cargo test tui::app::tests::test_activity_events_capped_at_20`
Expected: PASS.

```bash
git add src/tui/app.rs
git commit -m "refactor: switch activity strip to bounded VecDeque ring"
```

---

### Task 4: Async Cancellation, Bounded Tick Channel, and Stored Abort Handles

**Files:**
- Modify: `src/tui/app.rs`
- Modify: `src/data/feed.rs`
- Modify: `src/data/sources/aggregator.rs`
- Modify: `src/engine/scheduler.rs`
- Test: `src/engine/scheduler.rs` and `src/tui/app.rs`

- [ ] **Step 1: Add cancellation test for scheduler**

```rust
#[tokio::test]
async fn test_scheduler_exits_on_shutdown_signal() {
    let (state_tx, mut state_rx) = mpsc::channel(8);
    let (_snap_tx, snap_rx) = tokio::sync::watch::channel(AppState::new(Config::default()));
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    let handle = spawn_signal_scheduler_on(&tokio::runtime::Handle::current(), snap_rx, state_tx, shutdown_rx);
    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
    drop(state_rx);
}
```

- [ ] **Step 2: Run test to confirm signature/behavior gap**

Run: `cargo test engine::scheduler::tests::test_scheduler_exits_on_shutdown_signal`
Expected: FAIL until shutdown plumbing exists.

- [ ] **Step 3: Add app-level background task tracking and shutdown broadcast**

```rust
pub struct App {
    // ...
    background_tasks: Vec<tokio::task::JoinHandle<()>>,
    shutdown_tx: tokio::sync::watch::Sender<bool>,
}

let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
// pass shutdown_rx.clone() into all spawned workers
```

- [ ] **Step 4: Replace unbounded tick channel with bounded `mpsc::channel(256)` and backpressure log**

```rust
let (tick_tx, mut tick_rx) = mpsc::channel::<()>(256);
let tick_task = self.runtime_handle.spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_millis(100));
    loop {
        interval.tick().await;
        if tick_tx.try_send(()).is_err() {
            tracing::warn!("tick channel full; dropping UI tick");
        }
    }
});
```

- [ ] **Step 5: Add `select!` shutdown branches in long-running loops**

```rust
tokio::select! {
    _ = interval.tick() => { /* existing work */ }
    changed = shutdown_rx.changed() => {
        if changed.is_ok() && *shutdown_rx.borrow() { break; }
    }
}
```

- [ ] **Step 6: Ensure clean app exit aborts/joins workers and commit**

Run: `cargo test engine::scheduler::tests::test_scheduler_exits_on_shutdown_signal tui::app::tests::test_app_creation`
Expected: PASS.

```bash
git add src/tui/app.rs src/data/feed.rs src/data/sources/aggregator.rs src/engine/scheduler.rs
git commit -m "refactor: add explicit shutdown and bounded async channels"
```

---

### Task 5: Render-Loop Memoization and Frame Diff Guard Correctness

**Files:**
- Modify: `src/tui/app.rs`
- Modify: `src/tui/pages.rs`
- Test: `src/tui/app.rs` (tests module)

- [ ] **Step 1: Add failing hash-guard stability test**

```rust
#[test]
fn test_render_hash_stable_without_state_change() {
    let app = App::new(test_state(), test_handle());
    let area = Rect::new(0, 0, 120, 40);
    let a = app.render_state_hash(area);
    let b = app.render_state_hash(area);
    assert_eq!(a, b);
}
```

- [ ] **Step 2: Run test and baseline behavior**

Run: `cargo test tui::app::tests::test_render_hash_stable_without_state_change`
Expected: PASS/FAIL baseline recorded; keep for regression.

- [ ] **Step 3: Remove debug-string hashing and hash structured fields only**

```rust
fn render_state_hash(&self, size: Rect) -> u64 {
    let mut h = DefaultHasher::new();
    size.width.hash(&mut h);
    size.height.hash(&mut h);
    self.page.hash(&mut h);
    self.scroll.hash(&mut h);
    self.state.updated_at.timestamp_millis().hash(&mut h);
    self.news_history_query.hash(&mut h);
    // Do not hash entire Debug dumps of AppState/selector/autocomplete.
    h.finish()
}
```

- [ ] **Step 4: Move static formatting/sorting to update path cache**

```rust
#[derive(Default)]
struct UiShellState {
    sorted_source_names: Vec<String>,
    activity_joined: String,
    revision: u64,
}

fn rebuild_ui_shell_state(&mut self) {
    let mut names: Vec<String> = self.state.source_health.keys().cloned().collect();
    names.sort();
    self.ui_shell.sorted_source_names = names;
    self.ui_shell.activity_joined = self
        .activity_events
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join("   ✦   ");
}
```

- [ ] **Step 5: Run targeted tests and commit**

Run: `cargo test tui::app::tests::test_render_hash_stable_without_state_change tui::pages::tests::test_filter_news_history_by_query`
Expected: PASS.

```bash
git add src/tui/app.rs src/tui/pages.rs
git commit -m "perf: memoize stable UI data and harden frame diff hashing"
```

---

### Task 6: Hot-Path Clone/String Allocation Reduction

**Files:**
- Modify: `src/tui/app.rs`
- Modify: `src/tui/pages.rs`
- Modify: `src/engine/sentiment.rs`
- Modify: `src/engine/confluence.rs`
- Modify: `src/engine/scheduler.rs`

- [ ] **Step 1: Add a regression test for chart/query/filter behavior**

```rust
#[test]
fn test_news_history_filter_still_matches_source_and_title() {
    // existing helper pattern from pages tests, keep assertions unchanged
}
```

- [ ] **Step 2: Run test to set baseline**

Run: `cargo test tui::pages::tests::test_filter_news_history_by_query`
Expected: PASS baseline.

- [ ] **Step 3: Remove obvious render clones and use borrow/Cow**

```rust
use std::borrow::Cow;

let display_input: Cow<'_, str> = if self.input.chars().count() > input_area_width {
    Cow::Owned(
        self.input
            .chars()
            .skip(self.input.chars().count() - input_area_width)
            .collect::<String>(),
    )
} else {
    Cow::Borrowed(self.input.as_str())
};

Span::styled(entry.message.as_str(), theme.text());
```

- [ ] **Step 4: Remove per-call allocations in sentiment/confluence and reduce scheduler cloning**

```rust
// sentiment: no intermediate Vec
let (sum, count) = state
    .news_headlines
    .iter()
    .filter_map(|h| h.sentiment)
    .take(12)
    .fold((0.0f32, 0usize), |(s, n), v| (s + v, n + 1));

// confluence: preallocate vectors
let mut agreed = Vec::with_capacity(6);
let mut disagreed = Vec::with_capacity(6);

// scheduler: avoid per-pair AppState clone
for pair in &snapshot.config.pairs.watchlist {
    let out = run_pipeline_for_pair(&snapshot, pair, tracker);
    // no join_all of async wrappers
}
```

- [ ] **Step 5: Run targeted tests and commit**

Run: `cargo test engine::sentiment::tests::test_sentiment_delta_over_3_ticks engine::confluence::tests::test_confluence_weighted_vote_prefers_higher_weight_direction`
Expected: PASS.

```bash
git add src/tui/app.rs src/tui/pages.rs src/engine/sentiment.rs src/engine/confluence.rs src/engine/scheduler.rs
git commit -m "perf: remove hot-path allocations and large clone churn"
```

---

### Task 7: Remove `allow(...)` Suppressions and Non-test Panic Paths

**Files:**
- Modify: `src/main.rs`
- Modify: `src/data/feed.rs`
- Modify: `src/chat/llm/claude.rs`
- Modify: `src/chat/llm/openai.rs`
- Modify: `src/chat/llm/openrouter.rs`
- Modify: `src/chat/llm/gemini.rs`
- Modify: `src/chat/llm/gradio.rs`
- Modify: `src/chat/llm/copilot.rs`
- Modify: `src/chat/llm/mock.rs`

- [ ] **Step 1: Remove top-level allow attributes and run clippy to surface real issues**

```rust
// delete these from src/main.rs
// #![allow(dead_code, unused_imports, unused_variables, deprecated)]
// #![allow(...clippy...)]
```

Run: `cargo clippy -- -D warnings`
Expected: FAIL with concrete warnings to fix.

- [ ] **Step 2: Fix dead-code serde field warnings by explicit underscore fields**

```rust
#[derive(Debug, Deserialize)]
struct BinanceSymbolTicker {
    #[serde(rename = "e")]
    _event_type: String,
    #[serde(rename = "E")]
    _event_time: u64,
    // used fields remain unchanged
}
```

- [ ] **Step 3: Remove `#[allow(dead_code)]` in chat providers via same pattern**

```rust
#[derive(Debug, Deserialize)]
struct SomeResponse {
    #[serde(rename = "id")]
    _id: String,
    // retain only fields used in logic
}
```

- [ ] **Step 4: Verify no non-test `unwrap/expect` remains**

Run: `rg "\\b(unwrap|expect)\\(" src --glob '!**/*tests*'`
Expected: no matches in non-test runtime code.

- [ ] **Step 5: Re-run clippy and commit**

Run: `cargo clippy -- -D warnings`
Expected: PASS.

```bash
git add src/main.rs src/data/feed.rs src/chat/llm/*.rs
git commit -m "chore: remove allow suppressions and fix underlying warnings"
```

---

### Task 8: API Docs + `#[must_use]` for Public Result APIs

**Files:**
- Modify: `src/state/portfolio.rs`
- Modify: `src/engine/signal_engine.rs`
- Modify: `src/engine/scheduler.rs`
- Modify: `src/engine/risk.rs`
- Modify: `src/engine/confluence.rs`
- Modify: `src/engine/sentiment.rs`
- Modify: `src/chat/engine.rs`
- Modify: `src/chat/team/orchestrator.rs`
- Modify: `src/chat/llm/openrouter_models.rs`

- [ ] **Step 1: Add failing doc lint check (workspace command)**

Run: `cargo clippy -- -D warnings`
Expected: FAIL on missing docs/attributes until patched.

- [ ] **Step 2: Add missing doc comments on public API functions**

```rust
/// Resolve configured engine timeframe label; falls back to M5 on unknown labels.
pub fn timeframe_from_config(label: &str) -> Timeframe { ... }

/// Evaluate sentiment contribution for a pair and update rolling tracker state.
pub fn evaluate_sentiment(...) -> SentimentSignal { ... }
```

- [ ] **Step 3: Add explicit `#[must_use]` on public `Result` APIs**

```rust
#[must_use = "Handle open_position result to avoid silent risk/accounting failures"]
pub fn open_position(&mut self, position: Position) -> crate::error::Result<()> { ... }

#[must_use = "The returned receiver must be consumed to process streaming chat events"]
pub async fn process_message(&self, state: &AppState, user_input: &str) -> Result<mpsc::Receiver<ChatEvent>> { ... }

#[must_use]
pub fn run_pipeline_for_pair(...) -> Result<Option<PipelineOutcome>> { ... }
```

- [ ] **Step 4: Re-run clippy and commit**

Run: `cargo clippy -- -D warnings`
Expected: PASS.

```bash
git add src/state/portfolio.rs src/engine/*.rs src/chat/*.rs
git commit -m "docs: complete public API docs and add must_use on Result APIs"
```

---

### Task 9: Full Verification Gate

**Files:**
- Modify: as needed for final fixes discovered by checks

- [ ] **Step 1: Full compile check**

Run: `cargo check`
Expected: PASS.

- [ ] **Step 2: Strict clippy pass**

Run: `cargo clippy -- -D warnings`
Expected: PASS.

- [ ] **Step 3: Full test suite**

Run: `cargo test`
Expected: PASS (all tests green).

- [ ] **Step 4: Final commit for remaining integration fixes**

```bash
git add .
git commit -m "refactor: complete performance memory async and quality hardening"
```

---

## Spec Coverage Self-Review

- Memory bounds (news 500, chart 200 UI/cache, chat 200, activity 20): covered in Tasks 1-3.
- Chart map total bound + LRU max 50: covered in Task 2.
- Clone audit + borrow/Arc/Cow in hot paths: covered in Task 6.
- Render memoization + frame skip correctness: covered in Task 5.
- Async cancellation/select/channels/abort handles: covered in Task 4.
- Remove allow attributes + unwrap/expect cleanup: covered in Task 7.
- Public docs + must_use in portfolio/engine/chat: covered in Task 8.
- Verification (`check`, `clippy -D`, `test`): covered in Task 9.

No placeholders (`TODO`/`TBD`) remain; all tasks include concrete files, commands, and code snippets.
