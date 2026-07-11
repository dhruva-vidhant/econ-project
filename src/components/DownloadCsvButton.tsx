import { save } from "@tauri-apps/plugin-dialog";
import { useState } from "react";

import { exportCsv } from "@/api/client";
import type { ExportRequest } from "@/api/types";

interface Props {
  request: ExportRequest;
  defaultFilename: string;
}

/** Backend-generated CSV export (§7, FR-EX-001/002/003). Never routes values
 * through JS numbers — the Rust side renders the file bytes. */
export default function DownloadCsvButton({ request, defaultFilename }: Props) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const onClick = async () => {
    setError(null);
    setPending(true);
    try {
      const path = await save({ defaultPath: defaultFilename, filters: [{ name: "CSV", extensions: ["csv"] }] });
      if (!path) return;
      await exportCsv(request, path);
    } catch (e) {
      setError((e as { detail?: { message?: string } })?.detail?.message ?? "Export failed.");
    } finally {
      setPending(false);
    }
  };

  return (
    <div className="inline-flex items-center gap-2">
      <button
        className="rounded border border-border/60 px-2 py-1 text-xs hover:bg-bg disabled:opacity-50"
        onClick={onClick}
        disabled={pending}
      >
        {pending ? "Exporting…" : "Download CSV"}
      </button>
      {error && <span className="text-[11px] text-bad">{error}</span>}
    </div>
  );
}
