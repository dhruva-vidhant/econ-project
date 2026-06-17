import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";

import { fmtMetricValue, prettyMetric } from "@/api/types";
import { useCompanies, useMetricHistory } from "@/state/queries";

const STATEMENT_METRICS: Record<string, string[]> = {
  income: [
    "revenue", "cost_of_revenue", "gross_profit",
    "operating_income", "operating_margin", "net_income",
    "eps_basic", "eps_diluted",
  ],
  balance: [
    "total_assets", "total_liabilities", "total_equity",
    "cash_and_equivalents", "long_term_debt", "current_debt", "total_debt",
  ],
  cashflow: [
    "cash_from_operations", "capital_expenditures", "depreciation_amortization",
    "free_cash_flow",
  ],
  valuation: [
    "historical_market_cap", "free_cash_flow_ttm", "free_cash_flow_yield",
  ],
};

export default function StatementPage() {
  const { ticker, kind: stmtKind = "income" } = useParams<{ ticker: string; kind: string }>();
  const [periodKind, setPeriodKind] = useState<"annual" | "quarterly">("annual");
  const nav = useNavigate();
  const companies = useCompanies();
  const company = (companies.data ?? []).find((c) => c.ticker === ticker);

  const metrics = STATEMENT_METRICS[stmtKind] ?? STATEMENT_METRICS.income;

  return (
    <div className="mx-auto max-w-7xl px-6 py-6">
      <header className="mb-4 flex items-baseline gap-3">
        <Link to={`/c/${ticker}`} className="font-mono text-lg hover:text-accent">{ticker}</Link>
        <span className="text-muted">/</span>
        <span className="font-semibold capitalize">{stmtKind} statement</span>
        <div className="ml-auto inline-flex gap-2">
          <div className="inline-flex rounded border border-border/60 text-xs">
            {(["income", "balance", "cashflow", "valuation"] as const).map((k) => (
              <button
                key={k}
                onClick={() => nav(`/c/${ticker}/statement/${k}`)}
                className={`px-3 py-1 capitalize ${stmtKind === k ? "bg-accent/20" : "text-muted hover:text-text"}`}
              >
                {k}
              </button>
            ))}
          </div>
          <div className="inline-flex rounded border border-border/60 text-xs">
            {(["annual", "quarterly"] as const).map((k) => (
              <button
                key={k}
                onClick={() => setPeriodKind(k)}
                className={`px-3 py-1 ${periodKind === k ? "bg-accent/20" : "text-muted hover:text-text"}`}
              >
                {k}
              </button>
            ))}
          </div>
        </div>
      </header>

      <div className="overflow-x-auto rounded border border-border/60 bg-surface">
        <StatementTable cik={company?.cik} metrics={metrics} kind={periodKind} ticker={ticker!} />
      </div>
    </div>
  );
}

function StatementTable({
  cik,
  metrics,
  kind,
  ticker,
}: {
  cik: string | undefined;
  metrics: string[];
  kind: "annual" | "quarterly";
  ticker: string;
}) {
  // Column headers: union of all period labels across the metrics' series.
  // For simplicity, fetch each series and align. We need to call useMetricHistory
  // outside conditionals and outside loops, so we render one row per metric with
  // its own data fetch inside Row.
  return (
    <table className="w-full text-xs">
      <thead className="border-b border-border/60 text-muted">
        <tr>
          <th className="sticky left-0 bg-surface px-3 py-1.5 text-left">Metric</th>
          <th className="px-3 py-1.5 text-right">Latest</th>
          <th className="px-3 py-1.5 text-right">Prior</th>
          <th className="px-3 py-1.5 text-right">2 prior</th>
          <th className="px-3 py-1.5 text-right">3 prior</th>
          <th className="px-3 py-1.5 text-right">4 prior</th>
          <th />
        </tr>
      </thead>
      <tbody>
        {metrics.map((m) => (
          <StatementRow key={m} cik={cik} metric={m} kind={kind} ticker={ticker} />
        ))}
      </tbody>
    </table>
  );
}

function StatementRow({
  cik,
  metric,
  kind,
  ticker,
}: {
  cik: string | undefined;
  metric: string;
  kind: "annual" | "quarterly";
  ticker: string;
}) {
  const { data, isLoading } = useMetricHistory(cik, metric, kind);
  const series = (data ?? []).slice().reverse(); // newest first
  const cells = [0, 1, 2, 3, 4].map((i) => series[i]);

  return (
    <tr className="border-b border-border/30 last:border-b-0 hover:bg-bg/40">
      <td className="sticky left-0 bg-surface px-3 py-1.5">
        <Link to={`/c/${ticker}/metric/${metric}`} className="hover:text-accent">
          {prettyMetric(metric)}
        </Link>
      </td>
      {cells.map((p, i) => (
        <td key={i} className="px-3 py-1.5 text-right font-mono">
          {p ? (
            <>
              <div>{fmtMetricValue(metric, p.value)}</div>
              <div className="text-[10px] text-muted">
                {p.period.kind === "quarterly"
                  ? `FY${p.period.fiscal_year} Q${p.period.fiscal_quarter}`
                  : `FY${p.period.fiscal_year}`}
              </div>
            </>
          ) : (
            <span className="text-muted">{isLoading ? "…" : "—"}</span>
          )}
        </td>
      ))}
      <td />
    </tr>
  );
}
