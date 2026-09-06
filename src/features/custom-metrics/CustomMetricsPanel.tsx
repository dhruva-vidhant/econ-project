import { useState } from "react";

import ComparisonChart from "@/components/ComparisonChart";
import type { NamedSeries } from "@/components/ComparisonChart";
import DownloadCsvButton from "@/components/DownloadCsvButton";
import MetricChart from "@/components/MetricChart";
import type { CustomMetric, CustomSeriesPoint, MetricSeriesPoint, PeriodKind, SeriesRef } from "@/api/types";
import {
  useCustomMetricSeries,
  useCustomMetrics,
  useDeleteCustomMetric,
  useMetricHistory,
} from "@/state/queries";

import CustomMetricEditor from "./CustomMetricEditor";
import ImportCsvDialog from "./ImportCsvDialog";

interface Props {
  cik: string;
  ticker: string;
}

const BUILTIN_COMPARE_OPTIONS = ["revenue", "net_income"] as const;

/**
 * Custom-metrics management panel (§9.2, FR-CM-009). Carries a persistent
 * source badge and never wires the lineage drawer — custom metrics have no
 * normalized-fact chain to show.
 */
export default function CustomMetricsPanel({ cik, ticker }: Props) {
  const metrics = useCustomMetrics(cik);
  const del = useDeleteCustomMetric(cik);
  const [showImport, setShowImport] = useState(false);
  const [editing, setEditing] = useState<CustomMetric | "new" | null>(null);
  const [chartKind, setChartKind] = useState<PeriodKind>("annual");
  const [compareKeys, setCompareKeys] = useState<Set<string>>(new Set());

  const toggleCompare = (key: string) => {
    setCompareKeys((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  return (
    <section className="rounded border border-border/60 bg-surface p-3">
      <header className="mb-2 flex items-baseline gap-2">
        <h2 className="text-sm font-semibold">Custom metrics</h2>
        <span
          className="rounded bg-accent/10 px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-accent"
          title="Includes user-supplied and reviewed company operating data outside the normalized fact chain"
        >
          supplemental data
        </span>
        <div className="ml-auto inline-flex rounded border border-border/60 text-xs">
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
        <div className="flex gap-2 text-xs">
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
        <PanelBody
          // `metrics.data` drives a fixed-length `useCustomMetricSeries` call
          // per metric below; keying on the id set forces a remount (fresh
          // hook-call count) instead of a same-instance re-render whenever a
          // metric is added or removed, which would otherwise violate the
          // Rules of Hooks.
          key={metrics.data.map((m) => m.id).join(",")}
          cik={cik}
          ticker={ticker}
          metrics={metrics.data}
          chartKind={chartKind}
          compareKeys={compareKeys}
          toggleCompare={toggleCompare}
          onEdit={setEditing}
          onDelete={(id, label) => {
            if (window.confirm(`Delete "${label}" and all its points?`)) del.mutate(id);
          }}
        />
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

function toPoints(series: MetricSeriesPoint[] | CustomSeriesPoint[]): { period: MetricSeriesPoint["period"]; value_micro: number }[] {
  return series.map((p) => ({
    period: p.period,
    value_micro: "value" in p ? p.value : p.value_micro,
  }));
}

/**
 * Fetches series for every custom metric plus the fixed built-in compare
 * options unconditionally — a stable hook-call count per render — then
 * filters down with plain array logic to whatever the user checked. This
 * avoids calling query hooks inside a variable-length loop.
 */
function PanelBody({
  cik,
  ticker,
  metrics,
  chartKind,
  compareKeys,
  toggleCompare,
  onEdit,
  onDelete,
}: {
  cik: string;
  ticker: string;
  metrics: CustomMetric[];
  chartKind: PeriodKind;
  compareKeys: Set<string>;
  toggleCompare: (key: string) => void;
  onEdit: (m: CustomMetric | "new") => void;
  onDelete: (id: number, label: string) => void;
}) {
  const revenue = useMetricHistory(cik, "revenue", chartKind);
  const netIncome = useMetricHistory(cik, "net_income", chartKind);
  const builtinHistoryByKey: Record<string, MetricSeriesPoint[] | undefined> = {
    revenue: revenue.data,
    net_income: netIncome.data,
  };

  // Fixed-length: one `useCustomMetricSeries` call per metric in `metrics`,
  // same order every render — `metrics` itself only changes between renders
  // via the parent re-querying `useCustomMetrics`, not per-toggle.
  const customSeriesResults = metrics.map((m) => useCustomMetricSeries(cik, m.id, chartKind));

  const compareRefs: SeriesRef[] = Array.from(compareKeys).map((key) => ({
    source: (BUILTIN_COMPARE_OPTIONS as readonly string[]).includes(key) ? "builtin" : "custom",
    key,
  }));

  const namedSeries: NamedSeries[] = compareRefs
    .map((ref): NamedSeries | null => {
      if (ref.source === "builtin") {
        const data = builtinHistoryByKey[ref.key];
        return { label: ref.key, unitKind: "currency", points: toPoints(data ?? []) };
      }
      const idx = metrics.findIndex((m) => m.key === ref.key);
      if (idx === -1) return null;
      const metric = metrics[idx];
      const data = customSeriesResults[idx]?.data ?? [];
      return { label: metric.label, unitKind: metric.unit_kind, decimals: metric.decimals, points: toPoints(data) };
    })
    .filter((s): s is NamedSeries => s !== null);

  return (
    <>
      <ul className="mb-4 divide-y divide-border/40 text-xs">
        {metrics.map((m) => (
          <li key={m.id} className="flex items-center gap-3 py-1.5">
            <label className="flex items-center gap-1.5">
              <input type="checkbox" checked={compareKeys.has(m.key)} onChange={() => toggleCompare(m.key)} />
              <span className="font-mono">{m.key}</span>
            </label>
            <span className="text-muted">{m.label}</span>
            <span className="text-[10px] uppercase text-muted">{m.unit_kind}</span>
            <div className="ml-auto flex gap-2">
              {m.key.startsWith("bbw_") ? (
                <span className="text-[10px] uppercase text-accent">SEC reviewed</span>
              ) : (
                <>
                  <button className="text-muted hover:text-text" onClick={() => onEdit(m)}>
                    edit
                  </button>
                  <button className="text-bad hover:text-bad" onClick={() => onDelete(m.id, m.label)}>
                    delete
                  </button>
                </>
              )}
            </div>
          </li>
        ))}
      </ul>

      <div className="mb-4 grid grid-cols-1 gap-3 md:grid-cols-2">
        {metrics.map((m, i) => {
          const data = customSeriesResults[i]?.data ?? [];
          const series: MetricSeriesPoint[] = data.map((p) => ({
            period: p.period,
            value: p.value_micro,
            source_kind: "user_data",
            normalized_fact_id: -1,
          }));
          return (
            <div key={m.id}>
              <div className="mb-1 flex items-baseline gap-2">
                <span className="text-xs font-semibold">{m.label}</span>
                <DownloadCsvButton
                  request={{ type: "custom_metric_series", cik, metric_id: m.id, kind: chartKind }}
                  defaultFilename={`${ticker}_${m.key}_${chartKind}.csv`}
                />
              </div>
              <MetricChart series={series} unitKind={m.unit_kind} decimals={m.decimals} height={200} />
            </div>
          );
        })}
      </div>

      <section>
        <header className="mb-2 flex items-baseline gap-2">
          <h3 className="text-[11px] font-semibold uppercase tracking-wide text-muted">Compare</h3>
          <div className="flex gap-3 text-xs">
            {BUILTIN_COMPARE_OPTIONS.map((key) => (
              <label key={key} className="flex items-center gap-1.5">
                <input type="checkbox" checked={compareKeys.has(key)} onChange={() => toggleCompare(key)} />
                <span>{key}</span>
              </label>
            ))}
          </div>
          {namedSeries.length > 0 && (
            <DownloadCsvButton
              request={{ type: "comparison", cik, series: compareRefs, kind: chartKind }}
              defaultFilename={`${ticker}_comparison_${chartKind}.csv`}
            />
          )}
        </header>
        {namedSeries.length === 0 ? (
          <p className="text-xs text-muted">Select metrics above to compare them on one chart.</p>
        ) : (
          <ComparisonChart series={namedSeries} />
        )}
      </section>
    </>
  );
}
