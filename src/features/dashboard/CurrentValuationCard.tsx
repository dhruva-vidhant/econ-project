import { fmtPercent, fmtUsdCompact, microToPercent } from "@/api/types";
import {
  useCurrentValuation,
  useMetricHistory,
  useRefreshPrice,
} from "@/state/queries";

interface Props {
  cik: string;
}

export default function CurrentValuationCard({ cik }: Props) {
  const query = useCurrentValuation(cik);
  const refreshPrice = useRefreshPrice();
  const annualFcfYield = useMetricHistory(cik, "free_cash_flow_yield", "annual");

  const fmtUsd2 = (priceMicro: number) => `$${(priceMicro / 1e6).toFixed(2)}`;
  const fmtShares = (shares: number) => {
    if (shares >= 1e9) return `${(shares / 1e9).toFixed(2)}B`;
    if (shares >= 1e6) return `${(shares / 1e6).toFixed(2)}M`;
    return shares.toLocaleString();
  };

  const v = query.data;

  if (query.isLoading) {
    return (
      <section
        aria-label="Current free cash flow yield"
        className="relative overflow-hidden rounded-lg border border-accent/40 bg-surface p-5 ring-1 ring-accent/40 shadow-[0_0_28px_-6px_rgba(86,156,214,0.55)]"
      >
        <div className="flex items-center">
          <div className="text-[11px] uppercase tracking-widest text-muted">
            CURRENT FREE CASH FLOW YIELD
          </div>
          <div className="ml-auto inline-flex items-center gap-1.5 rounded-full border border-border/60 bg-surface px-2.5 py-0.5 text-[11px] font-medium text-muted">
            —
          </div>
        </div>
        <div className="mt-3 space-y-2">
          <div className="h-12 w-32 animate-pulse rounded bg-muted/20" />
          <div className="h-4 w-48 animate-pulse rounded bg-muted/20" />
          <div className="h-3 w-64 animate-pulse rounded bg-muted/20" />
        </div>
      </section>
    );
  }

  if (!v) {
    return (
      <section
        aria-label="Current free cash flow yield"
        className="relative overflow-hidden rounded-lg border border-accent/40 bg-surface p-5 ring-1 ring-accent/40 shadow-[0_0_28px_-6px_rgba(86,156,214,0.55)]"
      >
        <div className="flex items-center">
          <div className="text-[11px] uppercase tracking-widest text-muted">
            CURRENT FREE CASH FLOW YIELD
          </div>
          <div className="ml-auto inline-flex items-center gap-1.5 rounded-full border border-border/60 bg-surface px-2.5 py-0.5 text-[11px] font-medium text-muted">
            —
          </div>
        </div>
        <div className="mt-3 text-sm text-muted">
          No live quote yet — click Refresh price to fetch the current quote.
        </div>
        <button
          className="mt-4 w-full rounded border border-accent/60 bg-accent/10 px-3 py-1 text-xs hover:bg-accent/20 disabled:opacity-50 md:w-auto"
          onClick={() => refreshPrice.mutate(cik)}
          disabled={refreshPrice.isPending}
        >
          {refreshPrice.isPending ? "Refreshing…" : "↻ Refresh price"}
        </button>
      </section>
    );
  }

  const now = new Date();
  const priceDate = new Date(v.price_as_of);
  const ageMs = now.getTime() - priceDate.getTime();
  const ageHours = ageMs / (1000 * 60 * 60);
  const ageDays = Math.floor(ageHours / 24);
  const isFresh = ageHours < 24;

  const latestAnnualYield = annualFcfYield.data?.[annualFcfYield.data.length - 1];
  let deltaChip: JSX.Element | null = null;
  if (latestAnnualYield) {
    const deltaPts =
      microToPercent(v.fcf_yield_micro) - microToPercent(latestAnnualYield.value);
    const arrow = deltaPts >= 0 ? "▲" : "▼";
    deltaChip = (
      <span className="inline-flex items-center gap-1 rounded border border-accent/40 bg-accent/10 px-2 py-0.5 text-xs text-accent">
        {arrow} {Math.abs(deltaPts).toFixed(1)} pts vs FY
        {latestAnnualYield.period.fiscal_year} close
      </span>
    );
  }

  const heroColor = v.fcf_yield_micro >= 0 ? "text-good" : "text-bad";

  return (
    <section
      aria-label="Current free cash flow yield"
      className="relative overflow-hidden rounded-lg border border-accent/40 bg-surface p-5 ring-1 ring-accent/40 shadow-[0_0_28px_-6px_rgba(86,156,214,0.55)]"
    >
      <div className="md:flex md:items-start md:justify-between">
        <div className="flex-1">
          <div className="flex items-center">
            <div className="text-[11px] uppercase tracking-widest text-muted">
              CURRENT FREE CASH FLOW YIELD
            </div>
            <div
              className={`ml-auto inline-flex items-center gap-1.5 rounded-full border px-2.5 py-0.5 text-[11px] font-medium ${
                isFresh
                  ? "border-good/40 bg-good/10 text-good"
                  : "border-border/60 bg-surface text-muted"
              }`}
            >
              {isFresh && (
                <span
                  aria-hidden
                  className="h-1.5 w-1.5 rounded-full bg-good motion-safe:animate-pulse"
                />
              )}
              {isFresh ? "LIVE" : `QUOTED · ${ageDays}d ago`}
            </div>
          </div>
          <div
            className={`mt-3 font-mono text-5xl font-semibold tabular-nums md:text-6xl ${heroColor}`}
            aria-label={`Current free cash flow yield ${fmtPercent(v.fcf_yield_micro)}, as of ${priceDate.toLocaleString()}`}
          >
            {fmtPercent(v.fcf_yield_micro)}
          </div>
          {deltaChip && <div className="mt-2">{deltaChip}</div>}
          <div className="mt-2 text-sm text-muted">
            Trailing twelve-month free cash flow {fmtUsdCompact(v.ttm_fcf_micro)} ÷
            current market cap {fmtUsdCompact(v.market_cap_micro)}
          </div>
          <div className="mt-3 flex flex-wrap gap-x-4 gap-y-1 text-[11px] text-muted">
            <span>
              Price {fmtUsd2(v.price_micro)} · as of {priceDate.toLocaleString()}
            </span>
            <span>
              Basic shares {fmtShares(v.shares)} · as of {v.shares_period_end}
            </span>
            <span>Free cash flow through {v.ttm_fcf_period_end}</span>
          </div>
        </div>
        <div className="mt-4 md:ml-4 md:mt-0">
          <button
            className="w-full rounded border border-accent/60 bg-accent/10 px-3 py-1 text-xs hover:bg-accent/20 disabled:opacity-50 md:w-auto"
            onClick={() => refreshPrice.mutate(cik)}
            disabled={refreshPrice.isPending}
          >
            {refreshPrice.isPending ? "Refreshing…" : "↻ Refresh price"}
          </button>
        </div>
      </div>
      {refreshPrice.isError && (
        <div className="mt-3 text-xs text-bad">
          {(refreshPrice.error as { detail?: { message?: string } })?.detail?.message ??
            "Could not fetch the current price."}
        </div>
      )}
    </section>
  );
}
