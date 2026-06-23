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
| `EconProject_1.0.0_aarch64.dmg` | Ad-hoc signed macOS installer (no Apple Developer ID). Open, drag to Applications. |
| `fix-quarantine.sh` | One-shot script to strip macOS quarantine after transfer (REQUIRED — see §2.2). |
| `econ-project-source-<commit>.tar.gz` | Full source tarball at the demo commit, for inspection or rebuild. |
| `prd.pdf` | Product Requirements Document. |
| `architecture.pdf` | Architecture document. |
| `tech_spec.pdf` | Technical specification. |
| `DEMO_README.md` | This file. |

---

## 2. Install on the target Mac

### 2.1 Open the .dmg and copy the .app

1. Double-click `EconProject_1.0.0_aarch64.dmg`.
2. In the mounted Finder window, drag `EconProject.app` to `/Applications` (or any folder).
3. Eject the .dmg.

### 2.2 Strip the macOS quarantine attribute (REQUIRED)

The bundle is ad-hoc signed (linker-signed by the Rust toolchain, no Apple Developer ID) and **not** notarized by Apple. When you transfer the `.dmg` to the target Mac (AirDrop, USB, web download, email), macOS attaches a `com.apple.quarantine` attribute. Combined with the ad-hoc signature this triggers one of two Gatekeeper errors on first launch:

- "Apple cannot verify that this app is free of malware"
- **"EconProject.app has been modified or damaged. Move it to the Trash."**

Both are resolved the same way — strip the quarantine attribute. The simplest path is the bundled script (run it from the same folder where you copied `fix-quarantine.sh`):

```bash
bash ~/Desktop/econ-demo-bundle/fix-quarantine.sh
```

Or run the underlying command directly:

```bash
xattr -cr /Applications/EconProject.app
```

After this, double-click the `.app` to launch normally. No further unblock prompts.

**If you already saw the "modified or damaged" dialog**: dismiss it (do *not* click Move to Trash), run the command above, then re-launch. The app will start cleanly.

**If macOS will not let you run `xattr` either** (rare, only on locked-down corporate Macs): Right-click `EconProject.app` → **Open** → click **Open** in the warning dialog. If that also fails, open **System Settings → Privacy & Security → "EconProject was blocked…" → Open Anyway**.

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

- **Current market cap**: requires live Yahoo Finance access. Goes blank when offline; historical market cap (computed at ingestion) remains available offline. This is per FR-050.
- **8-K Item 4.02 unreliability warnings**: implemented but rarely triggered on common large-cap demo tickers — don't promise to demonstrate the warning live.
- **Multi-company comparison / peer benchmarking**: explicitly out of V1 scope (PRD §2.2).
- **CSV / PDF export**: out of V1 scope.
- **Test heuristic warning** in some lineage outputs: filing accession numbers carry the *filer's* CIK (often a filing agent), not the issuer's. Lineage data is correct; an internal test heuristic comments on this but it is not user-visible.

### Tickers known to work end-to-end

US issuers (10-K filers): AAPL, MSFT, GOOGL, AMZN, META, NVDA, TSLA, JPM, WMT, COST, WFC, CRM, V, MA, UNH, HD, PG, XOM, LLY, AVGO, KO, PEP, ORCL, ADBE, NFLX, DIS, INTC, AMD, ABBV, CVX. Bank filers (JPM, WFC) exercise the bank-revenue derivation chain.

Share-class tickers: BRK.B / BRK-B (Berkshire Hathaway B), BF.A / BF-A (Brown-Forman A) — both separator forms resolve.

Foreign private issuers (20-F filers): BABA (Alibaba, March year-end), and similar non-US issuers. Fiscal year end is derived from the latest annual filing when SEC's submissions endpoint omits it.

---

## 5. Recovery steps during demo

| Symptom | Action |
|---|---|
| "Apple cannot verify…" or "modified or damaged" on first launch | Run `bash ~/Desktop/econ-demo-bundle/fix-quarantine.sh` (or `xattr -cr /Applications/EconProject.app`), then relaunch. |
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
# Output: src-tauri/target/release/bundle/dmg/EconProject_1.0.0_aarch64.dmg
```

Requires: Node 20+, Rust 1.77+, Xcode Command Line Tools (`xcode-select --install`). Build takes 5-15 minutes.
