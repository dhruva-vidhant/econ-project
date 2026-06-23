/**
 * Tauri IPC mock for E2E tests. Activated via `VITE_E2E=1` env var.
 *
 * Mirrors the surface the Rust backend exposes (M29) so the React UI
 * can be exercised end-to-end with Playwright without a live Tauri
 * runtime. The state is in-memory; tests can preload it via the
 * `__econMock` window globals exposed below.
 */
import { mockIPC } from "@tauri-apps/api/mocks";

import type {
  AddCompanyResponse,
  Company,
  CurrentValuation,
  DashboardPayload,
  IngestionEvent,
  LineagePayload,
  MetricSeriesPoint,
  Period,
  Filing,
  RawFact,
  NormalizedFact,
} from "@/api/types";

// ───────── In-memory state ─────────
type State = {
  companies: Company[];
  events: Record<string, IngestionEvent[]>;
  history: Record<string, MetricSeriesPoint[]>; // keyed by `${cik}:${metric}:${kind}`
  dashboards: Record<string, DashboardPayload>;
  lineage: Record<number, LineagePayload>;
  currentValuations: Record<string, CurrentValuation | null>;
  failAddTickers: Set<string>;
};

const state: State = {
  companies: [],
  events: {},
  history: {},
  dashboards: {},
  lineage: {},
  currentValuations: {},
  failAddTickers: new Set(["XYZNOPE", "BADTKR"]),
};

declare global {
  interface Window {
    __econMock: {
      reset: () => void;
      seedAaplFixture: () => void;
      state: State;
    };
  }
}

window.__econMock = {
  state,
  reset() {
    state.companies = [];
    state.events = {};
    state.history = {};
    state.dashboards = {};
    state.lineage = {};
    state.currentValuations = {};
  },
  seedAaplFixture() {
    seedAapl();
  },
};

// ───────── Fixture data ─────────

function makePeriod(fy: number, fq: number, kind: "annual" | "quarterly"): Period {
  return {
    id: fy * 10 + fq,
    cik: "0000320193",
    fiscal_year: fy,
    fiscal_quarter: fq,
    fiscal_year_end: "0926",
    start_date: kind === "annual" ? `${fy - 1}-09-30` : `${fy}-${String(fq * 3 - 2).padStart(2, "0")}-01`,
    end_date: kind === "annual" ? `${fy}-09-30` : `${fy}-${String(fq * 3).padStart(2, "0")}-30`,
    kind,
    is_53_week: false,
  };
}

const APPLE_REVENUE_ANNUAL: number[] = [
  // FY2014 .. FY2024 (representative, in micro-units = USD × 1e6)
  182795_000_000_000_000, 233715_000_000_000_000, 215639_000_000_000_000,
  229234_000_000_000_000, 265595_000_000_000_000, 260174_000_000_000_000,
  274515_000_000_000_000, 365817_000_000_000_000, 394328_000_000_000_000,
  383285_000_000_000_000, 391035_000_000_000_000,
];
const APPLE_NETINCOME_ANNUAL: number[] = [
  39510_000_000_000_000, 53394_000_000_000_000, 45687_000_000_000_000,
  48351_000_000_000_000, 59531_000_000_000_000, 55256_000_000_000_000,
  57411_000_000_000_000, 94680_000_000_000_000, 99803_000_000_000_000,
  96995_000_000_000_000, 93736_000_000_000_000,
];
const APPLE_TOTAL_ASSETS_ANNUAL: number[] = [
  231839_000_000_000_000, 290479_000_000_000_000, 321686_000_000_000_000,
  375319_000_000_000_000, 365725_000_000_000_000, 338516_000_000_000_000,
  323888_000_000_000_000, 351002_000_000_000_000, 352755_000_000_000_000,
  352583_000_000_000_000, 364980_000_000_000_000,
];

function seriesFor(metric: string, vals: number[], kind: "annual" | "quarterly"): MetricSeriesPoint[] {
  return vals.map((v, i) => {
    const fy = 2014 + i;
    const period = makePeriod(fy, kind === "annual" ? 0 : 4, kind);
    return {
      period,
      value: v,
      source_kind: "xbrl_api",
      normalized_fact_id: 1000 + i + metricOffset(metric),
    };
  });
}

function metricOffset(m: string): number {
  return ({ Revenue: 0, NetIncome: 100, TotalAssets: 200, TotalLiabilities: 300, CashAndEquivalents: 400 }[m] ?? 500);
}

