import { useParams } from "react-router-dom";

export default function DiagnosticsPage() {
  const { ticker } = useParams<{ ticker: string }>();
  return (
    <div className="mx-auto max-w-6xl px-6 py-6">
      <header className="mb-3 flex items-baseline gap-3">
        <span className="font-mono text-lg">{ticker}</span>
        <span className="text-sm text-muted">/ diagnostics</span>
      </header>
      <p className="text-sm text-muted">
        Diagnostics tab (M41) — `ingestion_event` rows filtered by stage / level.
      </p>
    </div>
  );
}
