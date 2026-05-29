# EconProject V1 — Demo README

Local-first SEC-derived financial-analysis app. This document covers install on a fresh Apple Silicon Mac, demo flow, known limitations, and recovery steps.

---

## 0. Target-Mac prerequisites

- **Apple Silicon Mac** (M1 / M2 / M3 / M4) running macOS 13 Ventura or later (tested on Sequoia 15).
- ~500 MB free disk space (binary + SEC filings cache + SQLite database).
- Internet connection during ingestion (SEC EDGAR + Yahoo Finance for market cap). Demo runs offline once data is ingested.
- No Xcode / no developer tools required for the binary install path.

---

## 1. Bundle contents

The transfer bundle (`econ-demo-bundle/`) contains:

| File | Purpose |
|---|---|
| `EconProject_0.1.0_aarch64.dmg` | Ad-hoc signed macOS installer (no Apple Developer ID). Open, drag to Applications. |
| `econ-project-source-<commit>.tar.gz` | Full source tarball at the demo commit, for inspection or rebuild. |
| `prd.pdf` | Product Requirements Document. |
| `architecture.pdf` | Architecture document. |
| `tech_spec.pdf` | Technical specification. |
| `DEMO_README.md` | This file. |

---

## 2. Install on the target Mac

### 2.1 Open the .dmg and copy the .app

1. Double-click `EconProject_0.1.0_aarch64.dmg`.
2. In the mounted Finder window, drag `EconProject.app` to `/Applications` (or any folder).
3. Eject the .dmg.

### 2.2 Strip the macOS quarantine attribute (REQUIRED)

The bundle is ad-hoc signed (linker-signed by the Rust toolchain, no Apple Developer ID) and **not** notarized by Apple, so macOS Gatekeeper will block first launch with "Apple cannot verify…". Clear the quarantine bit before launching:

```bash
xattr -cr /Applications/EconProject.app
```

After this, double-click the `.app` to launch normally. No further unblock prompts.

**Recovery path if you skip the `xattr` step:** Right-click `EconProject.app` → **Open** → click **Open** in the warning dialog. macOS may also surface this as **System Settings → Privacy & Security → "EconProject was blocked…" → Open Anyway**. After the first manual override, the app launches normally on subsequent runs.

### 2.3 First launch

On first launch the app:
- Creates `~/Library/Application Support/com.econproject.app/data.sqlite`.
- Applies all schema migrations.
- Shows an empty home screen with the ticker-add field.

No internet activity happens until you add a ticker.

---

## 3. Demo flow (7 steps, ~5 minutes)

### Step 1 — Cold start

Launch `EconProject.app` from Applications. The home screen loads with an empty saved-companies list and a ticker-add field. Mention: "Local-only. No backend, no account."

### Step 2 — Add `AAPL`

Type `AAPL` into the add field and press **Add**. An ingest progress indicator appears. The status bar reports filing discovery → download → parse → normalize → persist phases. Initial ingestion takes 30-90 seconds depending on network. Mention: "Pulling SEC EDGAR `companyfacts` plus 10-K/10-Q metadata. Everything is cached locally — close your laptop afterwards and the data stays."

### Step 3 — View the dashboard

Click `AAPL` in the saved-companies list. The dashboard shows:
- **Headline widgets**: revenue, net income, total debt, free cash flow, historical market cap, cash. (Each widget displays the latest available value with a sparkline.)
- **Time-series chart**: switch between annual / quarterly modes to show ~20 years of revenue and net-income history.

Mention: "Free cash flow and total debt are derived locally with fixed formulas, computed at read time so the source data is never mutated."

### Step 4 — View statements

