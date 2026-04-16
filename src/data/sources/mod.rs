//! External data sources for macro, sentiment, and news context.

pub mod aggregator;
pub mod binance;
pub mod cache;
pub mod coingecko;
pub mod cryptopanic;
pub mod feargreed;
pub mod finnhub;
pub mod news_refresh;
pub mod newsdata;
pub mod reddit;
pub mod rss;
pub mod twitter;
pub mod yahoo;

pub use aggregator::{spawn_sources_aggregator, spawn_sources_aggregator_on};