function seedAapl() {
  const company: Company = {
    cik: "0000320193",
    ticker: "AAPL",
    name: "Apple Inc.",
    exchange: "Nasdaq",
    sic: "3571",
    fiscal_year_end: "0926",
    added_at: "2026-05-10T18:00:00Z",
    last_refreshed: "2026-05-10T18:00:00Z",
  };
  state.companies = [company];

  const widgets = [
    { metric: "Revenue", micro: APPLE_REVENUE_ANNUAL[APPLE_REVENUE_ANNUAL.length - 1], history: APPLE_REVENUE_ANNUAL },
    { metric: "NetIncome", micro: APPLE_NETINCOME_ANNUAL[APPLE_NETINCOME_ANNUAL.length - 1], history: APPLE_NETINCOME_ANNUAL },
    { metric: "TotalAssets", micro: APPLE_TOTAL_ASSETS_ANNUAL[APPLE_TOTAL_ASSETS_ANNUAL.length - 1], history: APPLE_TOTAL_ASSETS_ANNUAL },
  ];
  state.dashboards["0000320193"] = {
    company,
    widgets: widgets.map((w) => ({
      metric: w.metric,
      period_label: "FY2024",
      value_micro: w.micro,
      history: w.history.map((v, i) => [`FY${2014 + i}`, v] as [string, number]),
    })),
  };

  state.history["0000320193:Revenue:annual"] = seriesFor("Revenue", APPLE_REVENUE_ANNUAL, "annual");
  state.history["0000320193:NetIncome:annual"] = seriesFor("NetIncome", APPLE_NETINCOME_ANNUAL, "annual");
  state.history["0000320193:TotalAssets:annual"] = seriesFor("TotalAssets", APPLE_TOTAL_ASSETS_ANNUAL, "annual");
  state.history["0000320193:Revenue:quarterly"] = APPLE_REVENUE_ANNUAL.flatMap((annual, i) => {
    const q1 = Math.round(annual * 0.30);
    const q2 = Math.round(annual * 0.22);
    const q3 = Math.round(annual * 0.21);
    const q4 = annual - q1 - q2 - q3;
    return [q1, q2, q3, q4].map((v, qi) => ({
      period: makePeriod(2014 + i, qi + 1, "quarterly"),
      value: v,
      source_kind: "xbrl_api",
      normalized_fact_id: 5000 + i * 4 + qi,
    }));
  });

  state.events["0000320193"] = [
    {
      id: 1, cik: "0000320193", accession_no: null,
      stage: "persist", level: "info", user_visible: false,
      message: "Ingestion complete: 1000 filings, 24852 raw facts, 1306 normalized facts.",
      detail_json: null, occurred_at: new Date().toISOString(),
    },
    {
      id: 2, cik: "0000320193", accession_no: null,
      stage: "normalize", level: "info", user_visible: false,
      message: "Derived single-quarter Revenue for FY2024 Q4 from YTD difference.",
      detail_json: null, occurred_at: new Date().toISOString(),
    },
    {
      id: 3, cik: "0000320193", accession_no: null,
      stage: "discover", level: "info", user_visible: false,
      message: "Created 47 filing placeholders for accessions outside submissions.recent.",
      detail_json: null, occurred_at: new Date().toISOString(),
    },
  ];

  // Seed lineage for the latest revenue point.
  const latestRevId = state.history["0000320193:Revenue:annual"].at(-1)!.normalized_fact_id;
  const filing: Filing = {
    accession_no: "0000320193-24-000123",
    cik: "0000320193",
    form_type: "10-K",
    filed_at: "2024-11-01",
    period_of_report: "2024-09-28",
    is_amendment: false,
    amends: null,
    item_4_02_8k: false,
  };
  const rawFact: RawFact = {
    id: 9001,
    cik: "0000320193",
    accession_no: filing.accession_no,
    taxonomy: "us-gaap",
    concept: "RevenueFromContractWithCustomerExcludingAssessedTax",
    unit: "USD",
    value_numeric: APPLE_REVENUE_ANNUAL[APPLE_REVENUE_ANNUAL.length - 1],
    period_start: "2023-09-30",
    period_end: "2024-09-28",
    is_instant: false,
    fy: 2024,
    fp: "FY",
    filed: "2024-11-01",
    source_kind: "xbrl_api",
    ingested_at: "2026-05-10T18:00:00Z",
  };
  const primary: NormalizedFact = {
    id: latestRevId,
    cik: "0000320193",
    metric: "revenue",
    period_id: 20240,
    value: APPLE_REVENUE_ANNUAL[APPLE_REVENUE_ANNUAL.length - 1],
    unit: "USD",
    source_fact_id: rawFact.id,
    source_kind: "xbrl_api",
    is_primary: true,
    original_value: null,
    original_unit: null,
    fx_rate_micro: null,
    fx_rate_source: null,
    fx_rate_date: null,
    superseded_by: null,
  };
  state.lineage[latestRevId] = { primary, raw_fact: rawFact, filing, supersession_chain: [] };

  // Seed current valuation
  state.currentValuations["0000320193"] = {
    price_micro: 195_000_000,
    price_as_of: new Date().toISOString(),
    shares: 15_000_000_000,
    shares_period_end: "2024-09-28",
    market_cap_micro: 2_925_000_000_000_000,
    ttm_fcf_micro: 108_807_000_000_000_000,
    ttm_fcf_period_end: "2024-09-28",
    fcf_yield_micro: 37_198,
  };
}

