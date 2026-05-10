# PRODUCT REQUIREMENTS DOCUMENT (PRD)

# LOCAL-FIRST FINANCIAL ANALYSIS APPLICATION (V1)

---

# 1. PRODUCT OVERVIEW

## 1.1 Product Summary

The product is a local-first macOS desktop application designed for financially sophisticated individual investors who want deep access to structured company financial data derived primarily from SEC filings.

The application functions conceptually as a lightweight fundamentals-oriented Bloomberg terminal without any cloud-hosted backend infrastructure. All company data is ingested on demand, processed locally, and persisted entirely on the user’s machine.

The primary value proposition is:

- High-confidence financial data accuracy
- Deep historical coverage
- Full offline usability after ingestion
- Full traceability back to SEC filings
- Zero SaaS dependency

The system prioritizes correctness, auditability, and transparency over ingestion speed or flashy UI interactions.

---

## 1.2 Product Vision

Enable sophisticated investors to independently analyze public companies using authoritative SEC-derived structured data without relying on cloud services, subscription terminals, or opaque third-party financial aggregators.

---

## 1.3 V1 Product Scope

V1 focuses on:

- Single-company analysis
- Historical financial statements
- Derived metrics using fixed formulas
- Time-series visualization
- Local persistence
- Manual ingestion and refresh
- Offline-first workflows
- SEC traceability

V1 intentionally excludes collaborative, social, predictive, AI-assisted, or cloud-hosted capabilities.

---

## 1.4 Platform Scope

Primary target platform:

- macOS desktop

Acceptable implementation approaches:

- Native macOS application
- Electron/Tauri desktop wrapper
- Hybrid desktop architecture

The application must behave as a true desktop application with reliable local persistence and offline functionality.

---

# 2. GOALS AND NON-GOALS

## 2.1 Goals

### Core Goals

1. Deliver highly accurate financial statement data derived from SEC filings

2. Provide full offline usability for previously ingested companies

3. Enable historical analysis across long time periods

4. Support sophisticated financial workflows without educational simplification

5. Provide traceability for every displayed financial datapoint

6. Establish a modular architecture for future analytical expansion

---

## 2.2 Non-Goals (V1)

The following are explicitly excluded from V1:

### Analytical Features

- Multi-company comparison
- Peer benchmarking
- Screening
- Quantitative ranking systems
- Portfolio management
- Valuation modeling
- DCF generation
- AI-generated insights
- Earnings call analysis

### Data Features

- Real-time quotes
- Intraday market data
- News feeds
- Insider trading feeds
- Analyst estimates
- Alternative data

### Collaboration Features

- User accounts
- Cloud sync
- Shared dashboards
- Multi-device synchronization

### Customization Features

- User-defined formulas
- Plugin ecosystem
- Custom widgets

### Export Features

- CSV export
- PDF export
- Image export
- Spreadsheet integrations

---

# 3. TARGET USERS

## 3.1 Primary Users

### Sophisticated Individual Investors

Characteristics:

- Comfortable reading financial statements
- Familiar with accounting terminology
- Understand valuation concepts
- Prefer raw structured data over simplified summaries
- Require transparency into calculations

---

## 3.2 Secondary Users

### Financial Researchers

Individuals performing long-horizon company analysis who value:

- Historical consistency
- Offline accessibility
- Reproducibility
- Auditability

---

## 3.3 User Expectations

Users expect:

- High data accuracy
- Transparent calculations
- Stable historical records
- Minimal abstraction
- Fast navigation after ingestion
- Trustworthy financial lineage

Users do NOT expect:

- Beginner education
- Gamification
- Social interaction
- AI-generated narratives
- Brokerage integration

---

# 4. USER PERSONAS

## 4.1 Persona A — Long-Term Fundamental Investor

### Background

- Tracks 20–50 companies
- Performs deep annual analysis
- Values long historical trends
- Uses SEC filings directly today

### Pain Points

- Existing tools are expensive
- Data providers disagree
- SEC filings are cumbersome to navigate manually
- Offline workflows are difficult

