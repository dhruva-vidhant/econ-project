import { useState } from "react";

import type { CustomMetric, PeriodKind, UnitKind } from "@/api/types";
import {
  useCreateCustomMetric,
  useUpdateCustomMetric,
  useUpsertCustomMetricPoint,
} from "@/state/queries";

interface Props {
  cik: string;
  metric?: CustomMetric;
  onClose: () => void;
}

/** Create/edit a custom metric and manually add points (§9.2, FR-CM-001/005). */
export default function CustomMetricEditor({ cik, metric, onClose }: Props) {
  const [key, setKey] = useState(metric?.key ?? "");
  const [label, setLabel] = useState(metric?.label ?? "");
  const [unitKind, setUnitKind] = useState<UnitKind>(metric?.unit_kind ?? "currency");
  const [currency, setCurrency] = useState(metric?.currency ?? "USD");
  const [decimals, setDecimals] = useState(metric?.decimals ?? 2);

  const [pointPeriod, setPointPeriod] = useState("");
  const [pointKind, setPointKind] = useState<PeriodKind>("annual");
  const [pointValue, setPointValue] = useState("");

  const create = useCreateCustomMetric(cik);
  const update = useUpdateCustomMetric(cik);
  const upsertPoint = useUpsertCustomMetricPoint(cik);

  const save = () => {
    if (metric) {
      update.mutate(
        { id: metric.id, patch: { label, unit_kind: unitKind, currency, decimals } },
        { onSuccess: onClose },
      );
    } else {
      create.mutate(
        { key, label, unit_kind: unitKind, currency, decimals },
        { onSuccess: onClose },
      );
    }
  };

  const addPoint = () => {
    if (!metric) return;
    upsertPoint.mutate({ metricId: metric.id, period: pointPeriod, kind: pointKind, value: pointValue });
    setPointPeriod("");
    setPointValue("");
  };

  const pending = create.isPending || update.isPending;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50" role="dialog" aria-label="Custom metric editor">
      <div className="w-[480px] rounded border border-border/60 bg-surface p-4">
        <header className="mb-3 flex items-baseline">
          <h2 className="text-sm font-semibold">{metric ? "Edit" : "New"} custom metric</h2>
          <button className="ml-auto text-xs text-muted hover:text-text" onClick={onClose} aria-label="Close editor">
            close ✕
          </button>
        </header>

        <div className="mb-3 space-y-2 text-xs">
          {!metric && (
            <Field label="Key">
              <input
                className="w-full rounded border border-border/60 bg-bg px-2 py-1 text-text"
                value={key}
                onChange={(e) => setKey(e.target.value)}
                placeholder="tpv"
              />
            </Field>
          )}
          <Field label="Label">
            <input
              className="w-full rounded border border-border/60 bg-bg px-2 py-1 text-text"
              value={label}
              onChange={(e) => setLabel(e.target.value)}
              placeholder="Total Payment Volume"
            />
          </Field>
          <Field label="Unit kind">
            <select
              className="w-full rounded border border-border/60 bg-bg px-2 py-1 text-text"
              value={unitKind}
              onChange={(e) => setUnitKind(e.target.value as UnitKind)}
            >
              <option value="currency">currency</option>
              <option value="count">count</option>
              <option value="ratio">ratio</option>
              <option value="number">number</option>
            </select>
          </Field>
          {unitKind === "currency" && (
            <Field label="Currency">
              <input
                className="w-full rounded border border-border/60 bg-bg px-2 py-1 text-text"
                value={currency ?? ""}
                onChange={(e) => setCurrency(e.target.value)}
              />
            </Field>
          )}
          <Field label="Decimals">
            <input
              type="number"
              min={0}
              max={6}
              className="w-full rounded border border-border/60 bg-bg px-2 py-1 text-text"
              value={decimals}
              onChange={(e) => setDecimals(Number(e.target.value))}
            />
          </Field>
        </div>

        <button
          className="mb-4 rounded border border-accent/60 bg-accent/10 px-3 py-1 text-xs hover:bg-accent/20 disabled:opacity-50"
          onClick={save}
          disabled={pending || (!metric && !key) || !label}
        >
          {pending ? "Saving…" : "Save"}
        </button>

        {metric && (
          <section className="border-t border-border/40 pt-3">
            <h3 className="mb-2 text-[11px] font-semibold uppercase tracking-wide text-muted">Add a point</h3>
            <div className="flex items-end gap-2 text-xs">
              <Field label="Period">
                <input
                  className="w-28 rounded border border-border/60 bg-bg px-2 py-1 text-text"
                  value={pointPeriod}
                  onChange={(e) => setPointPeriod(e.target.value)}
                  placeholder="FY2024"
                />
              </Field>
              <Field label="Kind">
                <select
                  className="rounded border border-border/60 bg-bg px-2 py-1 text-text"
                  value={pointKind}
                  onChange={(e) => setPointKind(e.target.value as PeriodKind)}
                >
                  <option value="annual">annual</option>
                  <option value="quarterly">quarterly</option>
                </select>
              </Field>
              <Field label="Value">
                <input
                  className="w-28 rounded border border-border/60 bg-bg px-2 py-1 text-text"
                  value={pointValue}
                  onChange={(e) => setPointValue(e.target.value)}
                  placeholder="1234.56"
                />
              </Field>
              <button
                className="rounded border border-border/60 px-3 py-1 hover:bg-bg disabled:opacity-50"
                onClick={addPoint}
                disabled={!pointPeriod || !pointValue || upsertPoint.isPending}
              >
                Add
              </button>
            </div>
          </section>
        )}
      </div>
    </div>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="block text-muted">
      {label}
      <div className="mt-0.5">{children}</div>
    </label>
  );
}
