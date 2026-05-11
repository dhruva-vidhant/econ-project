import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";

import LineageDrawer from "@/components/LineageDrawer";
import MetricChart from "@/components/MetricChart";
import { fmtUsdCompact } from "@/api/types";
import {
  useCompanies,
  useDashboard,
  useEvents,
  useMetricHistory,
  useRefreshCompany,
} from "@/state/queries";

const HEADLINE_METRICS = ["Revenue", "NetIncome", "TotalAssets"] as const;

export default function CompanyDashboardPage() {
  const { ticker } = useParams<{ ticker: string }>();
  const nav = useNavigate();
  const [chartKind, setChartKind] = useState<"annual" | "quarterly">("annual");
  const [openLineage, setOpenLineage] = useState<number | null>(null);
  const companies = useCompanies();
  const company = (companies.data ?? []).find((c) => c.ticker === ticker);
  const dashboard = useDashboard(company?.cik);
  const events = useEvents(company?.cik ?? null, 50);
  const refresh = useRefreshCompany();

  if (companies.isLoading) return <Loading />;
  if (!company) {
    return (
      <div className="mx-auto max-w-3xl px-6 py-8">
        <p className="text-sm">
          {ticker} is not in your saved companies.{" "}
          <button className="text-accent underline" onClick={() => nav("/")}>Go home</button> to add it.
        </p>
      </div>
    );
  }

  return (
    <>
      <div className="mx-auto max-w-7xl px-6 py-6">
        <header className="mb-6 flex items-baseline gap-3">
          <span className="font-mono text-2xl">{company.ticker}</span>
          <span className="text-base text-muted">{company.name}</span>
          <span className="ml-3 text-xs text-muted">
            CIK {company.cik}
            {company.fiscal_year_end && <> · FYE {company.fiscal_year_end}</>}
            {company.last_refreshed && <> · refreshed {new Date(company.last_refreshed).toLocaleString()}</>}
          </span>
          <button
            className="ml-auto rounded border border-accent/60 bg-accent/10 px-3 py-1 text-xs hover:bg-accent/20 disabled:opacity-50"
            onClick={() => refresh.mutate(company.cik)}
            disabled={refresh.isPending}
          >
            {refresh.isPending ? "Refreshing…" : "Refresh"}
          </button>
        </header>

        {refresh.isError && (
          <Error msg={(refresh.error as { detail?: { message?: string } })?.detail?.message ?? "Refresh failed."} />
        )}

        {dashboard.isLoading && <Loading />}

        {dashboard.data && dashboard.data.widgets.length > 0 && (
          <section className="mb-6 grid grid-cols-2 gap-4 md:grid-cols-3 lg:grid-cols-5">
            {dashboard.data.widgets.map((w) => (
              <button
                key={w.metric}
                className="rounded border border-border/60 bg-surface p-3 text-left transition hover:border-accent/60"
                onClick={() => nav(`/c/${ticker}/metric/${w.metric}`)}
              >
                <div className="text-[11px] uppercase tracking-wide text-muted">
                  {w.metric.replace(/_/g, " ")}
                </div>
                <div className="my-1 font-mono text-xl">{fmtUsdCompact(w.value_micro)}</div>
                <div className="text-[11px] text-muted">{w.period_label}</div>
              </button>
            ))}
          </section>
        )}

        <section className="mb-6">
          <header className="mb-2 flex items-baseline gap-2">
            <h2 className="text-sm font-semibold">Time series</h2>
            <div className="ml-2 inline-flex rounded border border-border/60 text-xs">
              {(["annual", "quarterly"] as const).map((k) => (
                <button
                  key={k}
                  onClick={() => setChartKind(k)}
                  className={`px-3 py-1 ${chartKind === k ? "bg-accent/20 text-text" : "text-muted hover:text-text"}`}
                >
                  {k}
                </button>
              ))}
            </div>
            <div className="ml-auto inline-flex rounded border border-border/60 text-xs">
              {(["income", "balance", "cashflow"] as const).map((k) => (
                <Link
                  key={k}
                  to={`/c/${ticker}/statement/${k}`}
                  className="px-3 py-1 capitalize text-muted hover:text-text"
                >
                  {k}
                </Link>
              ))}
              <Link to={`/c/${ticker}/diagnostics`} className="px-3 py-1 text-muted hover:text-text">
                diagnostics
              </Link>
            </div>
          </header>
          <div className="grid grid-cols-1 gap-3 md:grid-cols-3">
            {HEADLINE_METRICS.map((m) => (
              <ChartCard
                key={m}
                cik={company.cik}
                metric={m}
                kind={chartKind}
                onPointClick={(id) => setOpenLineage(id)}
              />
            ))}
          </div>
        </section>

        <section>
          <header className="mb-2 flex items-baseline gap-2">
            <h2 className="text-sm font-semibold">Recent ingestion events</h2>
            <Link to={`/c/${ticker}/diagnostics`} className="ml-auto text-xs text-muted hover:text-text">view all →</Link>
          </header>
          <ul className="rounded border border-border/60 bg-surface text-xs">
            {(events.data ?? []).slice(0, 8).map((e) => (
              <li key={e.id} className="flex items-baseline gap-3 border-b border-border/40 px-3 py-1.5 last:border-b-0">
                <span className="w-12 shrink-0 font-mono uppercase text-muted">{e.level}</span>
                <span className="w-20 shrink-0 font-mono text-muted">{e.stage}</span>
                <span className="flex-1">{e.message}</span>
                <span className="shrink-0 text-muted">{new Date(e.occurred_at).toLocaleTimeString()}</span>
              </li>
            ))}
            {(events.data ?? []).length === 0 && (
              <li className="px-3 py-3 text-muted">No events recorded yet.</li>
            )}
          </ul>
        </section>
      </div>
      <LineageDrawer normalizedFactId={openLineage} onClose={() => setOpenLineage(null)} />
    </>
  );
}

function ChartCard({
  cik,
  metric,
  kind,
  onPointClick: _onPointClick,
}: {
  cik: string;
  metric: string;
  kind: "annual" | "quarterly";
  onPointClick: (id: number) => void;
}) {
  const { data } = useMetricHistory(cik, metric, kind);
  return (
    <div>
      <MetricChart series={data ?? []} title={metric.replace(/([A-Z])/g, " $1").trim()} height={220} />
    </div>
  );
}

function Loading() { return <p className="px-6 py-4 text-sm text-muted">Loading…</p>; }
function Error({ msg }: { msg: string }) {
  return <div className="mb-4 rounded border border-bad/40 bg-bad/10 p-3 text-xs text-bad">{msg}</div>;
}