### Goals

- Quickly inspect long-term company fundamentals
- Validate financial numbers directly against filings
- Analyze financial trends without subscriptions

---

## 4.2 Persona B — Independent Financial Researcher

### Background

- Builds personal investment theses
- Wants reproducible datasets
- Distrusts opaque aggregators

### Pain Points

- Hard to verify third-party financial data
- Derived metrics often lack traceability

### Goals

- Inspect source lineage for all metrics
- Build confidence in extracted data
- Retain complete local control over datasets

---

# 5. USER WORKFLOWS

## 5.1 Initial Company Ingestion Workflow

### Flow

1. User launches application
2. Home screen loads saved companies
3. User enters ticker symbol
4. System validates ticker
5. System downloads SEC filing metadata
6. System identifies relevant filings
7. System downloads filings
8. System parses structured filing data
9. System normalizes financial data
10. System stores normalized data locally
11. Dashboard becomes available

---

## 5.2 Company Analysis Workflow

### Flow

1. User selects saved company
2. Dashboard loads local data
3. User views:
   - Summary widgets
   - Financial tables
   - Charts
   - Derived metrics
4. User drills into metrics
5. User views filing traceability
6. User switches between annual and quarterly views

---

## 5.3 Refresh Workflow

### Flow

1. User presses refresh button
2. System checks for newer filings
3. System downloads updated filings
4. System re-processes affected data
5. Local database updates
6. Dashboard refreshes

---

## 5.4 Offline Workflow

### Flow

1. User launches app without internet
2. Previously ingested companies remain accessible
3. All dashboards, charts, and tables function normally
4. Refresh and new ingestion operations disabled

---

# 6. FUNCTIONAL REQUIREMENTS

# 6.1 Company Management

## Requirements

### FR-001 — Add Company

Users must be able to add companies by ticker symbol.

### FR-002 — Persist Saved Companies

Saved companies must persist across application restarts.

### FR-003 — Remove Company

Users must be able to remove companies from saved list.

### FR-004 — Cached Historical Data

Removing a company may optionally preserve cached data locally.

---

# 6.2 Financial Data Retrieval

## Requirements

### FR-010 — SEC Filing Discovery

System must identify:
- 10-K filings
- 10-Q filings
- Relevant 8-K filings

### FR-011 — Structured Filing Preference

System must prioritize:
1. XBRL/XML structured data
2. Structured SEC APIs
3. HTML/text parsing fallback

### FR-012 — Historical Depth

System must retrieve:
- Minimum 10 years
- Target 20 years

### FR-013 — Quarterly Coverage

Quarterly data must be supported where available.

---

# 6.3 Financial Statements

## Requirements

### FR-020 — Income Statement Support

Must support:
- Revenue
- Gross profit
- Operating income
- Net income
- EPS
- Shares outstanding

### FR-021 — Balance Sheet Support

Must support:
- Cash
- Debt
- Assets
- Liabilities
- Equity

### FR-022 — Cash Flow Statement Support

Must support:
- Operating cash flow
- Capital expenditures
- Depreciation/amortization
- Free cash flow inputs

---

# 6.4 Derived Metrics

## Requirements

### FR-030 — Fixed Derived Metrics

V1 supports predefined formulas only.

### FR-031 — Formula Transparency

Users must be able to inspect formulas used.

### FR-032 — Free Cash Flow

Must support FCF computation using predefined formula.

Free cash flow example:

FCF = Net Income − Depreciation & Amortization + Capital Expenditures

---

# 6.5 Financial Data Normalization

## Requirements

### FR-040 — Financial Data Normalization Layer

The system must normalize raw SEC filing data into a consistent internal financial representation suitable for accurate analysis, visualization, historical comparison, storage, and derived metric computation.

Normalization is a core architectural requirement of the platform and is essential to ensuring consistency across companies, filing periods, filing formats, and reporting conventions.

The normalization layer must address inconsistencies and variations including, but not limited to:

