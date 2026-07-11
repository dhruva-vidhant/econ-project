//! CSV import for custom metrics. See techspec §6.
//!
//! Never aborts on a bad row (FR-CM-004): rows are parsed and applied
//! independently, with per-row results collected into an `ImportReport`.

use std::collections::{HashMap, HashSet};

use crate::domain::{parse_micro, Cik, NewCustomMetric, Period, PeriodKind, UnitKind};
use crate::errors::RepoError;
use crate::repos::custom_metric::CustomMetricRepo;

use super::resolve::{resolve_period, Resolution};

#[derive(Debug, Clone)]
pub struct ImportOptions {
    pub granularity: PeriodKind,
    pub default_unit: UnitKind,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self { granularity: PeriodKind::Annual, default_unit: UnitKind::Number }
    }
}

#[derive(Debug, Clone)]
pub struct ImportRowResultDetail {
    pub row_index: usize,
    pub reason: String,
    pub raw: String,
}

#[derive(Debug, Clone, Default)]
pub struct ImportReport {
    pub metrics_created: Vec<String>,
    pub imported: u32,
    pub updated: u32,
    pub unaligned: u32,
    pub rejected: Vec<ImportRowResultDetail>,
}

/// Parses RFC 4180 CSV text into rows of raw string fields (first row is the
/// header). Handles quoted fields, embedded commas, escaped `""`, and CRLF.
fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut field = String::new();
    let mut row = Vec::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if in_quotes => {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            }
            '"' => in_quotes = true,
            ',' if !in_quotes => {
                row.push(std::mem::take(&mut field));
            }
            '\r' if !in_quotes => {}
            '\n' if !in_quotes => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            _ => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows.retain(|r| !(r.len() == 1 && r[0].trim().is_empty()));
    rows
}

fn normalize_unit_token(token: &str) -> Option<(UnitKind, bool)> {
    match token.trim().to_ascii_lowercase().as_str() {
        "currency" => Some((UnitKind::Currency, false)),
        "count" => Some((UnitKind::Count, false)),
        "ratio" => Some((UnitKind::Ratio, false)),
        "number" => Some((UnitKind::Number, false)),
        "percent" => Some((UnitKind::Ratio, true)),
        _ => None,
    }
}

/// Converts a value already parsed as micro-units (`raw = value * 1e6`) into
/// ratio-micro for a `percent` column: `round(value / 100 * 1e6)`.
fn percent_raw_to_ratio_micro(raw: i64) -> i64 {
    let r = raw as i128;
    let rounded = if r >= 0 { (r + 50) / 100 } else { (r - 50) / 100 };
    rounded as i64
}

