import { useParams } from "react-router-dom";

export default function MetricDrillPage() {
  const { ticker, metric } = useParams<{ ticker: string; metric: string }>();
  return (
    <div className="mx-auto max-w-6xl px-6 py-6">
      <header className="mb-3 flex items-baseline gap-3">
        <span className="font-mono text-lg">{ticker}</span>
        <span className="text-sm text-muted">/ {metric}</span>
      </header>
      <p className="text-sm text-muted">
        Per-metric history + lineage drawer (M40) lands with the ingestion pipeline.
      </p>
    </div>
  );
}
