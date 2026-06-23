// Mirrors src-tauri/src/domain Rust types. Kept in sync by hand for V1.

export type Cik = string;
export type Ticker = string;
export type AccessionNo = string;
export type Micro = number; // i64 fits in JS number for values up to 2^53; we store ×1e6 — safe for >$9 trillion.

// Mirror of Rust FormType (filing.rs). The wire format is always a flat
// string; non-canonical SEC form types (e.g., "POS AM") flow through as
// the literal SEC string rather than a tagged object.
export type FormType = "10-K" | "10-Q" | "10-K/A" | "10-Q/A" | "8-K" | string;

export interface Company {
  cik: Cik;
  ticker: Ticker;
  name: string;
  exchange?: string | null;
  sic?: string | null;
  fiscal_year_end?: string | null;
  added_at: string;
  last_refreshed?: string | null;
}

export type PeriodKind = "annual" | "quarterly";

export interface Period {
  id: number;
  cik: Cik;
  fiscal_year: number;
  fiscal_quarter: number;
  fiscal_year_end: string;
  start_date: string;
  end_date: string;
  kind: PeriodKind;
  is_53_week: boolean;
}

// Backend serializes via Metric::as_str() — these are the canonical
// snake_case identifiers used over IPC.
export type Metric =
  | "revenue" | "cost_of_revenue" | "gross_profit"
  | "operating_income" | "net_income"
  | "eps_basic" | "eps_diluted"
  | "shares_outstanding_basic" | "shares_outstanding_diluted"
  | "cash_and_equivalents" | "long_term_debt" | "current_debt" | "total_debt"
  | "total_assets" | "total_liabilities" | "total_equity"
  | "cash_from_operations" | "capital_expenditures" | "depreciation_amortization"
  | "free_cash_flow" | "operating_margin"
  | "free_cash_flow_ttm" | "free_cash_flow_yield"
  | "historical_market_cap" | "current_market_cap"
  | "current_free_cash_flow_yield";

/** Display-label overrides where title-casing the snake_case id reads poorly. */
const METRIC_LABELS: Record<string, string> = {
  free_cash_flow_ttm: "Free Cash Flow (TTM)",
  free_cash_flow_yield: "Free Cash Flow Yield",
  historical_market_cap: "Market Cap",
  current_market_cap: "Current Market Cap",
  current_free_cash_flow_yield: "Current Free Cash Flow Yield",
};

/** Pretty display label for a metric (e.g. "Net Income"). */
export function prettyMetric(m: string): string {
  return (
    METRIC_LABELS[m] ??
    m
      .split("_")
      .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
      .join(" ")
  );
}

export interface AppError {
  kind: string;
  detail: { code: string; message: string };
}

export type Result<T> = { ok: true; value: T } | { ok: false; error: AppError };

export interface IngestionSummary {
  cik: string;
  ticker: string;
  name: string;
  filings_ingested: number;
  raw_facts_ingested: number;
  normalized_facts_ingested: number;
  events_recorded: number;
}

export interface AddCompanyResponse {
  company: Company;
  summary: IngestionSummary;
}

export interface DashboardWidget {
  metric: string;
  period_label: string;
  value_micro: number;
  history: [string, number][];
}

export interface DashboardPayload {
  company: Company;
  widgets: DashboardWidget[];
}

export interface IngestionEvent {
  id: number;
  cik?: Cik | null;
  accession_no?: AccessionNo | null;
  stage: string;
  level: "info" | "warn" | "error";
  user_visible: boolean;
  message: string;
  detail_json?: string | null;
  occurred_at: string;
}

export interface RawFact {
  id: number;
  cik: Cik;
  accession_no: AccessionNo;
  taxonomy: string;
  concept: string;
  unit: string;
  value_numeric: number;
  period_start?: string | null;
  period_end: string;
  is_instant: boolean;
  fy?: number | null;
  fp?: string | null;
  filed?: string | null;
  source_kind: "xbrl_api" | "xbrl_xml";
  ingested_at: string;
}

export interface Filing {
  accession_no: AccessionNo;
  cik: Cik;
  form_type: FormType;
  filed_at: string;
  period_of_report?: string | null;
  is_amendment: boolean;
  amends?: AccessionNo | null;
  item_4_02_8k: boolean;
}

export interface NormalizedFact {
  id: number;
  cik: Cik;
  metric: Metric;
  period_id: number;
  value: number;
  unit: string;
  source_fact_id: number;
  source_kind: "xbrl_api" | "xbrl_xml";
  is_primary: boolean;
  original_value?: number | null;
  original_unit?: string | null;
  fx_rate_micro?: number | null;
  fx_rate_source?: string | null;
  fx_rate_date?: string | null;
  superseded_by?: number | null;
}

export interface LineagePayload {
  primary: NormalizedFact;
  raw_fact: RawFact;
  filing: Filing;
  supersession_chain: NormalizedFact[];
}

export interface MetricSeriesPoint {
  period: Period;
  value: number;
  source_kind: string;
  normalized_fact_id: number;
}

/**
 * Metrics whose stored value is a dimensionless decimal ratio (×1e6 per
 * architecture §6.2) rather than a USD amount, and so must render as a
 * percentage rather than currency.
 */
export const RATIO_METRICS: ReadonlySet<string> = new Set([
  "operating_margin",
  "free_cash_flow_yield",
  "current_free_cash_flow_yield",
]);

export function isRatioMetric(metric: string): boolean {
  return RATIO_METRICS.has(metric);
}

/** Convert a stored micro-unit value to USD dollars (with sign preserved). */
export function microToUsd(micro: number): number { return micro / 1_000_000; }

/**
 * Convert a stored ratio micro-value (ratio × 1e6) to a percentage number.
 * e.g. 253_000 → 25.3.
 */
export function microToPercent(micro: number): number { return micro / 10_000; }

/** Format a ratio micro-value as a percentage ("35.5%"). */
export function fmtPercent(micro: number): string {
  return `${microToPercent(micro).toFixed(1)}%`;
}

/**
 * Format a metric value for display, dispatching on the metric's unit:
 * ratio metrics render as a percentage, everything else as compact USD.
 */
export function fmtMetricValue(metric: string, micro: number): string {
  return isRatioMetric(metric) ? fmtPercent(micro) : fmtUsdCompact(micro);
}

/** Format a USD micro-unit value as compact ("$3.83B"). */
export function fmtUsdCompact(micro: number): string {
  const v = microToUsd(micro);
  const abs = Math.abs(v);
  const fmt = (n: number, suffix: string) =>
    `${v < 0 ? "-" : ""}$${n.toFixed(n >= 100 ? 0 : 2)}${suffix}`;
  if (abs >= 1e12) return fmt(abs / 1e12, "T");
  if (abs >= 1e9) return fmt(abs / 1e9, "B");
  if (abs >= 1e6) return fmt(abs / 1e6, "M");
  if (abs >= 1e3) return fmt(abs / 1e3, "K");
  return `${v < 0 ? "-" : ""}$${abs.toFixed(2)}`;
}

export interface CurrentValuation {
  price_micro: number;
  price_as_of: string;
  shares: number;
  shares_period_end: string;
  market_cap_micro: number;
  ttm_fcf_micro: number;
  ttm_fcf_period_end: string;
  fcf_yield_micro: number;
}
