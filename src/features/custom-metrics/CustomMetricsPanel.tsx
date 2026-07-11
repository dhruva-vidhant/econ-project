import { useState } from "react";

import type { CustomMetric } from "@/api/types";
import { useCustomMetrics, useDeleteCustomMetric } from "@/state/queries";

import CustomMetricEditor from "./CustomMetricEditor";
import ImportCsvDialog from "./ImportCsvDialog";

interface Props {
  cik: string;
}

/**
 * Custom-metrics management panel (§9.2, FR-CM-009). Carries a persistent
 * "user data" badge and never wires the lineage drawer — custom metrics
 * have no source-fact chain to show.
 */
export default function CustomMetricsPanel({ cik }: Props) {
  const metrics = useCustomMetrics(cik);
  const del = useDeleteCustomMetric(cik);
  const [showImport, setShowImport] = useState(false);
  const [editing, setEditing] = useState<CustomMetric | "new" | null>(null);

  return (
    <section className="rounded border border-border/60 bg-surface p-3">
      <header className="mb-2 flex items-baseline gap-2">
        <h2 className="text-sm font-semibold">Custom metrics</h2>
        <span
          className="rounded bg-accent/10 px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-accent"
          title="User-supplied data — not part of the SEC-sourced fact chain"
        >
          user data
        </span>
        <div className="ml-auto flex gap-2 text-xs">
          <button className="rounded border border-border/60 px-2 py-1 hover:bg-bg" onClick={() => setShowImport(true)}>
            Import CSV
          </button>
          <button className="rounded border border-border/60 px-2 py-1 hover:bg-bg" onClick={() => setEditing("new")}>
            + New metric
          </button>
        </div>
      </header>

      {metrics.isLoading && <p className="text-xs text-muted">Loading…</p>}

      {metrics.data && metrics.data.length === 0 && (
        <p className="text-xs text-muted">No custom metrics yet. Import a CSV or add one manually.</p>
      )}

      {metrics.data && metrics.data.length > 0 && (
        <ul className="divide-y divide-border/40 text-xs">
          {metrics.data.map((m) => (
            <li key={m.id} className="flex items-center gap-3 py-1.5">
              <span className="font-mono">{m.key}</span>
              <span className="text-muted">{m.label}</span>
              <span className="text-[10px] uppercase text-muted">{m.unit_kind}</span>
              <div className="ml-auto flex gap-2">
                <button className="text-muted hover:text-text" onClick={() => setEditing(m)}>
                  edit
                </button>
                <button
                  className="text-bad hover:text-bad"
                  onClick={() => {
                    if (window.confirm(`Delete "${m.label}" and all its points?`)) del.mutate(m.id);
                  }}
                >
                  delete
                </button>
              </div>
            </li>
          ))}
        </ul>
      )}

      {showImport && <ImportCsvDialog cik={cik} onClose={() => setShowImport(false)} />}
      {editing && (
        <CustomMetricEditor
          cik={cik}
          metric={editing === "new" ? undefined : editing}
          onClose={() => setEditing(null)}
        />
      )}
    </section>
  );
}
