//! M13 — SEC HTTP client.
//!
//! - User-Agent header (required by SEC's fair-access policy).
//! - Host allowlist (`www.sec.gov`, `data.sec.gov`).
//! - Token-bucket rate limiter, default 5 req/s (well under SEC's 10/s ceiling).
//! - Exponential backoff on 429 / 5xx, capped at 60 s.
//!
//! Verified live against the SEC docs and `data.sec.gov` endpoints during
//! the architecture review pass (see `docs/architecture.md` Verified claims
//! 1–9, 18).

use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use governor::{Quota, RateLimiter};
use reqwest::{Client, ClientBuilder, StatusCode};
use serde::de::DeserializeOwned;

use crate::errors::SourceError;

const ALLOWED_HOSTS: &[&str] = &["www.sec.gov", "data.sec.gov"];

pub struct SecClient {
    http: Client,
    user_agent: String,
    limiter:
        Arc<RateLimiter<governor::state::NotKeyed, governor::state::InMemoryState, governor::clock::DefaultClock>>,
}

impl SecClient {
    pub fn new(user_agent: impl Into<String>, rps: u32) -> Result<Self, SourceError> {
        let user_agent = user_agent.into();
        let http = ClientBuilder::new()
            .user_agent(&user_agent)
            .gzip(true)
            .timeout(Duration::from_secs(60))
            .build()?;
        let nz = NonZeroU32::new(rps.max(1)).unwrap();
        let limiter = Arc::new(RateLimiter::direct(Quota::per_second(nz)));
        Ok(SecClient { http, user_agent, limiter })
    }

    pub fn user_agent(&self) -> &str { &self.user_agent }

    fn allowed(url: &str) -> bool {
        match reqwest::Url::parse(url) {
            Ok(u) => u.host_str().map(|h| ALLOWED_HOSTS.contains(&h)).unwrap_or(false),
            Err(_) => false,
        }
    }

    async fn wait(&self) {
        self.limiter.until_ready().await;
    }

    async fn get_with_backoff(&self, url: &str) -> Result<reqwest::Response, SourceError> {
        if !Self::allowed(url) {
            return Err(SourceError::SchemaMismatch {
                url: url.to_string(),
                detail: "host not in SEC allowlist".into(),
            });
        }
        let mut delay = Duration::from_millis(500);
        let max = Duration::from_secs(60);
        for _ in 0..6 {
            self.wait().await;
            let resp = self.http.get(url).send().await?;
            match resp.status() {
                StatusCode::OK => return Ok(resp),
                s if s.as_u16() == 429 || s.is_server_error() => {
                    tokio::time::sleep(delay).await;
                    delay = std::cmp::min(delay.saturating_mul(2), max);
                    continue;
                }
                s => return Err(SourceError::Http { status: s.as_u16(), url: url.to_string() }),
            }
        }
        Err(SourceError::RateLimit { url: url.to_string() })
    }

    pub async fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T, SourceError> {
        let resp = self.get_with_backoff(url).await?;
        let v = resp.json::<T>().await?;
        Ok(v)
    }

    pub async fn get_bytes(&self, url: &str) -> Result<Vec<u8>, SourceError> {
        let resp = self.get_with_backoff(url).await?;
        let b = resp.bytes().await?;
        Ok(b.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_accepts_data_sec_gov() {
        assert!(SecClient::allowed("https://data.sec.gov/submissions/CIK0000320193.json"));
    }

    #[test]
    fn allowlist_accepts_www_sec_gov() {
        assert!(SecClient::allowed("https://www.sec.gov/files/company_tickers.json"));
    }

    #[test]
    fn allowlist_rejects_other_hosts() {
        assert!(!SecClient::allowed("https://example.com/x"));
        assert!(!SecClient::allowed("https://evil.data.sec.gov.attacker.com/x"));
    }
}
