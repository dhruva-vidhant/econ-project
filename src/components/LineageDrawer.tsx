import { useEffect } from "react";

import { fmtUsdCompact, microToUsd, prettyMetric } from "@/api/types";
import { useLineage } from "@/state/queries";

interface Props {
  normalizedFactId: number | null;
  onClose: () => void;
}

/** Lineage side drawer (M40). Shows filing accession, form, date, XBRL concept,
 * raw value, FX conversion, and supersession chain for a normalized fact. */
export default function LineageDrawer({ normalizedFactId, onClose }: Props) {
  const { data, isLoading, error } = useLineage(normalizedFactId ?? undefined);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  if (normalizedFactId === null) return null;

  return (
    <div
      className="fixed inset-y-0 right-0 z-40 w-[420px] overflow-y-auto border-l border-border/60 bg-surface shadow-xl"
      role="complementary"
      aria-label="Lineage details"
    >
      <header className="sticky top-0 flex items-center border-b border-border/60 bg-surface px-4 py-2">
        <h2 className="text-sm font-semibold">Lineage</h2>
        <button className="ml-auto text-xs text-muted hover:text-text" onClick={onClose} aria-label="Close lineage drawer">
          close ✕
        </button>
      </header>

      {isLoading && <p className="p-4 text-sm text-muted">Loading…</p>}
      {error && (
        <p className="p-4 text-xs text-bad">
          {(error as { detail?: { message?: string } })?.detail?.message ?? "Failed to load lineage."}
        </p>
      )}

      {data && (
        <div className="space-y-4 p-4 text-xs">
          <Section title="Current value">
            <Row label="Metric" value={prettyMetric(data.primary.metric)} />
            <Row label="Value" value={fmtUsdCompact(data.primary.value)} />
            <Row label="Unit" value={data.primary.unit} />
            <Row label="Source kind" value={data.primary.source_kind} />
            <Row label="Is primary" value={data.primary.is_primary ? "yes" : "no"} />
          </Section>

          <Section title="Source filing">
            <Row label="Form" value={data.filing.form_type} />
            <Row label="Accession" value={<code className="font-mono">{data.filing.accession_no}</code>} />
            <Row label="Filed" value={data.filing.filed_at} />
            <Row label="Period of report" value={data.filing.period_of_report ?? "—"} />
            {data.filing.is_amendment && <Row label="Amendment" value="yes" />}
            {data.filing.item_4_02_8k && <Row label="Item 4.02 8-K" value="yes" />}
          </Section>

          <Section title="Source XBRL concept">
            <Row label="Taxonomy" value={data.raw_fact.taxonomy} />
            <Row label="Concept" value={<code className="font-mono">{data.raw_fact.concept}</code>} />
            <Row label="Unit" value={data.raw_fact.unit} />
            <Row label="Period" value={`${data.raw_fact.period_start ?? "—"} → ${data.raw_fact.period_end}`} />
            <Row label="Raw value (micro-units)" value={data.raw_fact.value_numeric.toLocaleString()} />
            <Row label="Raw value (display)" value={fmtUsdCompact(data.raw_fact.value_numeric)} />
            <Row label="Fiscal" value={`FY${data.raw_fact.fy ?? "—"} ${data.raw_fact.fp ?? ""}`.trim()} />
          </Section>

          {data.primary.fx_rate_micro && (
            <Section title="FX conversion">
              <Row label="Original" value={`${fmtUsdCompact(data.primary.original_value ?? 0)} ${data.primary.original_unit ?? ""}`} />
              <Row label="Rate" value={`${microToUsd(data.primary.fx_rate_micro)} on ${data.primary.fx_rate_date}`} />
              <Row label="Source" value={data.primary.fx_rate_source ?? "—"} />
            </Section>
          )}

          {data.supersession_chain.length > 0 && (
            <Section title={`Supersession history (${data.supersession_chain.length})`}>
              <ol className="space-y-1 pl-4">
                {data.supersession_chain.map((s) => (
                  <li key={s.id}>
                    <code className="font-mono">id={s.id}</code> — {fmtUsdCompact(s.value)}{" "}
                    <span className="text-muted">via {s.source_kind}</span>
                  </li>
                ))}
              </ol>
            </Section>
          )}
        </div>
      )}
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section>
      <h3 className="mb-1 text-[11px] font-semibold uppercase tracking-wide text-muted">{title}</h3>
      <div className="rounded border border-border/40 bg-bg/40 p-2">{children}</div>
    </section>
  );
}

function Row({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="flex justify-between gap-4 py-0.5">
      <span className="text-muted">{label}</span>
      <span className="text-right">{value}</span>
    </div>
  );
}
