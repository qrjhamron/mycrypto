//! Sentiment stage for production signal engine.

use std::collections::VecDeque;

use crate::state::{AppState, SignalDirection};

/// Sentiment stage output.
#[derive(Debug, Clone)]
pub struct SentimentSignal {
    pub pair: String,
    pub score: f32,
    pub delta_3tick: f32,
    pub direction: SignalDirection,
}

/// Keeps sentiment history to compute momentum delta over last 3 ticks.
#[derive(Debug, Default, Clone)]
pub struct SentimentTracker {
    history: VecDeque<f32>,
}

impl SentimentTracker {
    /// Appends a new sentiment sample and keeps a bounded history window.
    pub fn push(&mut self, value: f32) {
        self.history.push_back(value);
        while self.history.len() > 8 {
            self.history.pop_front();
        }
    }

    /// Computes momentum as current score minus the value from 3 ticks ago.
    #[must_use]
    pub fn delta_over_3_ticks(&self) -> f32 {
        if self.history.len() < 4 {
            return 0.0;
        }

        let current = self.history.back().copied().unwrap_or(0.0);
        let anchor_idx = self.history.len().saturating_sub(4);
        let previous = self.history.get(anchor_idx).copied().unwrap_or(current);
        current - previous
    }
}

/// Builds a sentiment stage output from composite sentiment and headline context.
#[must_use]
pub fn evaluate_sentiment(
    state: &AppState,
    pair: &str,
    tracker: &mut SentimentTracker,
) -> SentimentSignal {
    let base_score = state
        .sentiment_score
        .as_ref()
        .map(|s| s.composite)
        .unwrap_or(0.0);

    let (news_sum, news_count) = state
        .news_headlines
        .iter()
        .filter_map(|h| h.sentiment)
        .take(12)
        .fold((0.0f32, 0usize), |(sum, count), value| {
            (sum + value, count + 1)
        });

    let blended = if news_count == 0 {
        base_score
    } else {
        let avg_news = news_sum / news_count as f32;
        (base_score * 0.8) + (avg_news * 0.2)
    };

    tracker.push(blended);
    let delta = tracker.delta_over_3_ticks();

    let direction = if blended >= 0.1 && delta >= 0.0 {
        SignalDirection::Long
    } else if blended <= -0.1 && delta <= 0.0 {
        SignalDirection::Short
    } else {
        SignalDirection::Wait
    };

    SentimentSignal {
        pair: pair.to_string(),
        score: blended,
        delta_3tick: delta,
        direction,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sentiment_delta_over_3_ticks() {
        let mut tracker = SentimentTracker::default();
        tracker.push(0.10);
        tracker.push(0.20);
        tracker.push(0.30);
        tracker.push(0.65);
        let delta = tracker.delta_over_3_ticks();
        assert!((delta - 0.55).abs() < 0.0001);
    }
}
