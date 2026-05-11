# EconProject

Local-first macOS desktop application for sophisticated investors who want
deep, structured access to SEC-derived company financial data — a
fundamentals-oriented Bloomberg terminal without any cloud dependency.

See `docs/prd.md` for product requirements, `docs/architecture.md` for
the V1 architecture, and `docs/tech_spec.md` for the module decomposition.

## Repository status (V1 implementation pass)

What works end-to-end:

- Rust core compiles cleanly (`cargo check`); 42 unit tests + 1 ignored
  real-network integration test against SEC EDGAR pass.
- Frontend compiles cleanly (`pnpm build`); 5 unit tests pass.
- Real ingestion against the live SEC API: a single
  `add_company("AAPL")` call ingests 1000 filings, 24,852 raw XBRL facts,
  and 879 normalized facts in ~2 seconds (verified).
- Schema is the §6.3 architecture baseline including cycle-protection
  triggers, partial unique indexes, and FK cascades.
- IPC commands wired: `add_company`, `remove_company`, `list_companies`,
  `get_dashboard`, `get_metric_history`, `get_ingestion_events`,
  `get_supersession_chain`, `ping`.
- Dashboard renders summary widgets with sparklines + recent ingestion events.

What's deferred — see `docs/followup.md` for the prioritized list.

## Prerequisites

- macOS (the project targets macOS desktop only for V1)
- Rust 1.77+ (`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)
- Node 20+ (`brew install node` or download from nodejs.org)
- For Tauri: Xcode Command Line Tools (`xcode-select --install`)

## Run

```bash
# Install JS dependencies
npm install

# Run unit tests
cargo test --manifest-path src-tauri/Cargo.toml --lib
npm test

# Run the integration test against real SEC EDGAR (requires network)
cargo test --manifest-path src-tauri/Cargo.toml --test integration_apple -- --ignored --nocapture

# Build production frontend bundle
npm run build

# Launch the app in development (Tauri 2)
npm run tauri dev
```

The first `cargo check` / `cargo build` will pull and compile ~600 crates
(Tauri's transitive deps); expect 5–10 minutes on a cold start.

## Layout

```
docs/                         PRD, architecture, tech spec, follow-up
src-tauri/                    Rust core (Tauri 2)
  src/domain/                 Cross-module canonical types (M03)
  src/errors.rs               Typed error hierarchy (M04)
  src/db/                     Schema + connection pool (M02, M05)
  src/repos/                  Per-table repositories (M06–M12)
  src/sources/                SEC + market-data adapters (M13–M17)
  src/normalize/              Concept maps (M18–M21)
  src/pipeline/               Ingestion orchestrator (M22–M27)
  src/ipc/                    Tauri commands + state (M29–M30)
  tests/                      Integration tests
src/                          React frontend
  api/                        Typed IPC client + types (M32)
  state/                      TanStack Query hooks (M33)
  components/                 Generic UI primitives
  features/                   Per-feature pages (M35–M41)
.claude/agents/               Agent definitions for orchestrated work
tests/e2e/                    E2E test plan (M45)
```

## SEC compliance

The Rust core enforces a strict SEC fair-access posture:

- **User-Agent** required, configured at `SecClient::new(user_agent, …)`.
- **Host allowlist** for outbound HTTP (`data.sec.gov`, `www.sec.gov`,
  market-data adapter, ECB).
- **Rate limit** — token-bucket capped at 5 req/s by default (well under
  SEC's published 10 req/s ceiling).
- **No mass crawling** — fetches happen per-company on user action.

A short ticker→CIK fallback table is bundled for environments where
`www.sec.gov/files/company_tickers.json` is rate-limited; live ingestion
uses `data.sec.gov`, which does not have the same gating.

## License

TBD.
