//! Binance REST helpers for supplemental market data.

use std::str::FromStr;

use chrono::{DateTime, TimeZone, Utc};
use rust_decimal::Decimal;
use serde::Deserialize;

use crate::error::{MycryptoError, Result};
use crate::state::{Timeframe, OHLCV};

#[derive(Debug, Clone)]
pub struct FundingRateSnapshot {
    pub pair: String,
    pub rate: Decimal,
    pub next_time: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct FundingRateResponse {
    symbol: String,
    #[serde(rename = "fundingRate")]
    funding_rate: String,
    #[serde(rename = "fundingTime")]
    funding_time: i64,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum KlineField {
    Str(String),
    Num(i64),
}

impl KlineField {
    fn as_str(&self) -> Option<&str> {
        match self {
            KlineField::Str(s) => Some(s.as_str()),
            KlineField::Num(_) => None,
        }
    }

    fn as_i64(&self) -> Option<i64> {
        match self {
            KlineField::Num(v) => Some(*v),
            KlineField::Str(_) => None,
        }
    }
}

/// Builds Binance REST klines URL path/query.
pub fn build_klines_query(pair: &str, timeframe: Timeframe, limit: usize) -> String {
    format!(
        "/api/v3/klines?symbol={}&interval={}&limit={}",
        pair.to_ascii_uppercase(),
        timeframe.as_binance_interval(),
        limit
    )
}

/// Fetch historical klines for a pair/timeframe using Binance spot REST.
pub async fn fetch_binance_klines(
    client: &reqwest::Client,
    pair: &str,
    timeframe: Timeframe,
    limit: usize,
) -> Result<Vec<OHLCV>> {
    let query = build_klines_query(pair, timeframe, limit);
    let url = format!("https://api.binance.com{}", query);

    let response = client.get(url).send().await?;
    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        return Err(MycryptoError::ApiError {
            api: "binance/klines".to_string(),
            status,
            message: body,
        });
    }

    let rows: Vec<Vec<KlineField>> = response.json().await?;
    let mut out = Vec::with_capacity(rows.len());

    for row in rows {
        if row.len() < 9 {
            return Err(MycryptoError::MarketDataParse(
                "binance kline row has insufficient fields".to_string(),
            ));
        }

        let open_time = row[0].as_i64().ok_or_else(|| {
            MycryptoError::MarketDataParse("binance kline open time missing".to_string())
        })?;

        let open = Decimal::from_str(row[1].as_str().ok_or_else(|| {
            MycryptoError::MarketDataParse("binance kline open missing".to_string())
        })?)
        .map_err(|e| MycryptoError::MarketDataParse(format!("invalid open: {}", e)))?;

        let high = Decimal::from_str(row[2].as_str().ok_or_else(|| {
            MycryptoError::MarketDataParse("binance kline high missing".to_string())
        })?)
        .map_err(|e| MycryptoError::MarketDataParse(format!("invalid high: {}", e)))?;

        let low = Decimal::from_str(row[3].as_str().ok_or_else(|| {
            MycryptoError::MarketDataParse("binance kline low missing".to_string())
        })?)
        .map_err(|e| MycryptoError::MarketDataParse(format!("invalid low: {}", e)))?;

        let close = Decimal::from_str(row[4].as_str().ok_or_else(|| {
            MycryptoError::MarketDataParse("binance kline close missing".to_string())
        })?)
        .map_err(|e| MycryptoError::MarketDataParse(format!("invalid close: {}", e)))?;

        let volume = Decimal::from_str(row[5].as_str().ok_or_else(|| {
            MycryptoError::MarketDataParse("binance kline volume missing".to_string())
        })?)
        .map_err(|e| MycryptoError::MarketDataParse(format!("invalid volume: {}", e)))?;

        let trades = row[8].as_i64().unwrap_or(0).max(0) as u64;

        let ts = Utc
            .timestamp_millis_opt(open_time)
            .single()
            .ok_or_else(|| {
                MycryptoError::MarketDataParse("binance kline timestamp invalid".to_string())
            })?;

        out.push(OHLCV {
            timestamp: ts,
            open,
            high,
            low,
            close,
            volume,
            trades,
            closed: true,
        });
    }

    Ok(out)
}

/// Fetch latest funding rates from Binance Futures.
pub async fn fetch_binance_funding_rates(
    client: &reqwest::Client,
) -> Result<Vec<FundingRateSnapshot>> {
    let response = client
        .get("https://fapi.binance.com/fapi/v1/fundingRate?limit=100")
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        return Err(MycryptoError::ApiError {
            api: "binance/fundingRate".to_string(),
            status,
            message: body,
        });
    }

    let payload: Vec<FundingRateResponse> = response.json().await?;
    let mut out = Vec::new();

    for item in payload {
        let rate = Decimal::from_str(&item.funding_rate).map_err(|e| {
            MycryptoError::MarketDataParse(format!(
                "invalid funding rate '{}' for {}: {}",
                item.funding_rate, item.symbol, e
            ))
        })?;

        let funding_time = Utc
            .timestamp_millis_opt(item.funding_time)
            .single()
            .unwrap_or_else(Utc::now);

        out.push(FundingRateSnapshot {
            pair: item.symbol,
            rate,
            next_time: funding_time + chrono::Duration::hours(8),
        });
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_klines_query() {
        assert_eq!(
            build_klines_query("btcusdt", Timeframe::H1, 200),
            "/api/v3/klines?symbol=BTCUSDT&interval=1h&limit=200"
        );
        assert_eq!(
            build_klines_query("ETHUSDT", Timeframe::MO1, 200),
            "/api/v3/klines?symbol=ETHUSDT&interval=1M&limit=200"
        );
    }
}
