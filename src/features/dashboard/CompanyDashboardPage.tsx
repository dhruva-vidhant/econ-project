import { useNavigate, useParams } from "react-router-dom";

import { useCompanies, useDashboard, useEvents } from "@/state/queries";
import { fmtUsdCompact } from "@/api/types";
import Sparkline from "@/components/Sparkline";

export default function CompanyDashboardPage() {
  const { ticker } = useParams<{ ticker: string }>();
  const nav = useNavigate();
  const companies = useCompanies();
  const company = (companies.data ?? []).find((c) => c.ticker === ticker);
  const dashboard = useDashboard(company?.cik);
  const events = useEvents(company?.cik ?? null, 50);

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
    <div className="mx-auto max-w-7xl px-6 py-6">
      <header className="mb-6 flex items-baseline gap-3">
        <span className="font-mono text-2xl">{company.ticker}</span>
        <span className="text-base text-muted">{company.name}</span>
        <span className="ml-auto text-xs text-muted">
          CIK {company.cik}
          {company.fiscal_year_end && <> · FYE {company.fiscal_year_end}</>}
          {company.last_refreshed && <> · refreshed {new Date(company.last_refreshed).toLocaleString()}</>}
        </span>
      </header>

      {dashboard.isLoading && <Loading />}
      {dashboard.error && (
        <Error msg={(dashboard.error as { detail?: { message?: string } })?.detail?.message ?? "Failed to load dashboard."} />
      )}

      {dashboard.data && dashboard.data.widgets.length === 0 && (
        <div className="rounded border border-border/60 bg-surface p-4 text-sm text-muted">
          No normalized facts persisted yet. The pipeline ingested filings but
          may not have produced any of the catalog metrics — check Diagnostics
          for details.
        </div>
      )}

      {dashboard.data && dashboard.data.widgets.length > 0 && (
        <section className="mb-8 grid grid-cols-2 gap-4 md:grid-cols-3 lg:grid-cols-5">
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
              <div className="mt-2 h-8">
                <Sparkline values={w.history.map(([, v]) => v / 1_000_000)} />
              </div>
            </button>
          ))}
        </section>
      )}

      <section>
        <header className="mb-2 flex items-baseline gap-2">
          <h2 className="text-sm font-semibold">Recent ingestion events</h2>
          <button
            className="ml-auto text-xs text-muted hover:text-text"
            onClick={() => nav(`/c/${ticker}/diagnostics`)}
          >
            view all →
          </button>
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
  );
}

function Loading() {
  return <p className="px-6 py-4 text-sm text-muted">Loading…</p>;
}

function Error({ msg }: { msg: string }) {
  return (
    <div className="rounded border border-bad/40 bg-bad/10 p-3 text-xs text-bad">{msg}</div>
  );
}
