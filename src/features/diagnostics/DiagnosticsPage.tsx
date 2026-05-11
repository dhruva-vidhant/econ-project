import { useState } from "react";
import { Link, useParams } from "react-router-dom";

import { useCompanies, useEvents } from "@/state/queries";

export default function DiagnosticsPage() {
  const { ticker } = useParams<{ ticker: string }>();
  const [filter, setFilter] = useState<"all" | "info" | "warn" | "error">("all");
  const companies = useCompanies();
  const company = (companies.data ?? []).find((c) => c.ticker === ticker);
  const events = useEvents(company?.cik ?? null, 500);

  const rows = (events.data ?? []).filter((e) => filter === "all" || e.level === filter);

  return (
    <div className="mx-auto max-w-6xl px-6 py-6">
      <header className="mb-3 flex items-baseline gap-3">
        <Link to={`/c/${ticker}`} className="font-mono text-lg hover:text-accent">{ticker}</Link>
        <span className="text-muted">/ diagnostics</span>
        <div className="ml-auto inline-flex rounded border border-border/60 text-xs">
          {(["all", "info", "warn", "error"] as const).map((k) => (
            <button
              key={k}
              onClick={() => setFilter(k)}
              className={`px-3 py-1 capitalize ${filter === k ? "bg-accent/20" : "text-muted hover:text-text"}`}
            >
              {k}
            </button>
          ))}
        </div>
      </header>

      <div className="rounded border border-border/60 bg-surface text-xs">
        {rows.length === 0 && <p className="p-4 text-muted">No events.</p>}
        {rows.map((e) => (
          <div key={e.id} className="flex items-baseline gap-3 border-b border-border/30 px-3 py-1.5 last:border-b-0">
            <span className={`w-12 shrink-0 font-mono uppercase ${
              e.level === "error" ? "text-bad" : e.level === "warn" ? "text-yellow-300" : "text-muted"
            }`}>{e.level}</span>
            <span className="w-20 shrink-0 font-mono text-muted">{e.stage}</span>
            <span className="flex-1">{e.message}</span>
            <span className="shrink-0 text-muted">{new Date(e.occurred_at).toLocaleString()}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
