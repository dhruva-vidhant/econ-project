# End-to-End Test Plan (M45)

These tests are written from the perspective of a **human user** clicking through the application. They drive the real Rust backend through the real Tauri IPC layer and assert against what the user sees on screen.

V1 ships scenarios 1–5 as automated tests; scenarios 6–7 are documented manual scripts pending the Item 4.02 / restatement features.

## Tooling

- **Harness:** Playwright in browser-emulation mode against the Vite dev server, with a thin "fake-IPC" shim that reroutes Tauri `invoke()` calls to a real Rust binary running `econ-project-headless`. (Tauri's `tauri-driver` is the alternative when a real Tauri window is required; for V1, the fake-IPC harness is sufficient.)
- **Fixtures:** see `tests/fixtures/FIXTURES.md`.

## Scenarios

### S1 — First-run flow

1. Launch the app. **Assert:** the home screen shows "Saved companies" with empty-state copy.
2. Type `AAPL` into the ticker input and click "Add". **Assert:** loading state appears; an `ingestion://progress/...` event fires; row `AAPL — Apple Inc.` appears in the list within 30 s.
3. Click on `AAPL`. **Assert:** dashboard renders with at least 3 summary widgets, each showing a non-zero value and a sparkline.

### S2 — Dashboard navigation

1. From a state where AAPL is ingested, navigate to `/c/AAPL`.
2. **Assert:** dashboard widgets show Revenue, Net Income, Cash, Total Assets (or a subset) with values formatted as `$XB` / `$YT`.
3. Click on the Revenue widget. **Assert:** the URL changes to `/c/AAPL/metric/Revenue`.
4. Click "← Saved companies" in the header. **Assert:** the URL is `/`.

### S3 — Refresh

1. From a state where AAPL is ingested, click the (future) Refresh button.
2. **Assert:** ingestion events surface in the recent-events strip; the dashboard re-renders without losing state; no duplicate companies appear.

(Pending UI: Refresh button is in the M22 follow-up alongside the orchestrator's progress channel.)

### S4 — Offline behavior

1. Pre-condition: AAPL ingested.
2. Disable network (Playwright `route.abort()` for `data.sec.gov`).
3. Reload the app. **Assert:** the home page lists `AAPL`. Navigating to `/c/AAPL` shows the dashboard with persisted data.
4. Click "Add" with a new ticker. **Assert:** ingestion fails with a clear, user-visible error message.

### S5 — Error flow

1. From the home screen, type `XYZNOPE` and click "Add".
2. **Assert:** an error banner renders with text including `unknown_ticker` and a human-readable message; the input does not get added to the list.

### S6 — Lineage correctness *(manual until lineage drawer ships)*

1. Ingest AAPL.
2. Navigate to `/c/AAPL/metric/Revenue`.
3. Click the latest row. **Manual assert:** lineage drawer shows accession `0000320193-{YY}-{NNNNNN}`, form `10-K`, the XBRL concept (`us-gaap:RevenueFromContractWithCustomerExcludingAssessedTax` for post-ASC 606 years), and the value preserved exactly.

### S7 — Restatement *(manual until 10-K/A ingestion + supersession UI)*

1. Use a fixture for a company that has filed a 10-K/A.
2. **Manual assert:** the chart shows the restated value for the affected period; the lineage drawer walks the supersession chain (`Original 10-K → 10-K/A`).

## Pass criteria

V1 is green when scenarios S1, S2, S5 pass automatically. S3 and S4 require additional UI surfacing (Refresh button, offline banner) that lands in the next implementation pass. S6 and S7 require the lineage drawer and restatement-aware UI.
