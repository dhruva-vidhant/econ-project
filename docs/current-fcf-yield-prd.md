# PRODUCT REQUIREMENTS DOCUMENT (PRD)

# CURRENT FREE CASH FLOW YIELD

**Status:** Approved for V1.0.0
**Companions:** `docs/current-fcf-yield-design.md`, `docs/current-fcf-yield-techspec.md`
**Parent PRD:** `docs/prd.md` (Local-First Financial Analysis Application — V1)

---

## 1. Summary

The application already derives a **period-end free cash flow yield** time series: for each fiscal period, the trailing free cash flow divided by the market capitalization measured **at that period's close**. That series is correct and auditable, but every point in it is anchored to a historical date. A user who wants to know "what is this company yielding *right now*, at today's price?" cannot answer that question from a period-end series, because the most recent point is anchored to the last reported period end — which may be months old and priced at a stock level that no longer exists.

This feature adds a single, prominent, **live scalar**: the **current free cash flow yield**, defined as the latest trailing-twelve-month free cash flow divided by the company's market capitalization computed from the **current spot price**. It is surfaced as an attention-drawing "live hero card" at the top of the company dashboard, with an explicit on-demand **Refresh price** action that fetches the current quote from the market-data provider.

---

## 2. Problem

A concrete worked example surfaced the gap. For Lululemon (LULU) FY2026:

- Numerator — trailing free cash flow: **$1.395B**.
- Denominator — market cap at the FY2026 close: **$20.76B** (the FY2026 closing price of $174.50 × 118.981M basic shares).
- Period-end free cash flow yield: **≈ 6.7%**.

That 6.7% is *arithmetically correct* but answers a question about the past. By the time the user is looking at the dashboard, LULU's market cap is **under $12B** — the stock roughly halved since the FY2026 close. At today's price the same trailing free cash flow yields **≈ 11.8%**, nearly double the period-end figure. A value investor scanning for yield needs the *current* number, computed against *today's* price, not the stale period-end number.

The period-end series is the right tool for studying valuation history. It is the wrong tool for the question "is this cheap **today**?" That question requires a live denominator.

---

## 3. Goals

- **G1.** Display a single current free cash flow yield figure: latest trailing-twelve-month free cash flow ÷ market cap at the current spot price.
- **G2.** Let the user pull a fresh quote on demand with a clearly labelled action that performs a live callout to the market-data provider.
- **G3.** Make the figure visually prominent and unmistakably distinct from the historical period-end metrics, and label its freshness honestly (how old is the price?).
- **G4.** Never fabricate. If any required input is missing — no stored price, no reported shares, no trailing free cash flow — show nothing rather than a misleading number.
- **G5.** Preserve the local-first, offline-after-ingest guarantee: the figure must be readable with **zero network access**; only the explicit Refresh price action touches the network.

---

## 4. Non-goals

- **N1.** Streaming or auto-polling live prices. The quote updates only when the user clicks Refresh price (or during a full company refresh). No background price ticker.
- **N2.** Intraday history, candles, or a price chart. This feature persists exactly one spot price per company — the latest.
- **N3.** Non-USD price handling. The provider guard rejects non-USD quotes, consistent with the existing market-cap feature.
- **N4.** A new valuation model. The numerator selection (trailing-twelve-month free cash flow with annual fallback) and the market-cap formula are reused unchanged from the existing derived-metric layer.
- **N5.** Replacing the period-end free cash flow yield series. Both coexist; they answer different questions.

---

## 5. Users & use cases

The user is the financially sophisticated individual investor described in the parent PRD. The driving use case:

> "I follow a watchlist of companies on free cash flow yield. When I open one, I want to see — at a glance, against today's price — what it currently yields, and I want to be able to refresh that quote on the spot."

A secondary use case is **comparison against the last period-end close**: the card shows how far the current yield has moved from the most recent fiscal year's period-end yield, contextualizing whether the stock has re-rated since the last report.

---

## 6. User experience

### 6.1 Placement & prominence

