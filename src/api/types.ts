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
