// Mirrors src-tauri/src/domain Rust types. Kept in sync by hand for V1.

export type Cik = string;
export type Ticker = string;
export type AccessionNo = string;
export type Micro = number; // i64 fits in JS number for values up to 2^53; we store ×1e6 — safe for >$9 trillion.

export type FormType = "10-K" | "10-Q" | "10-K/A" | "10-Q/A" | "8-K" | "other";

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

export type Metric =
  | "Revenue" | "CostOfRevenue" | "GrossProfit"
  | "OperatingIncome" | "NetIncome"
  | "EpsBasic" | "EpsDiluted"
  | "SharesOutstandingBasic" | "SharesOutstandingDiluted"
  | "CashAndEquivalents" | "LongTermDebt" | "CurrentDebt" | "TotalDebt"
  | "TotalAssets" | "TotalLiabilities" | "TotalEquity"
  | "CashFromOperations" | "CapitalExpenditures" | "DepreciationAmortization"
  | "HistoricalMarketCap" | "CurrentMarketCap";

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

/** Convert a stored micro-unit value to USD dollars (with sign preserved). */
export function microToUsd(micro: number): number { return micro / 1_000_000; }

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
