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
  CustomMetric,
  CustomMetricPatch,
  CustomMetricPoint,
  CustomSeriesPoint,
  DashboardPayload,
  ExportReport,
  ExportRequest,
  ImportOptions,
  ImportReport,
  IngestionEvent,
  LineagePayload,
  MetricSeriesPoint,
  NewCustomMetric,
  Period,
  Filing,
  RawFact,
  NormalizedFact,
  UnitKind,
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
  customMetrics: Record<string, CustomMetric[]>; // keyed by cik
  customPoints: Record<number, CustomMetricPoint[]>; // keyed by custom_metric_id
  nextCustomMetricId: number;
  nextCustomPointId: number;
  savedDialogPath: string | null; // stubbed native save-dialog response
};

const state: State = {
  companies: [],
  events: {},
  history: {},
  dashboards: {},
  lineage: {},
  currentValuations: {},
  failAddTickers: new Set(["XYZNOPE", "BADTKR"]),
  customMetrics: {},
  customPoints: {},
  nextCustomMetricId: 1,
  nextCustomPointId: 1,
  savedDialogPath: "/tmp/e2e-export.csv",
};

declare global {
  interface Window {
    __econMock: {
      reset: () => void;
      seedAaplFixture: () => void;
      setOffline: (offline: boolean) => void;
      setSaveDialogPath: (path: string | null) => void;
      state: State;
    };
  }
}

let offline = false;

