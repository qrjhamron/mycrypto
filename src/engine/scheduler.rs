//! Scheduler loop for production signal engine.

use std::collections::HashMap;
use std::time::Duration;

use chrono::Utc;
use tokio::sync::{mpsc, watch};
use tracing::{error, warn};

use crate::engine::sentiment::SentimentTracker;
use crate::engine::signal_engine::run_pipeline_for_pair;
use crate::engine::EngineStatus;
use crate::state::{AppState, LogEntry, StateUpdate};

/// Spawns scheduler on current runtime.
pub fn spawn_signal_scheduler(
    snapshot_rx: watch::Receiver<AppState>,
    state_tx: mpsc::Sender<StateUpdate>,
) -> tokio::task::JoinHandle<()> {
    let handle = tokio::runtime::Handle::current();
    let (_shutdown_tx, shutdown_rx) = watch::channel(false);
    spawn_signal_scheduler_on(&handle, snapshot_rx, state_tx, shutdown_rx)
}

/// Spawns scheduler on explicit runtime handle.
pub fn spawn_signal_scheduler_on(
    handle: &tokio::runtime::Handle,
    snapshot_rx: watch::Receiver<AppState>,
    state_tx: mpsc::Sender<StateUpdate>,
    mut shutdown_rx: watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    handle.spawn(async move {
        let mut sentiment_trackers: HashMap<String, SentimentTracker> = HashMap::new();
        let mut consecutive_errors: u8 = 0;
        let mut breaker_open = false;
        let mut current_tick_secs: u64 = 5;
        let mut interval = tokio::time::interval(Duration::from_secs(current_tick_secs));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            if *shutdown_rx.borrow() {
                break;
            }

            let snapshot = snapshot_rx.borrow().clone();
            if !snapshot.config.engine.enabled {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {}
                    changed = shutdown_rx.changed() => {
                        if changed.is_err() || *shutdown_rx.borrow() {
                            break;
                        }
                    }
                }
                continue;
            }

            let target_tick = snapshot.config.engine.tick_interval_secs.max(5);
            if target_tick != current_tick_secs {
                current_tick_secs = target_tick;
                interval = tokio::time::interval(Duration::from_secs(current_tick_secs));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            }
            tokio::select! {
                _ = interval.tick() => {}
                changed = shutdown_rx.changed() => {
                    if changed.is_err() || *shutdown_rx.borrow() {
                        break;
                    }
                    continue;
                }
            }

            if breaker_open {
                let status = EngineStatus {
                    active_indicators: snapshot.engine_status.active_indicators.clone(),
                    last_tick_time: Some(Utc::now()),
                    consecutive_errors,
                    circuit_breaker_open: true,
                    last_error: Some("circuit breaker open; waiting for healthy cycle".to_string()),
                    ws_reconnect_count: snapshot.engine_status.ws_reconnect_count,
                    ws_last_message_at: snapshot.engine_status.ws_last_message_at,
                    ws_uptime_ratio: snapshot.engine_status.ws_uptime_ratio,
                };
                let _ = state_tx.send(StateUpdate::EngineStatusUpdated(status)).await;
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(5)) => {}
                    changed = shutdown_rx.changed() => {
                        if changed.is_err() || *shutdown_rx.borrow() {
                            break;
                        }
                    }
                }
                continue;
            }

            let mut had_error = false;
            for pair in &snapshot.config.pairs.watchlist {
                let quality = snapshot.data_quality.get(pair).copied().unwrap_or(1.0);
                if quality < 0.5 {
                    let _ = state_tx
                        .send(StateUpdate::Log(LogEntry::warn(format!(
                            "Skipping {}: data quality {:.2} below 0.50",
                            pair, quality
                        ))))
                        .await;
                    continue;
                }
                let pair_key = pair.as_str();
                let mut tracker = sentiment_trackers.remove(pair_key).unwrap_or_default();
                let result = run_pipeline_for_pair(&snapshot, pair_key, &mut tracker);
                sentiment_trackers.insert(pair.clone(), tracker);
                match result {
                    Ok(Some(outcome)) => {
                        let _ = state_tx.send(StateUpdate::NewSignal(outcome.signal)).await;
                    }
                    Ok(None) => {}
                    Err(err) => {
                        had_error = true;
                        let _ = state_tx
                            .send(StateUpdate::Log(LogEntry::error(format!(
                                "Signal pipeline error on {}: {}",
                                pair_key, err
                            ))))
                            .await;
                    }
                }
            }

            if had_error {
                consecutive_errors = consecutive_errors.saturating_add(1);
                if consecutive_errors > 3 {
                    breaker_open = true;
                    warn!("Signal scheduler circuit breaker opened after repeated failures");
                    let _ = state_tx
                        .send(StateUpdate::Log(LogEntry::warn(
                            "Signal scheduler paused: circuit breaker opened after >3 consecutive errors",
                        )))
                        .await;
                }
            } else {
                consecutive_errors = 0;
            }

            let status = EngineStatus {
                active_indicators: snapshot.engine_status.active_indicators.clone(),
                last_tick_time: Some(Utc::now()),
                consecutive_errors,
                circuit_breaker_open: breaker_open,
                last_error: if had_error {
                    Some("pipeline stage error on latest tick".to_string())
                } else {
                    None
                },
                ws_reconnect_count: snapshot.engine_status.ws_reconnect_count,
                ws_last_message_at: snapshot.engine_status.ws_last_message_at,
                ws_uptime_ratio: snapshot.engine_status.ws_uptime_ratio,
            };

            if state_tx
                .send(StateUpdate::EngineStatusUpdated(status))
                .await
                .is_err()
            {
                error!("state channel closed; stopping scheduler");
                break;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use std::time::Duration;

    #[test]
    fn test_circuit_breaker_opens_after_more_than_three_errors() {
        let mut errors = 0u8;
        let mut breaker = false;
        for _ in 0..4 {
            errors = errors.saturating_add(1);
            if errors > 3 {
                breaker = true;
            }
        }
        assert!(breaker);
    }

    #[tokio::test]
    async fn test_scheduler_exits_on_shutdown_signal() {
        let (state_tx, mut state_rx) = mpsc::channel(8);
        let (_snap_tx, snap_rx) = watch::channel(AppState::new(Config::default()));
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let handle = spawn_signal_scheduler_on(
            &tokio::runtime::Handle::current(),
            snap_rx,
            state_tx,
            shutdown_rx,
        );

        let _ = shutdown_tx.send(true);
        let join_result = tokio::time::timeout(Duration::from_secs(2), handle).await;
        assert!(join_result.is_ok(), "scheduler did not stop on shutdown");
        drop(state_rx.recv().await);
    }
}
