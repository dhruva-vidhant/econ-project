import { useState } from "react";
import { Link } from "react-router-dom";

import { useAddCompany, useCompanies, useRemoveCompany } from "@/state/queries";

export default function HomePage() {
  const { data, isLoading, error } = useCompanies();
  const add = useAddCompany();
  const remove = useRemoveCompany();
  const [ticker, setTicker] = useState("");

  return (
    <div className="mx-auto max-w-3xl px-6 py-8">
      <h1 className="mb-4 text-lg font-semibold">Saved companies</h1>

      <form
        className="mb-6 flex gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          if (!ticker.trim()) return;
          add.mutate(ticker.trim().toUpperCase());
        }}
      >
        <input
          className="flex-1 rounded border border-border/60 bg-surface px-3 py-1.5 font-mono text-sm focus:outline-none focus:ring-1 focus:ring-accent"
          placeholder="Ticker (e.g., AAPL)"
          value={ticker}
          onChange={(e) => setTicker(e.target.value)}
        />
        <button
          type="submit"
          className="rounded border border-accent/60 bg-accent/15 px-3 py-1.5 text-sm hover:bg-accent/25"
          disabled={add.isPending}
        >
          {add.isPending ? "Adding…" : "Add"}
        </button>
      </form>

      {add.isError && (
        <div className="mb-4 rounded border border-bad/40 bg-bad/10 p-3 text-xs text-bad">
          {(add.error as { detail?: { message?: string } })?.detail?.message ?? "Failed to add company."}
        </div>
      )}

      {isLoading && <p className="text-sm text-muted">Loading…</p>}
      {error && <p className="text-sm text-bad">Failed to load companies.</p>}

      <ul className="divide-y divide-border/40">
        {(data ?? []).map((c) => (
          <li key={c.cik} className="flex items-center py-2">
            <Link
              to={`/c/${c.ticker}`}
              className="flex-1 text-sm hover:text-accent"
            >
              <span className="font-mono mr-3">{c.ticker}</span>
              <span className="text-muted">{c.name}</span>
            </Link>
            <button
              className="text-xs text-muted hover:text-bad"
              onClick={() => remove.mutate({ cik: c.cik, dropCache: false })}
            >
              remove
            </button>
          </li>
        ))}
        {(data ?? []).length === 0 && !isLoading && (
          <li className="py-4 text-sm text-muted">
            No companies yet. Add a ticker above to get started.
          </li>
        )}
      </ul>
    </div>
  );
}
