import { useParams } from "react-router-dom";

export default function CompanyDashboardPage() {
  const { ticker } = useParams<{ ticker: string }>();
  return (
    <div className="mx-auto max-w-6xl px-6 py-6">
      <header className="mb-4">
        <div className="font-mono text-xl">{ticker}</div>
        <div className="text-xs text-muted">
          Dashboard pending: SummaryWidgets / ChartGrid / StatementsTable will land
          once the ingestion pipeline (M22) is wired up.
        </div>
      </header>
      <section className="rounded border border-border/60 bg-surface p-4 text-sm">
        <h2 className="mb-2 font-semibold">Status</h2>
        <p className="text-muted">
          The data path for {ticker} requires the full V1 ingestion slice
          (Discover → Download → Parse → Normalize → Persist). The Rust core
          and persistence layer are in place; the source clients and pipeline
          stages are next.
        </p>
      </section>
    </div>
  );
}
