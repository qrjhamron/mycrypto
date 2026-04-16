# mycrypto Performance, Memory, and Code Quality Audit Design

Date: 2026-04-15
Status: Approved design (implementation pending)
Scope: Full `src/` audit-driven refactor

## 1) Objectives

This work hardens runtime behavior and maintainability without changing product semantics.

Primary goals:

1. Enforce bounded memory for user/session-facing collections.
2. Reduce avoidable cloning and per-frame/per-tick allocations.
3. Ensure background tasks and channels are cancellation-safe and bounded.
4. Remove warning suppressions and panic-prone non-test code patterns.
5. Pass strict verification gates:
   - `cargo check`
   - `cargo clippy -- -D warnings`
   - `cargo test`

Explicit user decision captured:

- `200 candles per timeframe` applies to chart/UI and persisted chart cache only.
- Engine analysis buffers remain strategy-configurable and are not forcibly reduced to 200.

## 2) Non-goals

- No strategy logic redesign.
- No user-facing workflow changes unrelated to performance/memory/quality.
- No broad architectural rewrite beyond what is required for bounds, caching, and lifecycle safety.

## 3) Target Architecture and Data Structure Changes

### 3.1 Bounded ring buffers (`VecDeque`)

Replace unbounded session/history vectors with bounded `VecDeque` and push-back/pop-front behavior:

- News feed/history: max 500 items.
- Chat history: max 200 messages.
- Activity events: max 20 entries.

Implementation pattern:

1. Push new item with `push_back`.
2. While `len() > cap`, call `pop_front`.
3. Keep display helpers returning slices/iterators without cloning.

### 3.2 Chart candle and cache bounds

- Cap chart candle series stored for UI/cache to last 200 candles after normalization/sort.
- Add bounded chart cache entry count with LRU eviction:
  - Max 50 pair/timeframe entries.
  - Eviction removes both candle series and corresponding fetch metadata.

Design shape:

- Introduce cache owner type in state layer (or equivalent utility) containing:
  - `HashMap<String, ChartSeriesEntry>`
  - LRU order queue (`VecDeque<String>`) and membership maintenance.
- Any cache hit/mutation updates recency.
- On insert overflow, evict least-recently-used key.

## 4) CPU and Render Loop Optimization Design

### 4.1 Move heavy render work to update path

Computation that does not change per frame is moved to state-update/tick recompute points:

- Formatted strings used repeatedly in pages.
- Sorted source/key lists.
- News filter indices and normalized lowercase search keys.
- Chart-derived display artifacts that depend on stable inputs.

### 4.2 Memoized view models

Add memoized page-level view models in `UiShellState`/page cache:

- Recompute only when relevant state revision changes.
- Render uses precomputed model data with minimal formatting/allocation.

### 4.3 Frame diff guard hardening

Replace debug-string-heavy frame hashing with structured hashing over visible-state fields.

- Keep animation fields isolated from full-page structural hash.
- Ensure unchanged content paths skip redraw correctly.
- Only animate and invalidate regions when animation is visible/active.

## 5) Clone and Allocation Reduction Design

### 5.1 State/update hot paths

- Replace full-collection clones with `std::mem::take` + merge where safe.
- Avoid duplicate candle/news materialization by moving values once and sharing via references.

### 5.2 Shared large data

- Use `Arc` for immutable payloads fanned out across tasks (notably team orchestration and scheduler-adjacent shared contexts).
- Keep cheap clone types unchanged where clone is effectively Arc copy.

### 5.3 String ownership in hot render paths

- Replace render-loop `String` clones with `&str` and `Cow<str>` where lifetimes permit.
- Keep owned strings only at mutation boundaries.

## 6) Async Safety and Task Lifecycle Design

### 6.1 Bounded channels and backpressure

- Replace any unbounded channels with bounded `tokio::mpsc` (capacity 256).
- Add explicit logging when producers encounter full channels (blocked send or `try_send` failure).

### 6.2 Cancellation and shutdown guarantees

All background systems have explicit cancellation/abort handles held by `App` and shut down on exit:

- News refresh worker.
- Chart refresh worker.
- Engine scheduler.
- WebSocket feed.
- Source aggregation loops.

Use `tokio::select!` with cancellation signals where loops currently rely on channel failure or implicit task drop.

### 6.3 Mutex discipline

- No `std::sync::Mutex` held across await points.
- Use `tokio::sync::Mutex` for async-shared mutable state where locking across async boundaries is required.

## 7) Code Quality and API Contracts

### 7.1 Lint suppression removal

Remove all:

- `#[allow(dead_code)]`
- `#[allow(unused)]`
- `#[allow(warnings)]`

and fix the underlying warnings instead of suppressing them.

### 7.2 Panic removal in non-test code

Replace all non-test `unwrap()`/`expect()` with:

- error propagation via `?`, or
- explicit logged fallback behavior when continuation is acceptable.

### 7.3 Documentation and `#[must_use]`

- Add/complete doc comments for all public API functions in:
  - `src/state/portfolio.rs`
  - `src/engine/`
  - `src/chat/`
- Add explicit `#[must_use]` on public APIs returning `Result` in those modules for clear intent and consistency.

## 8) Execution Phases

1. Data bounds + LRU implementation.
2. Clone/allocation reductions in hot paths.
3. Render-loop memoization and frame-skip correctness.
4. Async cancellation/channel hardening.
5. Lint/docs/must_use cleanup.
6. Verification and stabilization.

Each phase ends with at least `cargo check`; final phase requires full strict gate.

## 9) Verification Plan

Mandatory final evidence:

1. `cargo check`
2. `cargo clippy -- -D warnings`
3. `cargo test`

Additionally, targeted regression checks for:

- Ring-buffer cap enforcement (news/chat/activity).
- Chart series truncation to 200 and chart LRU eviction at 50.
- Render memoization invalidation correctness.
- Clean cancellation of all background tasks on shutdown.

## 10) Risk Management

Primary risks:

- Behavioral regression from data retention changes.
- Stale UI data due to overly aggressive memoization.
- Shutdown race conditions while introducing explicit cancellation.

Mitigations:

- Introduce phase-local tests for each cap/eviction/cancellation behavior.
- Use narrow, incremental changes with compile/test gates after each phase.
- Keep engine-analysis depth semantics unchanged per user decision.
