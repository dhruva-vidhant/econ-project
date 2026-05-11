# Follow-up implementation work

What's deferred from the V1 implementation slice to a follow-up pass. Ordered by user-visible impact. Each item is a known gap, not a discovered surprise — interfaces and tests are designed to absorb these without architectural change.

## High priority — finish the V1 slice

1. **Tauri runtime wiring for the dev server.** The Rust core, schema, and frontend all build cleanly, but `pnpm tauri dev` end-to-end was not exercised in this session because launching a windowed Tauri app inside the agent harness isn't viable. A human running `pnpm install && pnpm tauri dev` on a Mac with the Rust toolchain should see the app launch and the ingestion + dashboard flow work.

2. **ECharts chart grid (M38).** The dashboard ships with custom-SVG sparklines; full annual/quarterly chart toggle with ECharts is a follow-up. The data path (`get_metric_history`) already exists.

3. **Statements table (M39).** Income / Balance / Cash flow tables driven by the `current_series` repo method.

4. **Lineage panel / drawer (M40).** Wire up `get_supersession_chain` in the UI. Lineage data is already produced by ingestion.

5. **Refresh button + progress events (M30).** The pipeline emits diagnostic events to the DB; the IPC event channel hasn't been wired to push them to the UI live.

## Medium priority — accuracy / coverage

6. **YTD-to-quarter derivation.** The orchestrator currently *skips* YTD-style facts (10-Q with start/end spanning >100 days) with an info-level diagnostic. Per architecture §8.2, V1 must derive Q3 = 9M YTD − H1 YTD when the single-quarter value isn't reported. Module: extend `pipeline::orchestrator` (or split `M19 period reconciliation` into its own module).

7. **Supersession chain population on amendments.** The DB schema has `superseded_by` + cycle-protection triggers + `idx_norm_superseded_by`, and `NormalizedFactRepo::insert_primary_with_supersession` does the right thing. The pipeline's "most-recently-filed wins" heuristic produces the correct primary, but it doesn't yet write `superseded_by` chains for restated values. Wire the resolution rules from §8.5 into the persist stage.

8. **Amendment-coverage-gap detection (§8.5 step 4).** When a 10-K/A ingests but is silent on a concept the original tagged, insert an `amendment_coverage_gap` row + user-visible diagnostic.

9. **Concept-map alternates (`is_primary = 0`).** The current pipeline keeps only the winner per `(metric, period)`. Alternates should be persisted with `is_primary = 0` and surfaced in the lineage panel for transparency.

10. **8-K Item 4.02 HTML parser (M16 fallback).** The schema supports it (`restatement_announcement` + `restatement_resolved_by`). The parser itself, with the user's "must reliably extract affected periods regardless of phrasing" requirement, is real engineering work — likely 1–2 days alone.

## Lower priority — feature completion

11. **ECB FX bundled dataset + non-USD ingestion.** Schema supports it (`fx_rate` table + FX columns on `normalized_fact`). Adapter trait exists. Need: bundled CSV + ingestion path for non-USD filers.

12. **Yahoo Finance market-data adapter (M17 real impl).** Trait exists; impl currently a stub. Historical-market-cap requires the historical price series.

13. **Diagnostics tab (M41).** Route renders an empty state. Wire `useEvents` to render a filterable table.

14. **FYE-change banner (§11.2).** Logic to detect a company's `fiscal_year_end` changing across history; UI banner.

15. **52/53-week detection.** Schema column `is_53_week` exists; periods reconciliation should set it via `Period::detect_53_week` (already implemented as a helper).

16. **XBRL XML fallback path.** `source_kind` is in the schema; only `xbrl_api` is currently produced. The XBRL XML parser is needed for the rare cases where companyfacts has a gap.

## Testing follow-ups

17. **E2E harness.** `tests/e2e/PLAN.md` documents 7 user scenarios. The Playwright + fake-IPC harness is described but not built.

18. **Per-industry golden fixtures.** AAPL is the canonical fixture. Need at least one bank, insurer, REIT, foreign-private issuer, and 53-week filer to gate releases against §18 risk-register coverage.

19. **Unit tests for repo error paths.** The repos have happy-path tests; FK violation and unique-constraint conflict paths are not yet covered.

## Operational follow-ups

20. **Code signing + notarization.** `tauri.conf.json` is configured for bundle output; code-signing identity setup is required before any external `.dmg` distribution.

21. **App icons.** Placeholder PNGs are 32×32 / 128×128 / 128×128@2x dark-blue rectangles. Real artwork needed before release.

22. **CI.** GitHub Actions workflow not yet created. Should run `cargo test --workspace`, `cargo clippy`, `pnpm test`, `pnpm build`, and the integration test (with `--ignored` enabled in CI only) on a macOS runner.
