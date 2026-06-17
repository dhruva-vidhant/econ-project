//! M17 — MarketDataAdapter trait + Yahoo Finance implementation.
//!
//! Historical daily closes and the current price come from Yahoo Finance's
//! public chart endpoint (`query1.finance.yahoo.com/v8/finance/chart/{symbol}`),
//! the host already named in the app CSP `connect-src` allowlist. Prices are
//! returned in **USD micro-units** (USD × 1,000,000); the adapter enforces a
//! USD currency guard so a foreign-listed quote can never silently feed a
//! wrong market-cap computation (accuracy rule — V1 targets US issuers).
//!
//! Follows the same HTTP conventions as `SecClient`: a token-bucket rate
//! limiter and exponential backoff on 429/5xx.

use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use governor::{Quota, RateLimiter};
use reqwest::{Client, ClientBuilder, StatusCode};
use serde::Deserialize;

use crate::domain::{Micro, Ticker};
use crate::errors::SourceError;

const CHART_BASE: &str = "https://query1.finance.yahoo.com/v8/finance/chart/";

#[async_trait]
pub trait MarketDataAdapter: Send + Sync {
    /// Daily closing prices (USD micro-units) for `[from, to]`, ascending by
    /// date. Non-trading days are simply absent from the series.
    async fn historical_prices(
        &self,
        ticker: &Ticker,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<(NaiveDate, Micro)>, SourceError>;

    /// Latest regular-market price (USD micro-units).
    async fn current_price(&self, ticker: &Ticker) -> Result<Micro, SourceError>;
}

type DirectLimiter = RateLimiter<
    governor::state::NotKeyed,
    governor::state::InMemoryState,
    governor::clock::DefaultClock,
>;

pub struct YahooMarketData {
    http: Client,
    limiter: Arc<DirectLimiter>,
}

impl YahooMarketData {
    pub fn new() -> Result<Self, SourceError> {
        // Yahoo rejects requests without a browser-like User-Agent.
        let http = ClientBuilder::new()
            .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) EconProject/0.1")
            .gzip(true)
            .timeout(Duration::from_secs(30))
            .build()?;
        let nz = NonZeroU32::new(5).unwrap();
        let limiter = Arc::new(RateLimiter::direct(Quota::per_second(nz)));
        Ok(Self { http, limiter })
    }

    /// Yahoo uses a hyphen for share-class suffixes where SEC uses a dot
    /// (e.g. `BRK.B` → `BRK-B`).
    fn yahoo_symbol(ticker: &Ticker) -> String {
        ticker.0.replace('.', "-")
    }

    async fn fetch_chart(&self, symbol: &str, p1: i64, p2: i64) -> Result<ChartResult, SourceError> {
        let url = format!("{CHART_BASE}{symbol}?period1={p1}&period2={p2}&interval=1d");
        let mut delay = Duration::from_millis(500);
        let max = Duration::from_secs(30);
        for _ in 0..5 {
            self.limiter.until_ready().await;
            let resp = self.http.get(&url).send().await?;
            match resp.status() {
                StatusCode::OK => {
                    let env: ChartEnvelope = resp.json().await?;
                    return env
                        .chart
                        .result
                        .and_then(|mut r| (!r.is_empty()).then(|| r.remove(0)))
                        .ok_or_else(|| SourceError::SchemaMismatch {
                            url: url.clone(),
                            detail: "chart.result was empty".into(),
                        });
                }
                s if s.as_u16() == 429 || s.is_server_error() => {
                    tokio::time::sleep(delay).await;
                    delay = std::cmp::min(delay.saturating_mul(2), max);
                }
                s if s.as_u16() == 404 => {
                    return Err(SourceError::UnknownTicker(symbol.to_string()))
                }
                s => return Err(SourceError::Http { status: s.as_u16(), url }),
            }
        }
        Err(SourceError::RateLimit { url })
    }

    /// Reject non-USD quotes so they can never feed a wrong market cap.
    fn require_usd(meta: &Meta, symbol: &str) -> Result<(), SourceError> {
        match meta.currency.as_deref() {
            Some("USD") | None => Ok(()),
            Some(other) => Err(SourceError::Unavailable(format!(
                "{symbol} priced in {other}, not USD; market cap unsupported for non-USD listings"
            ))),
        }
    }
}