The current free cash flow yield is presented as a **live hero card** mounted at the **top of the company dashboard**, above the grid of period-end summary widgets. It is deliberately styled to stand apart from those muted tiles: an accent-colored ring and a soft outer glow draw the eye to it as the single "live" element on the page.

### 6.2 Anatomy

- **Eyebrow label:** `CURRENT FREE CASH FLOW YIELD` (small, uppercase, muted).
- **Freshness badge (top-right):** a pulsing `● LIVE` pill when the stored price is fresh (under 24 hours old); a muted, non-pulsing `● QUOTED · <age>` pill when the price is older, so the card never over-claims liveness.
- **Hero number:** the yield, very large, in a monospaced tabular figure (e.g. `11.8%`). Colored green when positive, red when negative (a cash-burning company yields a valid negative number).
- **Delta chip:** a neutral, accent-colored chip comparing the current yield to the most recent annual period-end yield, e.g. `▲ 5.1 pts vs FY2026 close`. Neutral (not green/red) because the direction is contextual, not inherently good or bad.
- **Math sub-line:** the full computation spelled out — `Trailing twelve-month free cash flow $1.40B ÷ current market cap $11.84B`.
- **Freshness stamps:** the spot price and its timestamp; the basic share count and the period it was reported; the date through which trailing free cash flow is measured.
- **Refresh price action:** a button (distinct from the company-wide Refresh) that fetches the current quote live, persists it, and re-renders the card.

### 6.3 State matrix

| State | Behavior |
|---|---|
| Loading | Card shell with skeleton placeholders; neutral badge. |
| No data (any input missing) | Card shell with a muted prompt: "No live quote yet — click Refresh price to fetch the current quote." A prominent Refresh price button. **No fabricated number.** |
| Refresh error | An inline error note; the last-known value (if any) stays visible. |
| Success | Full hero as described above. |

### 6.4 Accessibility

- The card is a labelled landmark (`aria-label`).
- The pulsing animation respects `prefers-reduced-motion`.
- Sign and direction are conveyed by text and glyphs, not color alone.

---

## 7. Functional requirements

- **FR1.** The current free cash flow yield = latest trailing-twelve-month free cash flow ÷ current market cap, where current market cap = current spot price × latest reported basic shares.
- **FR2.** The numerator is the most recent **quarterly** trailing-twelve-month free cash flow point; if no quarterly trailing point exists, fall back to the most recent **annual** free cash flow point. "Most recent" is by period end date.
- **FR3.** Shares are the latest reported basic shares outstanding, by maximum period end date across annual and quarterly facts.
- **FR4.** The spot price is **persisted** when fetched and **derived at read time**. Read paths perform **no** network I/O.
- **FR5.** Exactly one current spot price is stored per company; a new fetch overwrites it.
- **FR6.** Refresh price performs a live callout to the market-data provider, persists the returned quote, and returns the recomputed valuation.
- **FR7.** A full company refresh also opportunistically updates the spot price (best-effort; a price-fetch failure never fails ingestion).
- **FR8.** Return nothing (no card value) when any required input is missing or when the derived market cap is non-positive.

---

## 8. Success criteria

- **S1.** For LULU with a refreshed quote, the card shows a current free cash flow yield computed against the current price (≈ 11.8% at a sub-$12B market cap), distinct from the ≈ 6.7% period-end figure.
- **S2.** With network disabled, opening a previously-refreshed company still renders the card from the stored price (offline guarantee).
- **S3.** A company with no stored price shows the "No live quote yet" prompt — never a fabricated yield.
- **S4.** Clicking Refresh price updates the displayed number and the freshness stamps without a full page reload.
- **S5.** The feature ships as the **V1.0.0** release: the app is considered feature-complete for 1.0 after this addition.

---

## 9. Release

This feature is the **V1.0.0** capstone. The version is bumped from 0.1.0 to 1.0.0 across the Rust crate, the Tauri bundle config, the npm package, the in-app footer, and the packaged installer, and a fresh signed-adhoc DMG is built. See the design doc §7 for the version-touchpoint inventory.