- Differences in XBRL taxonomy usage across companies
- Different labels for equivalent financial concepts
- Changes in company reporting conventions over time
- Unit inconsistencies (dollars, thousands, millions)
- Fiscal calendar differences
- Quarterly versus year-to-date reporting formats
- Sign convention inconsistencies
- Amended or restated filings
- Missing or partially structured data
- Filing-format differences between XML/XBRL and text-based filings

Examples:

- Revenue
- SalesRevenueNet
- NetSales

may all map internally to a canonical metric representation such as:

- revenue

The normalization layer must preserve traceability to the original filing source while simultaneously exposing a stable internal schema to downstream systems including:

- Dashboard widgets
- Charts
- Financial tables
- Derived metric engines
- Historical trend analysis
- Future comparison engines

The application must never silently discard normalization conflicts or ambiguities. Any unresolved inconsistencies should be surfaced transparently to the user or recorded in ingestion diagnostics.

Accuracy and consistency are prioritized over ingestion speed.

---

# 6.6 Dashboard and Visualization

## Requirements

### FR-050 — Summary Widgets

Dashboard must display:
- Revenue
- Net income
- Market cap
- Debt
- Derived metrics

### FR-051 — Historical Charts

Charts must support:
- Annual mode
- Quarterly mode

### FR-052 — Financial Tables

Tables must display:
- Historical periods
- Raw line items
- Drill-down details

### FR-053 — Drill-Down Navigation

Users must be able to navigate into supporting line items.

---

# 6.7 Traceability

## Requirements

### FR-060 — Source Filing Traceability

Every datapoint must map to:
- Filing
- Filing type
- Filing date

### FR-061 — Source Location Traceability

System should retain:
- XBRL concept
- Filing section
- Line references where feasible

### FR-062 — Source Preference Visibility

UI must indicate whether value originated from:
- XBRL/XML
- Parsed text
- External API

---

# 7. NON-FUNCTIONAL REQUIREMENTS

# 7.1 Reliability

- Local data persistence must be durable
- Corruption-resistant storage required
- Application crashes must not destroy datasets

---

# 7.2 Offline Capability

- Previously ingested companies fully accessible offline
- No cloud dependency for analysis workflows

---

# 7.3 Maintainability

Architecture must support:
- Additional financial metrics
- Additional data sources
- Future comparison features

---

# 7.4 Extensibility

System should support future:
- Formula engines
- Peer analysis
- Export systems
- Plugin systems

---

# 7.5 Observability

System should provide:
- Ingestion logs
- Parsing diagnostics
- Data lineage metadata

---

# 8. DATA MODEL OVERVIEW

# 8.1 Core Entities

## Company

Fields:
- Ticker
- CIK
- Company name
- Exchange

---

## Filing

Fields:
- Filing ID
- Filing type
- Filing date
- Source URL
- Raw filing path

---

## Financial Period

Fields:
- Fiscal year
- Fiscal quarter
- Start date
- End date

---

## Financial Metric

Fields:
- Metric name
- Value
- Currency
- Period
- Filing source
- Source confidence

---

## Derived Metric

Fields:
- Formula ID
- Inputs
- Computed value
- Computation timestamp

---

# 8.2 Storage Recommendations

Recommended local storage:
- SQLite
- DuckDB
- Embedded relational database

Raw filing storage:
- Local filesystem

---

# 9. SYSTEM CONSTRAINTS

## 9.1 No Backend Infrastructure

System cannot require:
- Cloud storage
- Central APIs
- Hosted databases

---

## 9.2 SEC Dependency Constraints

SEC filings may:
- Change formatting
- Contain inconsistent XBRL tags
- Include amended filings

System must tolerate these inconsistencies.

---

## 9.3 API Constraints

Yahoo/Google finance APIs may:
- Rate limit
- Change response formats
- Become unavailable

System must gracefully degrade.

---

# 10. UI/UX REQUIREMENTS

# 10.1 Design Philosophy

The UI should feel:

- Dense but readable
- Professional
- Analytical
- Efficient
- Minimalistic