// ───────── IPC mock dispatcher ─────────

mockIPC(async (cmd, args) => {
  const a = (args as Record<string, unknown>) ?? {};
  switch (cmd) {
    case "ping":
      return { message: "pong", version: "1.0.0" };
    case "list_companies":
      return state.companies;
    case "add_company": {
      const ticker = String(a.ticker ?? "").toUpperCase();
      if (!ticker) throw mkErr("invalid_input", "ticker cannot be empty");
      if (state.failAddTickers.has(ticker)) throw mkErr("unknown_ticker", `ticker ${ticker} not found in SEC ticker map`);
      if (ticker !== "AAPL") throw mkErr("unknown_ticker", `mock harness only ingests AAPL; got ${ticker}`);
      seedAapl();
      const company = state.companies[0];
      return {
        company,
        summary: {
          cik: company.cik,
          ticker: company.ticker,
          name: company.name,
          filings_ingested: 1000,
          raw_facts_ingested: 24852,
          normalized_facts_ingested: 1306,
          events_recorded: 3,
        },
      } as AddCompanyResponse;
    }
    case "remove_company": {
      state.companies = state.companies.filter((c) => c.cik !== a.cik);
      return null;
    }
    case "refresh_company": {
      const cik = String(a.cik);
      const c = state.companies.find((x) => x.cik === cik);
      if (!c) throw mkErr("not_found", `company ${cik} not found`);
      c.last_refreshed = new Date().toISOString();
      return {
        company: c,
        summary: {
          cik: c.cik, ticker: c.ticker, name: c.name,
          filings_ingested: 1000, raw_facts_ingested: 24852,
          normalized_facts_ingested: 1306, events_recorded: 3,
        },
      } as AddCompanyResponse;
    }
    case "get_dashboard": {
      const cik = String(a.cik);
      const d = state.dashboards[cik];
      if (!d) throw mkErr("not_found", `dashboard for ${cik} not seeded`);
      return d;
    }
    case "get_metric_history": {
      const key = `${a.cik}:${a.metric}:${a.kind}`;
      return state.history[key] ?? [];
    }
    case "get_ingestion_events": {
      const cik = a.cik ? String(a.cik) : "0000320193";
      return state.events[cik] ?? [];
    }
    case "get_lineage": {
      const id = Number(a.normalizedFactId);
      const lineage = state.lineage[id];
      if (!lineage) throw mkErr("not_found", `lineage for ${id} not seeded`);
      return lineage;
    }
    case "get_supersession_chain": {
      return [];
    }
    case "get_current_valuation": {
      const cik = String(a.cik);
      return state.currentValuations[cik] ?? null;
    }
    case "refresh_price": {
      const cik = String(a.cik);
      const cur = state.currentValuations[cik];
      if (cur) {
        cur.price_as_of = new Date().toISOString();
      }
      return state.currentValuations[cik] ?? null;
    }
    default:
      throw mkErr("internal", `mock harness has no handler for command: ${cmd}`);
  }
});

function mkErr(code: string, message: string) {
  return { kind: "Internal", detail: { code, message } };
}

// Auto-seed AAPL at module load when the test harness requests it via
// `window.__shouldSeedAapl = true` set by Playwright's addInitScript.
if ((window as unknown as { __shouldSeedAapl?: boolean }).__shouldSeedAapl) {
  seedAapl();
}
