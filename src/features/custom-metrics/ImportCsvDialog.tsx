import { useState } from "react";

import type { ImportOptions, ImportReport, PeriodKind, UnitKind } from "@/api/types";
import { useImportCustomMetricsCsv } from "@/state/queries";

interface Props {
  cik: string;
  onClose: () => void;
}

/** CSV import dialog for custom metrics (§9.2, FR-CM-002/004). */
export default function ImportCsvDialog({ cik, onClose }: Props) {
  const [csvText, setCsvText] = useState("");
  const [granularity, setGranularity] = useState<PeriodKind>("annual");
  const [defaultUnit, setDefaultUnit] = useState<UnitKind>("currency");
  const [report, setReport] = useState<ImportReport | null>(null);
  const importCsv = useImportCustomMetricsCsv(cik);

  const handleFile = (file: File) => {
    const reader = new FileReader();
    reader.onload = () => setCsvText(String(reader.result ?? ""));
    reader.readAsText(file);
  };

  const submit = () => {
    const options: ImportOptions = { granularity, default_unit: defaultUnit };
    importCsv.mutate(
      { csvText, options },
      { onSuccess: (r) => setReport(r) },
    );
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50" role="dialog" aria-label="Import CSV">
      <div className="w-[520px] rounded border border-border/60 bg-surface p-4">
        <header className="mb-3 flex items-baseline">
          <h2 className="text-sm font-semibold">Import custom metrics CSV</h2>
          <button className="ml-auto text-xs text-muted hover:text-text" onClick={onClose} aria-label="Close import dialog">
            close ✕
          </button>
        </header>

        {!report && (
          <>
            <label className="mb-2 block text-xs text-muted">
              CSV file
              <input
                type="file"
                accept=".csv,text/csv"
                className="mt-1 block w-full text-xs"
                onChange={(e) => {
                  const f = e.target.files?.[0];
                  if (f) handleFile(f);
                }}
              />
            </label>

            <div className="mb-3 flex gap-4">
              <label className="text-xs text-muted">
                Default granularity
                <select
                  className="ml-2 rounded border border-border/60 bg-bg px-2 py-1 text-xs text-text"
                  value={granularity}
                  onChange={(e) => setGranularity(e.target.value as PeriodKind)}
                >
                  <option value="annual">annual</option>
                  <option value="quarterly">quarterly</option>
                </select>
              </label>
              <label className="text-xs text-muted">
                Default unit
                <select
                  className="ml-2 rounded border border-border/60 bg-bg px-2 py-1 text-xs text-text"
                  value={defaultUnit}
                  onChange={(e) => setDefaultUnit(e.target.value as UnitKind)}
                >
                  <option value="currency">currency</option>
                  <option value="count">count</option>
                  <option value="ratio">ratio</option>
                  <option value="number">number</option>
                </select>
              </label>
            </div>

            {importCsv.isError && (
              <p className="mb-2 text-xs text-bad">
                {(importCsv.error as { detail?: { message?: string } })?.detail?.message ?? "Import failed."}
              </p>
            )}

            <button
              className="rounded border border-accent/60 bg-accent/10 px-3 py-1 text-xs hover:bg-accent/20 disabled:opacity-50"
              onClick={submit}
              disabled={!csvText || importCsv.isPending}
            >
              {importCsv.isPending ? "Importing…" : "Import"}
            </button>
          </>
        )}

        {report && <ImportSummary report={report} onDone={onClose} />}
      </div>
    </div>
  );
}

function ImportSummary({ report, onDone }: { report: ImportReport; onDone: () => void }) {
  return (
    <div className="text-xs">
      <ul className="mb-3 space-y-1">
        <li>Metrics created: {report.metrics_created.length ? report.metrics_created.join(", ") : "none"}</li>
        <li>Imported: {report.imported}</li>
        <li>Updated: {report.updated}</li>
        <li>Unaligned (no matching period yet): {report.unaligned}</li>
        <li>Rejected: {report.rejected.length}</li>
      </ul>
      {report.rejected.length > 0 && (
        <div className="mb-3 max-h-40 overflow-y-auto rounded border border-bad/40 bg-bad/10 p-2">
          {report.rejected.map((r) => (
            <div key={r.row_index} className="border-b border-bad/20 py-1 last:border-b-0">
              row {r.row_index}: {r.reason} — <code className="font-mono">{r.raw}</code>
            </div>
          ))}
        </div>
      )}
      <button className="rounded border border-border/60 px-3 py-1 hover:bg-bg" onClick={onDone}>
        Done
      </button>
    </div>
  );
}
