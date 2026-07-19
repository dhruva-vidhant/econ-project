//! CSV renderers for built-in and custom metric series. See techspec §7.2/§7.3.
//!
//! `render_csv` is pure with respect to the filesystem — it only reads
//! through the repository/`ReadCtx` layer already used by the IPC series
//! commands — so its output is unit-testable without touching disk.
//! `export_csv` is `render_csv` plus a single file write.

use serde::{Deserialize, Serialize};

use crate::derived::custom_series;
use crate::derived::series::{self, ReadCtx};
use crate::domain::{
    micro_to_decimal_string, ratio_micro_to_percent_string, Cik, Metric, Period, PeriodKind,
    UnitKind,
};
use crate::errors::AppError;
use crate::repos::custom_metric::CustomMetricRepo;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExportRequest {
    MetricSeries { cik: String, metric: String, kind: String },
    CustomMetricSeries { cik: String, metric_id: i64, kind: String },
    Comparison { cik: String, series: Vec<SeriesRef>, kind: String },
    Statement { cik: String, statement: String, kind: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeriesRef {
    pub source: String, // "builtin" | "custom"
    pub key: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportReport {
    pub rows: u32,
    pub bytes: u64,
    pub path: String,
}

/// One row of formatted export data, unit-agnostic until rendered.
struct Row {
    period: Period,
    unit_label: &'static str,
    currency: Option<String>,
    value_str: String,
}

fn parse_kind(kind: &str) -> Result<PeriodKind, AppError> {
    PeriodKind::from_str(kind).ok_or_else(|| AppError::invalid(format!("unknown period kind: {kind}")))
}

/// Unit label + formatted value for a built-in metric's micro value, per
/// techspec §7.3: currency metrics carry a `currency` column ("USD"); ratio
/// metrics render as "percent"; counts render as exact integers.
fn format_builtin_value(metric: Metric, value: i64) -> (&'static str, Option<String>, String) {
    match metric {
        Metric::OperatingMargin | Metric::FreeCashFlowYield | Metric::CurrentFreeCashFlowYield => {
            ("percent", None, ratio_micro_to_percent_string(value))
        }
        Metric::SharesOutstandingBasic | Metric::SharesOutstandingDiluted => {
            ("count", None, micro_to_decimal_string(value, 0))
        }
        _ => ("USD", Some("USD".to_string()), micro_to_decimal_string(value, 2)),
    }
}

/// Formats a custom-metric value. `currency` is the metric's declared ISO code,
/// used verbatim for `Currency` metrics so exports carry the true currency
/// (e.g. "EUR") rather than a hard-coded "USD".
fn format_custom_value(
    unit_kind: UnitKind,
    currency: Option<&str>,
    decimals: u8,
    value: i64,
) -> (&'static str, Option<String>, String) {
    match unit_kind {
        UnitKind::Currency => {
            let code = currency.unwrap_or("USD").to_string();
            ("USD", Some(code), micro_to_decimal_string(value, 2))
        }
        UnitKind::Count => ("count", None, micro_to_decimal_string(value, 0)),
        UnitKind::Ratio => ("percent", None, ratio_micro_to_percent_string(value)),
        UnitKind::Number => ("number", None, micro_to_decimal_string(value, decimals)),
    }
}

fn period_label(p: &Period) -> String {
    match p.kind {
        PeriodKind::Annual => format!("FY{}", p.fiscal_year),
        PeriodKind::Quarterly => format!("FY{} Q{}", p.fiscal_year, p.fiscal_quarter),
    }
}

fn csv_escape(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

async fn builtin_rows(
    ctx: &ReadCtx<'_>,
    cik: &Cik,
    metric: Metric,
    kind: PeriodKind,
) -> Result<Vec<Row>, AppError> {
    let points = series::revenue_aware_series(ctx, cik, metric, kind).await?;
    Ok(points
        .into_iter()
        .map(|p| {
            let (unit_label, currency, value_str) = format_builtin_value(metric, p.value);
            Row { period: p.period, unit_label, currency, value_str }
        })
        .collect())
}

async fn custom_rows(
    custom_repo: &dyn CustomMetricRepo,
    metric_id: i64,
    kind: PeriodKind,
) -> Result<(Vec<Row>, String), AppError> {
    let metric = custom_repo
        .get(metric_id)
        .await?
        .ok_or_else(|| AppError::not_found("custom metric"))?;
    let points = custom_series::custom_metric_series(custom_repo, metric_id, kind).await?;
    let rows = points
        .into_iter()
        .map(|p| {
            let (unit_label, currency, value_str) = format_custom_value(
                p.unit_kind,
                metric.currency.as_deref(),
                metric.decimals,
                p.value_micro,
            );
            Row { period: p.period, unit_label, currency, value_str }
        })
        .collect();
    Ok((rows, metric.key))
}

fn render_series_csv(metric_key: &str, rows: &[Row]) -> String {
    let has_currency = rows.iter().any(|r| r.currency.is_some());
    let mut out = String::from("metric,unit,period_label,period_kind,fiscal_year,fiscal_quarter,period_start,period_end,value");
    if has_currency {
        out.push_str(",currency");
    }
    out.push('\n');
    for r in rows {
        out.push_str(&csv_escape(metric_key));
        out.push(',');
        out.push_str(r.unit_label);
        out.push(',');
        out.push_str(&period_label(&r.period));
        out.push(',');
        out.push_str(r.period.kind.as_str());
        out.push(',');
        out.push_str(&r.period.fiscal_year.to_string());
        out.push(',');
        out.push_str(&r.period.fiscal_quarter.to_string());
        out.push(',');
        out.push_str(&r.period.start_date.to_string());
        out.push(',');
        out.push_str(&r.period.end_date.to_string());
        out.push(',');
        out.push_str(&r.value_str);
        if has_currency {
            out.push(',');
            out.push_str(r.currency.as_deref().unwrap_or(""));
        }
        out.push('\n');
    }
    out
}

async fn resolve_series_ref(
    ctx: &ReadCtx<'_>,
    custom_repo: &dyn CustomMetricRepo,
    cik: &Cik,
    kind: PeriodKind,
    r: &SeriesRef,
) -> Result<(String, String, Vec<Row>), AppError> {
    match r.source.as_str() {
        "builtin" => {
            let metric = Metric::from_str(&r.key)
                .ok_or_else(|| AppError::invalid(format!("unknown metric: {}", r.key)))?;
            let rows = builtin_rows(ctx, cik, metric, kind).await?;
            let unit_label = rows.first().map(|r| r.unit_label).unwrap_or("USD");
            Ok((r.key.clone(), unit_label.to_string(), rows))
        }
        "custom" => {
            let metric = custom_repo
                .get_by_key(cik, &r.key)
                .await?
                .ok_or_else(|| AppError::not_found(format!("custom metric {}", r.key)))?;
            let (rows, key) = custom_rows(custom_repo, metric.id, kind).await?;
            let unit_label = rows.first().map(|r| r.unit_label).unwrap_or("number");
            Ok((key, unit_label.to_string(), rows))
        }
        other => Err(AppError::invalid(format!("unknown series source: {other}"))),
    }
}

fn render_comparison_csv(columns: &[(String, String, Vec<Row>)]) -> String {
    use std::collections::BTreeMap;

    let mut periods: BTreeMap<i64, Period> = BTreeMap::new();
    for (_, _, rows) in columns {
        for r in rows {
            periods.entry(r.period.id).or_insert_with(|| r.period.clone());
        }
    }
    let mut sorted_periods: Vec<&Period> = periods.values().collect();
    sorted_periods.sort_by(|a, b| a.end_date.cmp(&b.end_date));

    let mut header = String::from("period_label,period_kind,fiscal_year,fiscal_quarter,period_end");
    for (key, unit_label, _) in columns {
        header.push(',');
        header.push_str(&csv_escape(&format!("{key} [{unit_label}]")));
    }
    header.push('\n');

    let mut out = header;
    for p in sorted_periods {
        out.push_str(&period_label(p));
        out.push(',');
        out.push_str(p.kind.as_str());
        out.push(',');
        out.push_str(&p.fiscal_year.to_string());
        out.push(',');
        out.push_str(&p.fiscal_quarter.to_string());
        out.push(',');
        out.push_str(&p.end_date.to_string());
        for (_, _, rows) in columns {
            out.push(',');
            if let Some(r) = rows.iter().find(|r| r.period.id == p.id) {
                out.push_str(&r.value_str);
            }
        }
        out.push('\n');
    }
    out
}

const STATEMENT_INCOME: &[Metric] = &[
    Metric::Revenue,
    Metric::CostOfRevenue,
    Metric::GrossProfit,
    Metric::OperatingIncome,
    Metric::OperatingMargin,
    Metric::NetIncome,
    Metric::EpsBasic,
    Metric::EpsDiluted,
];
const STATEMENT_BALANCE: &[Metric] = &[
    Metric::TotalAssets,
    Metric::TotalLiabilities,
    Metric::TotalEquity,
    Metric::CashAndEquivalents,
    Metric::LongTermDebt,
    Metric::CurrentDebt,
    Metric::TotalDebt,
];
const STATEMENT_CASH_FLOW: &[Metric] = &[
    Metric::CashFromOperations,
    Metric::CapitalExpenditures,
    Metric::DepreciationAmortization,
    Metric::FreeCashFlow,
];

fn statement_metrics(statement: &str) -> Result<&'static [Metric], AppError> {
    match statement {
        "income" => Ok(STATEMENT_INCOME),
        "balance" => Ok(STATEMENT_BALANCE),
        "cash_flow" => Ok(STATEMENT_CASH_FLOW),
        other => Err(AppError::invalid(format!("unknown statement: {other}"))),
    }
}

async fn render_statement_csv(
    ctx: &ReadCtx<'_>,
    cik: &Cik,
    statement: &str,
    kind: PeriodKind,
) -> Result<String, AppError> {
    let metrics = statement_metrics(statement)?;

    let mut per_metric_rows: Vec<Vec<Row>> = Vec::with_capacity(metrics.len());
    let mut labels: Vec<String> = Vec::new();
    let mut seen_labels = std::collections::HashSet::new();
    for &metric in metrics {
        let rows = builtin_rows(ctx, cik, metric, kind.clone()).await?;
        for r in &rows {
            let label = period_label(&r.period);
            if seen_labels.insert(label.clone()) {
                labels.push(label);
            }
        }
        per_metric_rows.push(rows);
    }
    // Column order follows first-seen period (already end_date-sorted per metric,
    // but merge sort-stable across metrics by end_date for determinism).
    let mut label_order: Vec<(String, chrono::NaiveDate)> = Vec::new();
    for rows in &per_metric_rows {
        for r in rows {
            let label = period_label(&r.period);
            if !label_order.iter().any(|(l, _)| l == &label) {
                label_order.push((label, r.period.end_date));
            }
        }
    }
    label_order.sort_by_key(|(_, d)| *d);

    let mut out = String::from("metric,unit");
    for (label, _) in &label_order {
        out.push(',');
        out.push_str(label);
    }
    out.push('\n');

    for (metric, rows) in metrics.iter().zip(per_metric_rows.iter()) {
        out.push_str(metric.as_str());
        out.push(',');
        let unit_label = rows.first().map(|r| r.unit_label).unwrap_or("USD");
        out.push_str(unit_label);
        for (label, _) in &label_order {
            out.push(',');
            if let Some(r) = rows.iter().find(|r| period_label(&r.period) == *label) {
                out.push_str(&r.value_str);
            }
        }
        out.push('\n');
    }
    Ok(out)
}

/// Renders CSV text for an [`ExportRequest`]. Pure with respect to the
/// filesystem: all data comes from `ctx`/`custom_repo`, the same read path the
/// series/dashboard IPC commands already use.
pub async fn render_csv(
    request: &ExportRequest,
    ctx: &ReadCtx<'_>,
    custom_repo: &dyn CustomMetricRepo,
) -> Result<String, AppError> {
    match request {
        ExportRequest::MetricSeries { cik, metric, kind } => {
            let cik = Cik::from_any(cik).map_err(AppError::invalid)?;
            let metric = Metric::from_str(metric)
                .ok_or_else(|| AppError::invalid(format!("unknown metric: {metric}")))?;
            let kind = parse_kind(kind)?;
            let rows = builtin_rows(ctx, &cik, metric, kind).await?;
            Ok(render_series_csv(metric.as_str(), &rows))
        }
        ExportRequest::CustomMetricSeries { metric_id, kind, .. } => {
            let kind = parse_kind(kind)?;
            let (rows, key) = custom_rows(custom_repo, *metric_id, kind).await?;
            Ok(render_series_csv(&key, &rows))
        }
        ExportRequest::Comparison { cik, series, kind } => {
            let cik = Cik::from_any(cik).map_err(AppError::invalid)?;
            let kind = parse_kind(kind)?;
            let mut columns = Vec::with_capacity(series.len());
            for r in series {
                columns.push(resolve_series_ref(ctx, custom_repo, &cik, kind.clone(), r).await?);
            }
            Ok(render_comparison_csv(&columns))
        }
        ExportRequest::Statement { cik, statement, kind } => {
            let cik = Cik::from_any(cik).map_err(AppError::invalid)?;
            let kind = parse_kind(kind)?;
            render_statement_csv(ctx, &cik, statement, kind).await
        }
    }
}

/// `render_csv` plus a write to `dest_path`.
pub async fn export_csv(
    request: ExportRequest,
    dest_path: &str,
    ctx: &ReadCtx<'_>,
    custom_repo: &dyn CustomMetricRepo,
) -> Result<ExportReport, AppError> {
    let text = render_csv(&request, ctx, custom_repo).await?;
    let rows = text.lines().count().saturating_sub(1) as u32;
    let bytes = text.len() as u64;
    std::fs::write(dest_path, &text).map_err(|e| AppError::internal(e.to_string()))?;
    Ok(ExportReport { rows, bytes, path: dest_path.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Pool;
    use crate::domain::{Company, NewCustomMetric, Ticker};
    use crate::repos::company::{CompanyRepo, SqliteCompanyRepo};
    use crate::repos::current_price::SqliteCurrentPriceRepo;
    use crate::repos::custom_metric::SqliteCustomMetricRepo;
    use crate::repos::derived_metric::SqliteDerivedMetricRepo;
    use crate::repos::historical_price::SqliteHistoricalPriceRepo;
    use crate::repos::normalized_fact::{NormalizedFactRepo, SqliteNormalizedFactRepo};
    use crate::repos::period::{PeriodRepo, SqlitePeriodRepo};
    use chrono::{NaiveDate, TimeZone, Utc};
    use std::sync::Arc;

    async fn setup() -> Arc<Pool> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.sqlite");
        Box::leak(Box::new(dir));
        Arc::new(Pool::open(&path).unwrap())
    }

    async fn seed_company(pool: &Arc<Pool>) -> Cik {
        let companies = SqliteCompanyRepo::new(pool.clone());
        let cik = Cik("0000000001".into());
        companies
            .upsert(&Company {
                cik: cik.clone(),
                ticker: Ticker("TEST".into()),
                name: "Test Co".into(),
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

    async fn seed_period(pool: &Arc<Pool>, cik: &Cik, fy: i32) -> i64 {
        let periods = SqlitePeriodRepo::new(pool.clone());
        periods
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
            .unwrap()
    }

    async fn insert_normalized(
        pool: &Arc<Pool>,
        cik: &Cik,
        accession: &str,
        metric: Metric,
        period_id: i64,
        value: i64,
    ) {
        // Minimal filing + raw_fact + normalized_fact chain, matching the shape
        // used elsewhere in the repo's own tests.
        use crate::domain::{AccessionNo, Filing, FormType, NormalizedFact, RawFact, SourceKind};
        use crate::repos::filing::{FilingRepo, SqliteFilingRepo};
        use crate::repos::raw_fact::{RawFactRepo, SqliteRawFactRepo};

        let accn = AccessionNo(accession.into());
        let filings = SqliteFilingRepo::new(pool.clone());
        filings
            .upsert(&Filing {
                accession_no: accn.clone(),
                cik: cik.clone(),
                form_type: FormType::TenK,
                filed_at: NaiveDate::from_ymd_opt(2024, 11, 1).unwrap(),
                period_of_report: None,
                is_amendment: false,
                amends: None,
                item_4_02_8k: false,
            })
            .await
            .unwrap();

        let raw_facts = SqliteRawFactRepo::new(pool.clone());
        raw_facts
            .upsert_many(&[RawFact {
                id: 0,
                cik: cik.clone(),
                accession_no: accn.clone(),
                taxonomy: "us-gaap".into(),
                concept: metric.as_str().into(),
                unit: "USD".into(),
                value_numeric: value,
                period_start: None,
                period_end: NaiveDate::from_ymd_opt(2024, 12, 31).unwrap(),
                is_instant: metric.is_instant(),
                fy: Some(2024),
                fp: Some("FY".into()),
                filed: Some(NaiveDate::from_ymd_opt(2024, 11, 1).unwrap()),
                source_kind: SourceKind::XbrlApi,
                ingested_at: Utc::now(),
            }])
            .await
            .unwrap();
        let raw_id: i64 = {
            let g = pool.read().unwrap();
            g.conn()
                .query_row(
                    "SELECT id FROM raw_fact WHERE accession_no = ?1 AND concept = ?2",
                    rusqlite::params![accn.0, metric.as_str()],
                    |r| r.get(0),
                )
                .unwrap()
        };

        let nf = SqliteNormalizedFactRepo::new(pool.clone());
        nf.insert_primary_with_supersession(&NormalizedFact {
            id: 0,
            cik: cik.clone(),
            metric,
            period_id,
            value,
            unit: "USD".into(),
            source_fact_id: raw_id,
            source_kind: SourceKind::XbrlApi,
            is_primary: true,
            original_value: None,
            original_unit: None,
            fx_rate_micro: None,
            fx_rate_source: None,
            fx_rate_date: None,
            superseded_by: None,
        })
        .await
        .unwrap();
    }

    struct TestRepos {
        nf: SqliteNormalizedFactRepo,
        dm: SqliteDerivedMetricRepo,
        prices: SqliteHistoricalPriceRepo,
        current_prices: SqliteCurrentPriceRepo,
        custom: SqliteCustomMetricRepo,
    }

    async fn setup_repos() -> (Arc<Pool>, TestRepos, Cik) {
        let pool = setup().await;
        let cik = seed_company(&pool).await;
        let repos = TestRepos {
            nf: SqliteNormalizedFactRepo::new(pool.clone()),
            dm: SqliteDerivedMetricRepo::new(pool.clone()),
            prices: SqliteHistoricalPriceRepo::new(pool.clone()),
            current_prices: SqliteCurrentPriceRepo::new(pool.clone()),
            custom: SqliteCustomMetricRepo::new(pool.clone()),
        };
        (pool, repos, cik)
    }

    fn ctx(repos: &TestRepos) -> ReadCtx<'_> {
        ReadCtx {
            normalized_facts: &repos.nf,
            derived_metrics: &repos.dm,
            prices: &repos.prices,
            current_prices: &repos.current_prices,
        }
    }

    #[tokio::test]
    async fn custom_series_excludes_unaligned() {
        let (pool, repos, cik) = setup_repos().await;
        let period_id = seed_period(&pool, &cik, 2024).await;
        let m = repos
            .custom
            .create(
                &cik,
                &NewCustomMetric {
                    key: "tpv".into(),
                    label: "TPV".into(),
                    unit_kind: UnitKind::Currency,
                    currency: Some("USD".into()),
                    decimals: Some(2),
                    description: None,
                },
            )
            .await
            .unwrap();
        repos.custom.upsert_point(m.id, Some(period_id), None, 100_000_000, None).await.unwrap();
        repos.custom.upsert_point(m.id, None, Some("2019-01-01"), 1, None).await.unwrap();
        let series = custom_series::custom_metric_series(&repos.custom, m.id, PeriodKind::Annual)
            .await
            .unwrap();
        assert_eq!(series.len(), 1);
    }

    #[tokio::test]
    async fn custom_series_kind_filter() {
        let (pool, repos, cik) = setup_repos().await;
        let annual_id = seed_period(&pool, &cik, 2024).await;
        let quarterly_id = {
            let periods = SqlitePeriodRepo::new(pool.clone());
            periods
                .upsert_returning_id(&Period {
                    id: 0,
                    cik: cik.clone(),
                    fiscal_year: 2024,
                    fiscal_quarter: 1,
                    fiscal_year_end: "1231".into(),
                    start_date: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
                    end_date: NaiveDate::from_ymd_opt(2024, 3, 31).unwrap(),
                    kind: PeriodKind::Quarterly,
                    is_53_week: false,
                })
                .await
                .unwrap()
        };
        let m = repos
            .custom
            .create(
                &cik,
                &NewCustomMetric {
                    key: "tpv".into(),
                    label: "TPV".into(),
                    unit_kind: UnitKind::Currency,
                    currency: Some("USD".into()),
                    decimals: Some(2),
                    description: None,
                },
            )
            .await
            .unwrap();
        repos.custom.upsert_point(m.id, Some(annual_id), None, 1, None).await.unwrap();
        repos.custom.upsert_point(m.id, Some(quarterly_id), None, 2, None).await.unwrap();
        let annual = custom_series::custom_metric_series(&repos.custom, m.id, PeriodKind::Annual)
            .await
            .unwrap();
        let quarterly = custom_series::custom_metric_series(&repos.custom, m.id, PeriodKind::Quarterly)
            .await
            .unwrap();
        assert_eq!(annual.len(), 1);
        assert_eq!(quarterly.len(), 1);
        assert_ne!(annual[0].value_micro, quarterly[0].value_micro);
    }

    #[tokio::test]
    async fn render_series_currency_exact() {
        let (pool, repos, cik) = setup_repos().await;
        let period_id = seed_period(&pool, &cik, 2024).await;
        insert_normalized(&pool, &cik, "acc-1", Metric::Revenue, period_id, 383_285_000_000_000_000).await;
        let request = ExportRequest::MetricSeries {
            cik: cik.0.clone(),
            metric: "revenue".into(),
            kind: "annual".into(),
        };
        let text = render_csv(&request, &ctx(&repos), &repos.custom).await.unwrap();
        assert!(text.starts_with("metric,unit,period_label,period_kind,fiscal_year,fiscal_quarter,period_start,period_end,value,currency\n"));
        let line = text.lines().nth(1).unwrap();
        assert!(line.contains("383285000000.00"));
        assert!(line.ends_with(",USD"));
    }

    #[tokio::test]
    async fn render_series_ratio_percent() {
        let (pool, repos, cik) = setup_repos().await;
        let period_id = seed_period(&pool, &cik, 2024).await;
        insert_normalized(&pool, &cik, "acc-1", Metric::OperatingIncome, period_id, 100_000_000).await;
        insert_normalized(&pool, &cik, "acc-2", Metric::Revenue, period_id, 1_000_000_000).await;
        let request = ExportRequest::MetricSeries {
            cik: cik.0.clone(),
            metric: "operating_margin".into(),
            kind: "annual".into(),
        };
        let text = render_csv(&request, &ctx(&repos), &repos.custom).await.unwrap();
        assert!(!text.contains(",currency"), "ratio series should not have a currency column");
        let line = text.lines().nth(1).unwrap();
        assert!(line.contains(",percent,"));
        assert!(line.ends_with(",10"));
    }

    #[tokio::test]
    async fn render_comparison_aligns_periods() {
        let (pool, repos, cik) = setup_repos().await;
        let p2023 = seed_period(&pool, &cik, 2023).await;
        let p2024 = seed_period(&pool, &cik, 2024).await;
        insert_normalized(&pool, &cik, "acc-1", Metric::Revenue, p2023, 1_000_000_000_000).await;
        insert_normalized(&pool, &cik, "acc-2", Metric::Revenue, p2024, 2_000_000_000_000).await;

        let m = repos
            .custom
            .create(
                &cik,
                &NewCustomMetric {
                    key: "tpv".into(),
                    label: "TPV".into(),
                    unit_kind: UnitKind::Currency,
                    currency: Some("USD".into()),
                    decimals: Some(2),
                    description: None,
                },
            )
            .await
            .unwrap();
        repos.custom.upsert_point(m.id, Some(p2024), None, 5_000_000_000, None).await.unwrap();

        let request = ExportRequest::Comparison {
            cik: cik.0.clone(),
            series: vec![
                SeriesRef { source: "builtin".into(), key: "revenue".into() },
                SeriesRef { source: "custom".into(), key: "tpv".into() },
            ],
            kind: "annual".into(),
        };
        let text = render_csv(&request, &ctx(&repos), &repos.custom).await.unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3); // header + FY2023 + FY2024
        assert!(lines[0].contains("revenue [USD]"));
        assert!(lines[0].contains("tpv [USD]"));
        // FY2023 row has empty tpv cell (trailing comma, nothing after).
        let fy2023 = lines.iter().find(|l| l.starts_with("FY2023")).unwrap();
        assert!(fy2023.ends_with(','));
        let fy2024 = lines.iter().find(|l| l.starts_with("FY2024")).unwrap();
        assert!(fy2024.ends_with("5000.00"));
    }

    #[tokio::test]
    async fn render_statement_matrix() {
        let (pool, repos, cik) = setup_repos().await;
        let period_id = seed_period(&pool, &cik, 2024).await;
        insert_normalized(&pool, &cik, "acc-1", Metric::Revenue, period_id, 1_000_000_000_000).await;
        insert_normalized(&pool, &cik, "acc-2", Metric::NetIncome, period_id, 200_000_000_000).await;
        let request = ExportRequest::Statement {
            cik: cik.0.clone(),
            statement: "income".into(),
            kind: "annual".into(),
        };
        let text = render_csv(&request, &ctx(&repos), &repos.custom).await.unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "metric,unit,FY2024");
        assert!(lines.iter().any(|l| l.starts_with("revenue,USD,1000000.00")));
        assert!(lines.iter().any(|l| l.starts_with("net_income,USD,200000.00")));
    }

    #[tokio::test]
    async fn render_precision_large_count() {
        let (pool, repos, cik) = setup_repos().await;
        let period_id = seed_period(&pool, &cik, 2024).await;
        // 25.1e9 shares, stored as micro-units: 25_100_000_000 * 1_000_000.
        insert_normalized(&pool, &cik, "acc-1", Metric::SharesOutstandingBasic, period_id, 25_100_000_000 * 1_000_000).await;
        let request = ExportRequest::MetricSeries {
            cik: cik.0.clone(),
            metric: "shares_outstanding_basic".into(),
            kind: "annual".into(),
        };
        let text = render_csv(&request, &ctx(&repos), &repos.custom).await.unwrap();
        let line = text.lines().nth(1).unwrap();
        assert!(line.ends_with(",25100000000"), "line was: {line}");
    }

    /// A custom currency metric declared in EUR must export its true ISO code,
    /// not a hard-coded "USD". Regression for the export currency bug.
    #[tokio::test]
    async fn render_custom_series_honours_currency() {
        let (pool, repos, cik) = setup_repos().await;
        let period_id = seed_period(&pool, &cik, 2024).await;
        let m = repos
            .custom
            .create(
                &cik,
                &NewCustomMetric {
                    key: "eu_revenue".into(),
                    label: "EU Revenue".into(),
                    unit_kind: UnitKind::Currency,
                    currency: Some("EUR".into()),
                    decimals: Some(2),
                    description: None,
                },
            )
            .await
            .unwrap();
        repos.custom.upsert_point(m.id, Some(period_id), None, 12_340_000_000, None).await.unwrap();
        let request = ExportRequest::CustomMetricSeries {
            cik: cik.0.clone(),
            metric_id: m.id,
            kind: "annual".into(),
        };
        let text = render_csv(&request, &ctx(&repos), &repos.custom).await.unwrap();
        assert!(text.contains(",currency"), "currency metric must carry a currency column");
        let line = text.lines().nth(1).unwrap();
        assert!(line.ends_with(",EUR"), "expected EUR currency code, line was: {line}");
        assert!(line.contains("12340.00"), "line was: {line}");
    }

    /// §12.6 `export_writes_file`: `export_csv` must write a file whose bytes
    /// equal `render_csv`, and report the correct row count.
    #[tokio::test]
    async fn export_writes_file() {
        let (pool, repos, cik) = setup_repos().await;
        let p2023 = seed_period(&pool, &cik, 2023).await;
        let p2024 = seed_period(&pool, &cik, 2024).await;
        insert_normalized(&pool, &cik, "acc-1", Metric::Revenue, p2023, 1_000_000_000_000).await;
        insert_normalized(&pool, &cik, "acc-2", Metric::Revenue, p2024, 2_000_000_000_000).await;
        let request = ExportRequest::MetricSeries {
            cik: cik.0.clone(),
            metric: "revenue".into(),
            kind: "annual".into(),
        };
        let expected = render_csv(&request, &ctx(&repos), &repos.custom).await.unwrap();

        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("revenue.csv");
        let dest_str = dest.to_str().unwrap();
        let report = export_csv(request, dest_str, &ctx(&repos), &repos.custom).await.unwrap();

        let on_disk = std::fs::read_to_string(&dest).unwrap();
        assert_eq!(on_disk, expected, "file content must equal render_csv output");
        assert_eq!(report.rows, 2, "two data rows (FY2023, FY2024)");
        assert_eq!(report.bytes, expected.len() as u64);
        assert_eq!(report.path, dest_str);
    }

    /// §12.6 `export_builtin_matches_series`: the exported built-in series must
    /// have one row per period in the underlying series and carry the exact
    /// micro value (rendered to 2dp) for each.
    #[tokio::test]
    async fn export_builtin_matches_series() {
        let (pool, repos, cik) = setup_repos().await;
        let p2023 = seed_period(&pool, &cik, 2023).await;
        let p2024 = seed_period(&pool, &cik, 2024).await;
        insert_normalized(&pool, &cik, "acc-1", Metric::Revenue, p2023, 1_500_000_000_000).await;
        insert_normalized(&pool, &cik, "acc-2", Metric::Revenue, p2024, 2_500_000_000_000).await;

        let series =
            series::revenue_aware_series(&ctx(&repos), &cik, Metric::Revenue, PeriodKind::Annual)
                .await
                .unwrap();
        let request = ExportRequest::MetricSeries {
            cik: cik.0.clone(),
            metric: "revenue".into(),
            kind: "annual".into(),
        };
        let text = render_csv(&request, &ctx(&repos), &repos.custom).await.unwrap();
        let data_rows: Vec<&str> = text.lines().skip(1).collect();
        assert_eq!(data_rows.len(), series.len(), "one CSV row per series point");
        for (row, point) in data_rows.iter().zip(series.iter()) {
            let expected_value = micro_to_decimal_string(point.value, 2);
            assert!(
                row.contains(&expected_value),
                "row {row} should contain exact value {expected_value}"
            );
        }
    }
}