pub async fn import_csv(
    cik: &Cik,
    csv_text: &str,
    options: &ImportOptions,
    repo: &dyn CustomMetricRepo,
    periods: &[Period],
) -> Result<ImportReport, RepoError> {
    let mut report = ImportReport::default();
    let rows = parse_csv(csv_text);
    let Some(header) = rows.first() else { return Ok(report) };

    let col = |name: &str| -> Option<usize> {
        header.iter().position(|h| h.trim().eq_ignore_ascii_case(name))
    };
    let metric_idx = col("metric");
    let period_idx = col("period");
    let value_idx = col("value");
    let unit_idx = col("unit");
    let label_idx = col("label");
    let currency_idx = col("currency");

    let mut metrics_by_key: HashMap<String, crate::domain::CustomMetric> = repo
        .list(cik)
        .await?
        .into_iter()
        .map(|m| (m.key.clone(), m))
        .collect();

    let mut existing_points: HashSet<(i64, i64)> = HashSet::new();
    for m in metrics_by_key.values() {
        for kind in [PeriodKind::Annual, PeriodKind::Quarterly] {
            for (period, _pt) in repo.points_aligned(m.id, kind.clone()).await? {
                existing_points.insert((m.id, period.id));
            }
        }
    }

    for (row_index, row) in rows[1..].iter().enumerate() {
        let raw = row.join(",");
        let get = |idx: Option<usize>| -> Option<String> {
            idx.and_then(|i| row.get(i)).map(|s| s.trim().to_string())
        };

        let (metric_str, period_str, value_str) = match (get(metric_idx), get(period_idx), get(value_idx)) {
            (Some(m), Some(p), Some(v)) if !m.is_empty() && !p.is_empty() && !v.is_empty() => (m, p, v),
            _ => {
                report.rejected.push(ImportRowResultDetail {
                    row_index,
                    reason: "missing_column".into(),
                    raw,
                });
                continue;
            }
        };

        let key = metric_str.to_ascii_lowercase();
        let unit_token = get(unit_idx).filter(|s| !s.is_empty());
        let label = get(label_idx).filter(|s| !s.is_empty());
        let currency = get(currency_idx).filter(|s| !s.is_empty());

        let parsed_unit = match &unit_token {
            Some(t) => match normalize_unit_token(t) {
                Some(u) => Some(u),
                None => {
                    report.rejected.push(ImportRowResultDetail {
                        row_index,
                        reason: "bad_value".into(),
                        raw,
                    });
                    continue;
                }
            },
            None => None,
        };

        let raw_micro = match parse_micro(&value_str) {
            Ok(v) => v,
            Err(_) => {
                report.rejected.push(ImportRowResultDetail {
                    row_index,
                    reason: "bad_value".into(),
                    raw,
                });
                continue;
            }
        };
        let value_micro = match parsed_unit {
            Some((_, true)) => percent_raw_to_ratio_micro(raw_micro),
            _ => raw_micro,
        };

        let metric_id = if let Some(existing) = metrics_by_key.get(&key) {
            if let Some((unit_kind, _)) = parsed_unit {
                if unit_kind != existing.unit_kind {
                    report.rejected.push(ImportRowResultDetail {
                        row_index,
                        reason: "unit_conflict".into(),
                        raw,
                    });
                    continue;
                }
            }
            existing.id
        } else {
            let create_unit = parsed_unit.map(|(u, _)| u).unwrap_or(options.default_unit);
            let default_currency = if create_unit == UnitKind::Currency {
                Some(currency.clone().unwrap_or_else(|| "USD".to_string()))
            } else {
                currency.clone()
            };
            let created = repo
                .create(
                    cik,
                    &NewCustomMetric {
                        key: key.clone(),
                        label: label.clone().unwrap_or_else(|| key.clone()),
                        unit_kind: create_unit,
                        currency: default_currency,
                        decimals: None,
                        description: None,
                    },
                )
                .await?;
            report.metrics_created.push(key.clone());
            let id = created.id;
            metrics_by_key.insert(key.clone(), created);
            id
        };

        match resolve_period(&period_str, options.granularity.clone(), periods) {
            Resolution::Matched(period_id) => {
                repo.upsert_point(metric_id, Some(period_id), None, value_micro, Some("csv import"))
                    .await?;
                if existing_points.insert((metric_id, period_id)) {
                    report.imported += 1;
                } else {
                    report.updated += 1;
                }
            }
            Resolution::Unaligned => {
                repo.upsert_point(metric_id, None, Some(&period_str), value_micro, Some("csv import"))
                    .await?;
                report.unaligned += 1;
            }
            Resolution::Error(e) => {
                report.rejected.push(ImportRowResultDetail {
                    row_index,
                    reason: e.0.to_string(),
                    raw,
                });
            }
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Pool;
    use crate::domain::{Company, Ticker};
    use crate::repos::company::{CompanyRepo, SqliteCompanyRepo};
    use crate::repos::custom_metric::SqliteCustomMetricRepo;
    use crate::repos::period::{PeriodRepo, SqlitePeriodRepo};
    use chrono::{NaiveDate, TimeZone, Utc};

    async fn setup() -> std::sync::Arc<Pool> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.sqlite");
        Box::leak(Box::new(dir));
        std::sync::Arc::new(Pool::open(&path).unwrap())
    }

    async fn seed_company(pool: &std::sync::Arc<Pool>) -> Cik {
        let companies = SqliteCompanyRepo::new(pool.clone());
        let cik = Cik("0000000001".into());
        companies
            .upsert(&Company {
                cik: cik.clone(),
                ticker: Ticker("PYPL".into()),
                name: "PayPal Holdings".into(),
                exchange: None,
                sic: None,
                fiscal_year_end: Some("1231".into()),
                added_at: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
                last_refreshed: None,
            })
            .await
            .unwrap();
        cik
    }

    async fn seed_annual_periods(pool: &std::sync::Arc<Pool>, cik: &Cik, years: &[i32]) -> Vec<Period> {
        let periods = SqlitePeriodRepo::new(pool.clone());
        let mut out = Vec::new();
        for &fy in years {
            let id = periods
                .upsert_returning_id(&Period {
                    id: 0,
                    cik: cik.clone(),
                    fiscal_year: fy,
                    fiscal_quarter: 0,
                    fiscal_year_end: "1231".into(),
                    start_date: NaiveDate::from_ymd_opt(fy, 1, 1).unwrap(),
                    end_date: NaiveDate::from_ymd_opt(fy, 12, 31).unwrap(),
                    kind: PeriodKind::Annual,
                    is_53_week: false,
                })
                .await
                .unwrap();
            out.push(Period {
                id,
                cik: cik.clone(),
                fiscal_year: fy,
                fiscal_quarter: 0,
                fiscal_year_end: "1231".into(),
                start_date: NaiveDate::from_ymd_opt(fy, 1, 1).unwrap(),
                end_date: NaiveDate::from_ymd_opt(fy, 12, 31).unwrap(),
                kind: PeriodKind::Annual,
                is_53_week: false,
            });
        }
        out
    }

    fn paypal_fixture() -> String {
        std::fs::read_to_string(
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/paypal_kpis.csv"),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn import_creates_metrics_and_points() {
        let pool = setup().await;
        let cik = seed_company(&pool).await;
        let periods = seed_annual_periods(&pool, &cik, &[2021, 2022, 2023, 2024]).await;
        let repo = SqliteCustomMetricRepo::new(pool);
        let report = import_csv(&cik, &paypal_fixture(), &ImportOptions::default(), &repo, &periods)
            .await
            .unwrap();
        assert_eq!(report.metrics_created.len(), 4);
        assert_eq!(report.imported, 16);
        assert_eq!(report.updated, 0);
        assert_eq!(report.rejected.len(), 0);
    }

    #[tokio::test]
    async fn import_is_idempotent() {
        let pool = setup().await;
        let cik = seed_company(&pool).await;
        let periods = seed_annual_periods(&pool, &cik, &[2021, 2022, 2023, 2024]).await;
        let repo = SqliteCustomMetricRepo::new(pool);
        import_csv(&cik, &paypal_fixture(), &ImportOptions::default(), &repo, &periods)
            .await
            .unwrap();
        let report = import_csv(&cik, &paypal_fixture(), &ImportOptions::default(), &repo, &periods)
            .await
            .unwrap();
        assert_eq!(report.imported, 0);
        assert_eq!(report.updated, 16);
        assert_eq!(report.metrics_created.len(), 0);
    }

    #[tokio::test]
    async fn import_percent_normalizes_to_ratio() {
        let pool = setup().await;
        let cik = seed_company(&pool).await;
        let periods = seed_annual_periods(&pool, &cik, &[2024]).await;
        let repo = SqliteCustomMetricRepo::new(pool);
        let csv = "metric,period,value,unit\nfcf_margin,FY2024,25.3,percent\n";
        import_csv(&cik, csv, &ImportOptions::default(), &repo, &periods).await.unwrap();
        let m = repo.get_by_key(&cik, "fcf_margin").await.unwrap().unwrap();
        assert_eq!(m.unit_kind, UnitKind::Ratio);
        let pts = repo.points_aligned(m.id, PeriodKind::Annual).await.unwrap();
        assert_eq!(pts[0].1.value_micro, 253_000);
    }

    #[tokio::test]
    async fn import_bad_row_isolated() {
        let pool = setup().await;
        let cik = seed_company(&pool).await;
        let periods = seed_annual_periods(&pool, &cik, &[2024]).await;
        let repo = SqliteCustomMetricRepo::new(pool);
        let csv = "metric,period,value,unit\ntpv,FY2024,x,currency\nheadcount,FY2024,100,count\n";
        let report = import_csv(&cik, csv, &ImportOptions::default(), &repo, &periods).await.unwrap();
        assert_eq!(report.rejected.len(), 1);
        assert_eq!(report.rejected[0].reason, "bad_value");
        assert_eq!(report.imported, 1);
    }

    #[tokio::test]
    async fn import_unmatched_period_retained() {
        let pool = setup().await;
        let cik = seed_company(&pool).await;
        let periods = seed_annual_periods(&pool, &cik, &[2024]).await;
        let repo = SqliteCustomMetricRepo::new(pool);
        let csv = "metric,period,value,unit\ntpv,2019-06-01,100,currency\n";
        let report = import_csv(&cik, csv, &ImportOptions::default(), &repo, &periods).await.unwrap();
        assert_eq!(report.unaligned, 1);
        let m = repo.get_by_key(&cik, "tpv").await.unwrap().unwrap();
        let unaligned = repo.points_unaligned(&cik).await.unwrap();
        assert_eq!(unaligned.len(), 1);
        assert_eq!(unaligned[0].custom_metric_id, m.id);
        assert_eq!(unaligned[0].observed_date, Some("2019-06-01".to_string()));
    }

    #[tokio::test]
    async fn import_unit_conflict() {
        let pool = setup().await;
        let cik = seed_company(&pool).await;
        let periods = seed_annual_periods(&pool, &cik, &[2023, 2024]).await;
        let repo = SqliteCustomMetricRepo::new(pool);
        let csv1 = "metric,period,value,unit\ntpv,FY2023,100,currency\n";
        import_csv(&cik, csv1, &ImportOptions::default(), &repo, &periods).await.unwrap();
        let csv2 = "metric,period,value,unit\ntpv,FY2024,200,count\n";
        let report = import_csv(&cik, csv2, &ImportOptions::default(), &repo, &periods).await.unwrap();
        assert_eq!(report.rejected.len(), 1);
        assert_eq!(report.rejected[0].reason, "unit_conflict");
    }

    #[tokio::test]
    async fn import_never_aborts_file() {
        let pool = setup().await;
        let cik = seed_company(&pool).await;
        let periods = seed_annual_periods(&pool, &cik, &[2024]).await;
        let repo = SqliteCustomMetricRepo::new(pool);
        let csv = "metric,period,value,unit\ntpv,FY2024,100,currency\nbad,FY2024,notanumber,count\nheadcount,FY2024,50,count\n";
        let report = import_csv(&cik, csv, &ImportOptions::default(), &repo, &periods).await.unwrap();
        assert_eq!(report.imported, 2);
        assert_eq!(report.rejected.len(), 1);
        assert!(repo.get_by_key(&cik, "tpv").await.unwrap().is_some());
        assert!(repo.get_by_key(&cik, "headcount").await.unwrap().is_some());
    }
}