Avoid:
- Excessive animations
- Consumer-style simplifications
- Educational overlays

---

# 10.2 Home Screen

Must include:
- Saved companies list
- Search/add ticker
- Recent refresh status

---

# 10.3 Dashboard Layout

Recommended layout:
- Top summary widgets
- Middle chart section
- Lower financial tables

---

# 10.4 Loading States

Must support:
- Ingestion progress
- Parsing progress
- Refresh progress

---

# 10.5 Error Visibility

Users must clearly see:
- Missing data
- Parsing failures
- Partial ingestion issues

---

# 11. DATA INGESTION AND PROCESSING

# 11.1 Ingestion Pipeline

## Step 1 — Filing Discovery

Discover relevant SEC filings.

---

## Step 2 — Filing Download

Download:
- XBRL
- XML
- Filing metadata

---

## Step 3 — Parsing

Extract:
- Financial concepts
- Time periods
- Units
- Filing context

---

## Step 4 — Normalization

Normalize:
- Fiscal periods
- Units
- Metric naming
- Sign conventions
- Filing taxonomy differences
- Reporting inconsistencies

The normalization stage is considered a critical subsystem of the platform rather than a lightweight transformation step.

---

## Step 5 — Persistence

Store:
- Raw filings
- Normalized metrics
- Traceability metadata

---

# 11.2 Data Normalization Challenges

System must handle:
- Different fiscal calendars
- Renamed XBRL concepts
- Stock splits
- Restatements
- Currency inconsistencies
- Partial filings
- Conflicting filing representations
- Quarterly versus cumulative reporting differences

---

# 11.3 Filing Priority Logic

Priority order:
1. Latest amended filing
2. Latest original filing
3. Structured data
4. Text fallback

---

# 12. ERROR HANDLING AND EDGE CASES

# 12.1 Missing Data

If data unavailable:
- Show explicit gaps
- Avoid fabricated estimates

---

# 12.2 Inconsistent Filings

If filings conflict:
- Prefer latest amendment
- Preserve audit trail

---

# 12.3 Partial Parsing Failures

System should:
- Continue ingesting valid sections
- Mark incomplete areas

---

# 12.4 Offline Errors

When offline:
- Disable refresh
- Preserve dashboard functionality

---

# 12.5 API Failures

If supplemental APIs fail:
- Continue SEC-based functionality
- Mark market data unavailable

---

# 13. PERFORMANCE CONSIDERATIONS

# 13.1 Performance Priorities

Priority order:
1. Accuracy
2. Stability
3. Traceability
4. Speed

---

# 13.2 Expected Latency

Acceptable:
- Multi-minute initial ingestion
- Noticeable refresh delays

Not acceptable:
- UI freezing
- Data corruption
- Silent failures

---

# 13.3 Caching

System should cache:
- Filing metadata
- Parsed financial statements
- Derived computations

---

# 14. SECURITY AND PRIVACY CONSIDERATIONS

# 14.1 Local-Only Data Model

All financial data stored locally.

No mandatory:
- Telemetry
- Cloud sync
- User accounts

---

# 14.2 Privacy Principles

Application should:
- Avoid unnecessary outbound requests
- Minimize third-party dependencies

---

# 14.3 Secure Storage

Recommended:
- Sandboxed storage
- Integrity-safe local database handling

---

# 15. FUTURE ENHANCEMENTS

# 15.1 V2 Enhancements

## Analytical Enhancements

- Multi-company comparison
- Peer benchmarking
- Multiple predefined formulas

## Data Enhancements

- Automatic refresh
- Saved watchlists
- Smarter caching

---

# 15.2 V3 Enhancements

## Intelligence Features

- News ingestion
- Earnings transcript integration
- AI-assisted exploration

## Export Features

- CSV export
- Spreadsheet interoperability
- Chart/image export

---

# 15.3 Long-Term Possibilities

Potential future areas:
- Portfolio overlays
- Local LLM integration
- Advanced screening
- Plugin SDK
- Formula editor
- Custom dashboards

