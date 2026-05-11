import { useState } from "react";
import { Link, useParams } from "react-router-dom";

import LineageDrawer from "@/components/LineageDrawer";
import MetricChart from "@/components/MetricChart";
import { fmtUsdCompact } from "@/api/types";
import { useCompanies, useMetricHistory } from "@/state/queries";

const PRETTY: Record<string, string> = {
  Revenue: "Revenue",
  CostOfRevenue: "Cost of Revenue",
  GrossProfit: "Gross Profit",
  OperatingIncome: "Operating Income",
  NetIncome: "Net Income",
  EpsBasic: "EPS (Basic)",
  EpsDiluted: "EPS (Diluted)",
  CashAndEquivalents: "Cash & Equivalents",
  LongTermDebt: "Long-term Debt",
  CurrentDebt: "Current Debt",
  TotalDebt: "Total Debt",
  TotalAssets: "Total Assets",
  TotalLiabilities: "Total Liabilities",
  TotalEquity: "Total Equity",
  CashFromOperations: "Cash from Operations",
  CapitalExpenditures: "Capital Expenditures",
  DepreciationAmortization: "Depreciation & Amortization",
  HistoricalMarketCap: "Historical Market Cap",
  CurrentMarketCap: "Current Market Cap",
};

export default function MetricDrillPage() {
  const { ticker, metric } = useParams<{ ticker: string; metric: string }>();
  const [kind, setKind] = useState<"annual" | "quarterly">("annual");
  const [openId, setOpenId] = useState<number | null>(null);
  const companies = useCompanies();
  const company = (companies.data ?? []).find((c) => c.ticker === ticker);
  const history = useMetricHistory(company?.cik, metric ?? "Revenue", kind);

  const series = history.data ?? [];
  const label = (metric && PRETTY[metric]) ?? metric ?? "";

  return (
    <>
      <div className="mx-auto max-w-6xl px-6 py-6">
        <header className="mb-4 flex items-baseline gap-3">
          <Link to={`/c/${ticker}`} className="font-mono text-lg hover:text-accent">{ticker}</Link>
          <span className="text-muted">/</span>
          <span className="text-base font-semibold">{label}</span>
          <div className="ml-auto inline-flex rounded border border-border/60 text-xs">
            {(["annual", "quarterly"] as const).map((k) => (
              <button
                key={k}
                onClick={() => setKind(k)}
                className={`px-3 py-1 ${kind === k ? "bg-accent/20 text-text" : "text-muted hover:text-text"}`}
              >
                {k}
              </button>
            ))}
          </div>
        </header>

        <div className="mb-4">
          <MetricChart series={series} height={360} />
        </div>

        <section className="rounded border border-border/60 bg-surface">
          <table className="w-full text-xs">
            <thead className="border-b border-border/60 text-left text-muted">
              <tr>
                <th className="px-3 py-1.5">Period</th>
                <th className="px-3 py-1.5">Period span</th>
                <th className="px-3 py-1.5 text-right">Value</th>
                <th className="px-3 py-1.5">Source</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {series.length === 0 && (
                <tr><td colSpan={5} className="px-3 py-6 text-center text-muted">No values for this metric.</td></tr>
              )}
              {series.map((p) => (
                <tr key={p.normalized_fact_id} className="border-b border-border/30 last:border-b-0 hover:bg-bg/40">
                  <td className="px-3 py-1.5 font-mono">
                    {p.period.kind === "quarterly"
                      ? `FY${p.period.fiscal_year} Q${p.period.fiscal_quarter}`
                      : `FY${p.period.fiscal_year}`}
                  </td>
                  <td className="px-3 py-1.5 text-muted">{p.period.start_date} → {p.period.end_date}</td>
                  <td className="px-3 py-1.5 text-right font-mono">{fmtUsdCompact(p.value)}</td>
                  <td className="px-3 py-1.5 text-muted">{p.source_kind}</td>
                  <td className="px-3 py-1.5 text-right">
                    <button
                      className="text-xs text-accent hover:underline"
                      onClick={() => setOpenId(p.normalized_fact_id)}
                    >
                      lineage
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </section>
      </div>
      <LineageDrawer normalizedFactId={openId} onClose={() => setOpenId(null)} />
    </>
  );
}