Click **Statements** in the navigation. Show the income statement, then the balance sheet, then the cash-flow statement for one fiscal year. Point out gross profit (a derived value when AAPL doesn't directly tag it) and capital expenditures.

### Step 5 — Drill into lineage

Click any single fact (a revenue cell, for example). The lineage drawer opens and shows:
- Source filing accession number, filing type (10-K / 10-Q / 10-K/A), filing date.
- XBRL concept name and the canonical metric it normalized to.
- Source preference indicator (XBRL vs derived vs text-fallback).

Mention: "Every datapoint traces back to the original SEC filing. Click the accession number and you'd open the actual filing in your browser."

### Step 6 — Add a second ticker (`JPM`)

Return to the home screen. Add `JPM`. After ingestion, click into JPM and show revenue. Mention: "JPM doesn't tag `us-gaap:Revenues` — banks report net interest income plus noninterest income. The system derived revenue from those components automatically. Click the revenue lineage to see the contributing concepts."

### Step 7 — Refresh

Return to AAPL. Click the **Refresh** button. The system checks SEC for newer filings, re-runs the ingest pipeline only on changed periods, and updates the dashboard cleanly. The refresh handler wipes per-ticker stale state before re-running so superseded periods don't linger.

---

## 4. Known limitations (avoid clicking these during demo)

- **Share-class tickers with `.` (e.g. `BRK.B`)**: ticker resolver does not handle the dot. Use a different ticker.
- **Current market cap**: requires live Yahoo Finance access. Goes blank when offline; historical market cap (computed at ingestion) remains available offline. This is per FR-050.
- **8-K Item 4.02 unreliability warnings**: implemented but rarely triggered on common large-cap demo tickers — don't promise to demonstrate the warning live.
- **Multi-company comparison / peer benchmarking**: explicitly out of V1 scope (PRD §2.2).
- **CSV / PDF export**: out of V1 scope.
- **Test heuristic warning** in some lineage outputs: filing accession numbers carry the *filer's* CIK (often a filing agent), not the issuer's. Lineage data is correct; an internal test heuristic comments on this but it is not user-visible.

---

## 5. Recovery steps during demo

| Symptom | Action |
|---|---|
| Gatekeeper blocks first launch | Run `xattr -cr /Applications/EconProject.app` in Terminal, relaunch. Or right-click → Open → Open. |
| SEC rate-limit (HTTP 429) during ingestion | Wait 60 seconds, then click **Refresh** or remove + re-add the ticker. EDGAR rate-limits at 10 req/s. |
| Network failure mid-ingestion | Ticker remains in saved list with partial data. Click Refresh once network returns; ingestion resumes. |
| Current market cap widget blank | Expected when offline or when Yahoo is unreachable. Demo continues; FR-050 only requires *historical* market cap offline. |
| App fails to launch / crashes | Quit fully (`⌘Q`), delete `~/Library/Application Support/com.econproject.app/data.sqlite`, relaunch. Note: this destroys ingested data — use only as last resort. |
| Demo Mac has no internet | Show pre-ingested data on the original demo Mac. Skip new-ticker steps. |

---

## 6. 30-second elevator framing

"V1 is a local-first SEC fundamentals app for sophisticated investors. It pulls SEC EDGAR filings on demand, normalizes the data into a stable schema, persists everything locally in SQLite, and lets you analyze ~20 years of statements offline. Every displayed value traces back to its source filing. No cloud, no account, no subscription — and accuracy is the hard constraint, not speed."

What V1 *is*: single-company analysis, historical statements, fixed derived metrics, time-series visualization, full traceability.

What V1 *is not* (per PRD §2.2): multi-company comparison, screening, portfolio management, valuation modeling, AI-generated insights, real-time quotes, cloud sync.

---

## 7. If something goes really wrong

The full source is in `econ-project-source-<commit>.tar.gz`. Rebuild from source on the target Mac:

```bash
tar -xzf econ-project-source-*.tar.gz
cd econ-project-*/
npm install
npm run tauri build
# Output: src-tauri/target/release/bundle/dmg/EconProject_0.1.0_aarch64.dmg
```

Requires: Node 20+, Rust 1.77+, Xcode Command Line Tools (`xcode-select --install`). Build takes 5-15 minutes.
