//! M14 — company_tickers fetcher and ticker→CIK lookup.
//!
//! Primary source: `https://www.sec.gov/files/company_tickers.json` (the
//! authoritative full mapping). Some networks aggressively rate-limit
//! `www.sec.gov`, so we ship a small fallback map for the most common
//! tickers. The fallback is used only when the live fetch fails; ingestion
//! through `data.sec.gov` (which is on a different host) works regardless.

use std::collections::HashMap;

use serde::Deserialize;

use crate::domain::{Cik, Ticker};
use crate::errors::SourceError;

use super::sec_client::SecClient;

const URL: &str = "https://www.sec.gov/files/company_tickers.json";

/// Bundled minimal ticker→(cik, name) map used when the live SEC fetch fails.
/// Covers the most-frequently-requested public companies. The full live map
/// has ~10,000 entries; this is a deliberately small fallback.
const FALLBACK_MAP: &[(&str, &str, &str)] = &[
    ("AAPL", "0000320193", "Apple Inc."),
    ("MSFT", "0000789019", "Microsoft Corp"),
    ("GOOGL", "0001652044", "Alphabet Inc."),
    ("GOOG", "0001652044", "Alphabet Inc."),
    ("AMZN", "0001018724", "Amazon.com Inc"),
    ("META", "0001326801", "Meta Platforms Inc"),
    ("NVDA", "0001045810", "NVIDIA Corp"),
    ("TSLA", "0001318605", "Tesla Inc"),
    ("BRK.B", "0001067983", "Berkshire Hathaway"),
    ("JPM", "0000019617", "JPMorgan Chase"),
    ("V", "0001403161", "Visa Inc"),
    ("MA", "0001141391", "Mastercard Inc"),
    ("UNH", "0000731766", "UnitedHealth Group"),
    ("HD", "0000354950", "Home Depot Inc"),
    ("PG", "0000080424", "Procter & Gamble"),
    ("XOM", "0000034088", "Exxon Mobil"),
    ("LLY", "0000059478", "Eli Lilly"),
    ("AVGO", "0001730168", "Broadcom Inc"),
    ("COST", "0000909832", "Costco Wholesale"),
    ("WMT", "0000104169", "Walmart Inc"),
    ("CVX", "0000093410", "Chevron Corp"),
    ("ABBV", "0001551152", "AbbVie Inc"),
    ("KO", "0000021344", "Coca-Cola Co"),
    ("PEP", "0000077476", "PepsiCo Inc"),
    ("ORCL", "0001341439", "Oracle Corp"),
    ("ADBE", "0000796343", "Adobe Inc"),
    ("CRM", "0001108524", "Salesforce Inc"),
    ("NFLX", "0001065280", "Netflix Inc"),
    ("DIS", "0001744489", "Walt Disney Co"),
    ("INTC", "0000050863", "Intel Corp"),
    ("AMD", "0000002488", "Advanced Micro Devices"),
];

/// Raw shape of `company_tickers.json` — top-level is a JSON object whose
/// keys are stringified indexes and whose values are entries.
#[derive(Debug, Deserialize)]
struct Entry {
    cik_str: serde_json::Value, // sometimes number, sometimes string
    ticker: String,
    title: String,
}

pub struct TickerMap {
    by_ticker: HashMap<Ticker, (Cik, String)>,
}

impl TickerMap {
    /// Load with live fetch + automatic fallback to the bundled map on failure.
    pub async fn load(client: &SecClient) -> Result<Self, SourceError> {
        match Self::load_live(client).await {
            Ok(m) => Ok(m),
            Err(e) => {
                tracing::warn!("ticker map live fetch failed ({e}); using bundled fallback");
                Ok(Self::fallback())
            }
        }
    }

    pub async fn load_live(client: &SecClient) -> Result<Self, SourceError> {
        let raw: HashMap<String, Entry> = client.get_json(URL).await?;
        let mut by_ticker = HashMap::with_capacity(raw.len());
        for (_, e) in raw {
            let cik_num: u64 = match &e.cik_str {
                serde_json::Value::Number(n) => n.as_u64().unwrap_or(0),
                serde_json::Value::String(s) => s.parse().unwrap_or(0),
                _ => 0,
            };
            if cik_num == 0 { continue; }
            let cik = Cik(format!("{:0>10}", cik_num));
            let t = Ticker::from_str(&e.ticker);
            by_ticker.insert(t, (cik, e.title));
        }
        Ok(TickerMap { by_ticker })
    }

    pub fn fallback() -> Self {
        let mut by_ticker = HashMap::with_capacity(FALLBACK_MAP.len());
        for (t, c, n) in FALLBACK_MAP {
            by_ticker.insert(Ticker::from_str(t), (Cik((*c).to_string()), (*n).to_string()));
        }
        TickerMap { by_ticker }
    }

    pub fn lookup(&self, ticker: &Ticker) -> Option<(Cik, String)> {
        self.by_ticker.get(ticker).cloned()
    }

    pub fn len(&self) -> usize { self.by_ticker.len() }
    pub fn is_empty(&self) -> bool { self.by_ticker.is_empty() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_company_tickers_shape() {
        // Mirrors the actual SEC payload.
        let raw_json = json!({
            "0": { "cik_str": 320193, "ticker": "AAPL", "title": "Apple Inc." },
            "1": { "cik_str": 789019, "ticker": "MSFT", "title": "MICROSOFT CORP" },
        });
        let raw: HashMap<String, Entry> = serde_json::from_value(raw_json).unwrap();
        let mut m = HashMap::new();
        for (_, e) in raw {
            let cik_num: u64 = match &e.cik_str {
                serde_json::Value::Number(n) => n.as_u64().unwrap_or(0),
                serde_json::Value::String(s) => s.parse().unwrap_or(0),
                _ => 0,
            };
            let cik = Cik(format!("{:0>10}", cik_num));
            let t = Ticker::from_str(&e.ticker);
            m.insert(t, (cik, e.title));
        }
        let map = TickerMap { by_ticker: m };
        let (cik, name) = map.lookup(&Ticker("AAPL".into())).unwrap();
        assert_eq!(cik.0, "0000320193");
        assert_eq!(name, "Apple Inc.");
    }
}