fn close_to_micro(close: f64) -> Micro {
    (close * 1_000_000.0).round() as Micro
}

#[async_trait]
impl MarketDataAdapter for YahooMarketData {
    async fn historical_prices(
        &self,
        ticker: &Ticker,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<(NaiveDate, Micro)>, SourceError> {
        let symbol = Self::yahoo_symbol(ticker);
        let p1 = from.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp();
        // Pad the upper bound by a day so the final period end is covered.
        let p2 = to.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp() + 86_400;
        let result = self.fetch_chart(&symbol, p1, p2).await?;
        Self::require_usd(&result.meta, &symbol)?;

        let timestamps = result.timestamp.unwrap_or_default();
        let closes = result
            .indicators
            .quote
            .into_iter()
            .next()
            .and_then(|q| q.close)
            .unwrap_or_default();

        let mut out = Vec::with_capacity(timestamps.len());
        for (ts, close) in timestamps.iter().zip(closes.iter()) {
            // Some sessions report a null close (e.g. trading halt); skip them.
            let (Some(close), Some(dt)) = (close, DateTime::<Utc>::from_timestamp(*ts, 0)) else {
                continue;
            };
            out.push((dt.date_naive(), close_to_micro(*close)));
        }
        Ok(out)
    }

    async fn current_price(&self, ticker: &Ticker) -> Result<Micro, SourceError> {
        let symbol = Self::yahoo_symbol(ticker);
        let now = Utc::now().timestamp();
        let result = self.fetch_chart(&symbol, now - 7 * 86_400, now).await?;
        Self::require_usd(&result.meta, &symbol)?;
        result
            .meta
            .regular_market_price
            .map(close_to_micro)
            .ok_or_else(|| SourceError::Unavailable(format!("no regularMarketPrice for {symbol}")))
    }
}

// ── Yahoo chart JSON shapes (only the fields we use) ─────────────────────────

#[derive(Deserialize)]
struct ChartEnvelope {
    chart: Chart,
}

#[derive(Deserialize)]
struct Chart {
    result: Option<Vec<ChartResult>>,
}

#[derive(Deserialize)]
struct ChartResult {
    meta: Meta,
    timestamp: Option<Vec<i64>>,
    indicators: Indicators,
}

#[derive(Deserialize)]
struct Meta {
    currency: Option<String>,
    #[serde(rename = "regularMarketPrice")]
    regular_market_price: Option<f64>,
}

#[derive(Deserialize)]
struct Indicators {
    #[serde(default)]
    quote: Vec<Quote>,
}

#[derive(Deserialize)]
struct Quote {
    close: Option<Vec<Option<f64>>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yahoo_symbol_maps_share_class_dot_to_hyphen() {
        assert_eq!(YahooMarketData::yahoo_symbol(&Ticker("BRK.B".into())), "BRK-B");
        assert_eq!(YahooMarketData::yahoo_symbol(&Ticker("AAPL".into())), "AAPL");
    }

    #[test]
    fn close_micro_rounds_to_nearest() {
        assert_eq!(close_to_micro(190.45), 190_450_000);
        assert_eq!(close_to_micro(299.2399995), 299_240_000);
    }

    #[test]
    fn require_usd_rejects_foreign_currency() {
        let usd = Meta { currency: Some("USD".into()), regular_market_price: None };
        let eur = Meta { currency: Some("EUR".into()), regular_market_price: None };
        assert!(YahooMarketData::require_usd(&usd, "X").is_ok());
        assert!(YahooMarketData::require_usd(&eur, "X").is_err());
    }

    #[test]
    fn parses_chart_envelope() {
        let body = r#"{"chart":{"result":[{"meta":{"currency":"USD","regularMarketPrice":299.24},
            "timestamp":[1704067200,1704153600],
            "indicators":{"quote":[{"close":[185.64,null]}]}}]}}"#;
        let env: ChartEnvelope = serde_json::from_str(body).unwrap();
        let r = env.chart.result.unwrap().remove(0);
        assert_eq!(r.meta.currency.as_deref(), Some("USD"));
        assert_eq!(r.meta.regular_market_price, Some(299.24));
        assert_eq!(r.timestamp.unwrap().len(), 2);
        let closes = r.indicators.quote.into_iter().next().unwrap().close.unwrap();
        assert_eq!(closes[0], Some(185.64));
        assert_eq!(closes[1], None);
    }
}
