//! M17 — MarketDataAdapter trait + Yahoo Finance impl. Stub.

use async_trait::async_trait;
use chrono::NaiveDate;

use crate::domain::{Micro, Ticker};
use crate::errors::SourceError;

#[async_trait]
pub trait MarketDataAdapter: Send + Sync {
    async fn historical_prices(
        &self,
        ticker: &Ticker,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<(NaiveDate, Micro)>, SourceError>;

    async fn current_price(&self, ticker: &Ticker) -> Result<Micro, SourceError>;
}