window.__econMock = {
  state,
  reset() {
    state.companies = [];
    state.events = {};
    state.history = {};
    state.dashboards = {};
    state.lineage = {};
    state.currentValuations = {};
    state.customMetrics = {};
    state.customPoints = {};
    state.nextCustomMetricId = 1;
    state.nextCustomPointId = 1;
    offline = false;
  },
  seedAaplFixture() {
    seedAapl();
  },
  setOffline(value: boolean) {
    offline = value;
  },
  setSaveDialogPath(path: string | null) {
    state.savedDialogPath = path;
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
  // `getMetricHistory` (used by the custom-metrics "Compare" builtin options)
  // passes the lowercase `Metric` id, unlike the widget/drill-page callers
  // above which historically used the display-cased key — alias both.
  state.history["0000320193:revenue:annual"] = state.history["0000320193:Revenue:annual"];
  state.history["0000320193:net_income:annual"] = state.history["0000320193:NetIncome:annual"];
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

// ───────── Custom-metrics CSV import (mirrors src-tauri/src/import/csv.rs) ─────────

function parseMockCsv(text: string): string[][] {
  return text
    .split(/\r?\n/)
    .filter((line) => line.trim().length > 0)
    .map((line) => line.split(",").map((f) => f.trim()));
}

function toMicro(raw: string): number {
  const n = Number(raw);
  if (!Number.isFinite(n)) throw new Error("bad_value");
  return Math.round(n * 1_000_000);
}

function importCustomMetricsCsvMock(cik: string, csvText: string, options: ImportOptions): ImportReport {
  const report: ImportReport = { metrics_created: [], imported: 0, updated: 0, unaligned: 0, rejected: [] };
  const rows = parseMockCsv(csvText);
  const header = rows[0];
  if (!header) return report;
  const idx = (name: string) => header.findIndex((h) => h.toLowerCase() === name);
  const metricIdx = idx("metric");
  const periodIdx = idx("period");
  const valueIdx = idx("value");
  const unitIdx = idx("unit");
  const labelIdx = idx("label");

  const metrics = state.customMetrics[cik] ?? (state.customMetrics[cik] = []);

  rows.slice(1).forEach((row, rowIndex) => {
    const raw = row.join(",");
    const metricStr = row[metricIdx];
    const periodStr = row[periodIdx];
    const valueStr = row[valueIdx];
    if (!metricStr || !periodStr || !valueStr) {
      report.rejected.push({ row_index: rowIndex, reason: "missing_column", raw });
      return;
    }
    const key = metricStr.toLowerCase();
    const unitToken = unitIdx >= 0 ? row[unitIdx] : undefined;
    const label = labelIdx >= 0 ? row[labelIdx] : undefined;
    const unitKind = (unitToken?.toLowerCase() as UnitKind | undefined) ?? options.default_unit;

    let valueMicro: number;
    try {
      valueMicro = toMicro(valueStr);
    } catch {
      report.rejected.push({ row_index: rowIndex, reason: "bad_value", raw });
      return;
    }

    let metric = metrics.find((m) => m.key === key);
    if (!metric) {
      metric = {
        id: state.nextCustomMetricId++,
        cik,
        key,
        label: label ?? key,
        unit_kind: unitKind,
        currency: unitKind === "currency" ? "USD" : null,
        decimals: 2,
        description: null,
        created_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
      };
      metrics.push(metric);
      report.metrics_created.push(key);
    }

    const points = state.customPoints[metric.id] ?? (state.customPoints[metric.id] = []);
    const fyMatch = /^FY(\d{4})$/.exec(periodStr);
    if (!fyMatch) {
      report.unaligned += 1;
      points.push({
        id: state.nextCustomPointId++,
        custom_metric_id: metric.id,
        period_id: null,
        observed_date: periodStr,
        value_micro: valueMicro,
        source_note: "csv import",
        imported_at: new Date().toISOString(),
      });
      return;
    }
    const fy = Number(fyMatch[1]);
    const periodId = fy * 10; // matches makePeriod's annual id convention
    const existing = points.find((p) => p.period_id === periodId);
    if (existing) {
      existing.value_micro = valueMicro;
      report.updated += 1;
    } else {
      points.push({
        id: state.nextCustomPointId++,
        custom_metric_id: metric.id,
        period_id: periodId,
        observed_date: null,
        value_micro: valueMicro,
        source_note: "csv import",
        imported_at: new Date().toISOString(),
      });
      report.imported += 1;
    }
  });

  return report;
}

function customSeriesFor(metric: CustomMetric, kind: "annual" | "quarterly"): CustomSeriesPoint[] {
  const points = state.customPoints[metric.id] ?? [];
  return points
    .filter((p) => p.period_id != null)
    .map((p) => {
      const fy = Math.floor(p.period_id! / 10);
      return {
        period: makePeriod(fy, kind === "annual" ? 0 : 4, kind),
        value_micro: p.value_micro,
        unit_kind: metric.unit_kind,
        point_id: p.id,
      };
    });
}

function renderCsvMock(request: ExportRequest): string {
  if (request.type === "custom_metric_series") {
    const metric = Object.values(state.customMetrics)
      .flat()
      .find((m) => m.id === request.metric_id);
    if (!metric) throw mkErr("not_found", `custom metric ${request.metric_id} not found`);
    const rows = customSeriesFor(metric, request.kind);
    const lines = ["period,value"];
    for (const r of rows) {
      const label = r.period.kind === "quarterly" ? `FY${r.period.fiscal_year} Q${r.period.fiscal_quarter}` : `FY${r.period.fiscal_year}`;
      lines.push(`${label},${(r.value_micro / 1_000_000).toFixed(metric.decimals)}`);
    }
    return lines.join("\n") + "\n";
  }
  if (request.type === "metric_series") {
    const rows = state.history[`${request.cik}:${request.metric}:${request.kind}`] ?? [];
    const lines = ["period,value"];
    for (const r of rows) {
      const label = r.period.kind === "quarterly" ? `FY${r.period.fiscal_year} Q${r.period.fiscal_quarter}` : `FY${r.period.fiscal_year}`;
      lines.push(`${label},${(r.value / 1_000_000).toFixed(2)}`);
    }
    return lines.join("\n") + "\n";
  }
  // "comparison" and "statement" render as a minimal but non-empty CSV for E2E smoke purposes.
  return "period,value\nFY2024,0\n";
}

// ───────── IPC mock dispatcher ─────────

mockIPC(async (cmd, args) => {
  const a = (args as Record<string, unknown>) ?? {};
  switch (cmd) {
    case "ping":
      return { message: "pong", version: "1.5.0" };
    case "plugin:dialog|save":
      return state.savedDialogPath;
    case "list_companies":
      return state.companies;
    case "add_company": {
      // Ingestion hits real SEC EDGAR — the only command that actually
      // requires network, so it's the one gated by the offline flag.
      if (offline) throw mkErr("network_error", "network is disabled (offline mode)");
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
    case "list_custom_metrics": {
      const cik = String(a.cik);
      return state.customMetrics[cik] ?? [];
    }
    case "create_custom_metric": {
      const cik = String(a.cik);
      const def = a.def as NewCustomMetric;
      const metrics = state.customMetrics[cik] ?? (state.customMetrics[cik] = []);
      const metric: CustomMetric = {
        id: state.nextCustomMetricId++,
        cik,
        key: def.key,
        label: def.label,
        unit_kind: def.unit_kind,
        currency: def.currency ?? (def.unit_kind === "currency" ? "USD" : null),
        decimals: def.decimals ?? 2,
        description: def.description ?? null,
        created_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
      };
      metrics.push(metric);
      return metric;
    }
    case "update_custom_metric": {
      const id = Number(a.id);
      const patch = a.patch as CustomMetricPatch;
      const metric = Object.values(state.customMetrics).flat().find((m) => m.id === id);
      if (!metric) throw mkErr("not_found", `custom metric ${id} not found`);
      Object.assign(metric, patch, { updated_at: new Date().toISOString() });
      return metric;
    }
    case "delete_custom_metric": {
      const id = Number(a.id);
      for (const cik of Object.keys(state.customMetrics)) {
        state.customMetrics[cik] = state.customMetrics[cik].filter((m) => m.id !== id);
      }
      delete state.customPoints[id];
      return null;
    }
    case "count_custom_points": {
      const id = Number(a.id);
      return (state.customPoints[id] ?? []).length;
    }
    case "get_custom_metric_series": {
      const metricId = Number(a.metricId);
      const kind = a.kind as "annual" | "quarterly";
      const metric = Object.values(state.customMetrics).flat().find((m) => m.id === metricId);
      if (!metric) return [];
      return customSeriesFor(metric, kind);
    }
    case "upsert_custom_metric_point": {
      const metricId = Number(a.metricId);
      const period = String(a.period);
      const value = String(a.value);
      const metric = Object.values(state.customMetrics).flat().find((m) => m.id === metricId);
      if (!metric) throw mkErr("not_found", `custom metric ${metricId} not found`);
      const points = state.customPoints[metricId] ?? (state.customPoints[metricId] = []);
      const fyMatch = /^FY(\d{4})$/.exec(period);
      const point: CustomMetricPoint = {
        id: state.nextCustomPointId++,
        custom_metric_id: metricId,
        period_id: fyMatch ? Number(fyMatch[1]) * 10 : null,
        observed_date: fyMatch ? null : period,
        value_micro: toMicro(value),
        source_note: "manual",
        imported_at: new Date().toISOString(),
      };
      points.push(point);
      return point;
    }
    case "delete_custom_metric_point": {
      const pointId = Number(a.pointId);
      for (const metricId of Object.keys(state.customPoints)) {
        state.customPoints[Number(metricId)] = state.customPoints[Number(metricId)].filter((p) => p.id !== pointId);
      }
      return null;
    }
    case "import_custom_metrics_csv": {
      const cik = String(a.cik);
      const csvText = String(a.csvText);
      const options = a.options as ImportOptions;
      return importCustomMetricsCsvMock(cik, csvText, options);
    }
    case "render_csv": {
      return renderCsvMock(a.request as ExportRequest);
    }
    case "export_csv": {
      const request = a.request as ExportRequest;
      const destPath = String(a.destPath);
      const text = renderCsvMock(request);
      const rows = text.split("\n").filter(Boolean).length - 1;
      return { rows, bytes: text.length, path: destPath } as ExportReport;
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
