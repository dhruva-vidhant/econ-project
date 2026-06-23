-- M02. Initial schema. Faithful to docs/architecture.md §6.3.
-- All currency / per-share values stored as INTEGER micro-units (×1,000,000).
-- Share counts: absolute integers.

CREATE TABLE IF NOT EXISTS company (
  cik             TEXT PRIMARY KEY,
  ticker          TEXT NOT NULL,
  name            TEXT NOT NULL,
  exchange        TEXT,
  sic             TEXT,
  fiscal_year_end TEXT,                       -- MMDD
  added_at        TEXT NOT NULL,
  last_refreshed  TEXT
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_company_ticker ON company(ticker);

CREATE TABLE IF NOT EXISTS filing (
  accession_no    TEXT PRIMARY KEY,
  cik             TEXT NOT NULL REFERENCES company(cik) ON DELETE RESTRICT,
  form_type       TEXT NOT NULL,
  filed_at        TEXT NOT NULL,
  period_of_report TEXT,
  is_amendment    INTEGER NOT NULL DEFAULT 0,
  amends          TEXT,
  item_4_02_8k    INTEGER NOT NULL DEFAULT 0,
  source_url      TEXT,
  raw_path        TEXT
);
CREATE INDEX IF NOT EXISTS idx_filing_cik_filed ON filing(cik, filed_at DESC);

CREATE TABLE IF NOT EXISTS period (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  cik             TEXT NOT NULL REFERENCES company(cik) ON DELETE RESTRICT,
  fiscal_year     INTEGER NOT NULL,
  fiscal_quarter  INTEGER NOT NULL,           -- 0 = annual, 1..4 = quarterly
  fiscal_year_end TEXT NOT NULL,              -- MMDD
  start_date      TEXT NOT NULL,
  end_date        TEXT NOT NULL,
  kind            TEXT NOT NULL,
  is_53_week      INTEGER NOT NULL DEFAULT 0,
  CHECK (fiscal_quarter BETWEEN 0 AND 4),
  CHECK ((fiscal_quarter = 0 AND kind = 'annual') OR
         (fiscal_quarter BETWEEN 1 AND 4 AND kind = 'quarterly')),
  UNIQUE (cik, fiscal_year, fiscal_quarter)
);
CREATE INDEX IF NOT EXISTS idx_period_cik_year ON period(cik, fiscal_year);

CREATE TABLE IF NOT EXISTS raw_fact (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  cik             TEXT NOT NULL REFERENCES company(cik) ON DELETE RESTRICT,
  accession_no    TEXT NOT NULL REFERENCES filing(accession_no) ON DELETE RESTRICT,
  taxonomy        TEXT NOT NULL,
  concept         TEXT NOT NULL,
  unit            TEXT NOT NULL,
  value_numeric   INTEGER NOT NULL,
  period_start    TEXT,
  period_end      TEXT NOT NULL,
  is_instant      INTEGER NOT NULL,
  fy              INTEGER,
  fp              TEXT,
  filed           TEXT,
  source_kind     TEXT NOT NULL,
  ingested_at     TEXT NOT NULL,
  UNIQUE (cik, accession_no, taxonomy, concept, unit, period_start, period_end, fp)
);
CREATE INDEX IF NOT EXISTS idx_raw_cik_concept ON raw_fact(cik, taxonomy, concept);
CREATE INDEX IF NOT EXISTS idx_raw_filing ON raw_fact(accession_no);

CREATE TABLE IF NOT EXISTS normalized_fact (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  cik             TEXT NOT NULL REFERENCES company(cik) ON DELETE RESTRICT,
  metric          TEXT NOT NULL,
  period_id       INTEGER NOT NULL REFERENCES period(id) ON DELETE RESTRICT,
  value           INTEGER NOT NULL,
  unit            TEXT NOT NULL,
  source_fact_id  INTEGER NOT NULL REFERENCES raw_fact(id) ON DELETE RESTRICT,
  source_kind     TEXT NOT NULL,
  is_primary      INTEGER NOT NULL DEFAULT 1,
  original_value  INTEGER,
  original_unit   TEXT,
  fx_rate_micro   INTEGER,
  fx_rate_source  TEXT,
  fx_rate_date    TEXT,
  superseded_by   INTEGER REFERENCES normalized_fact(id) ON DELETE RESTRICT,
  ingested_at     TEXT NOT NULL,
  UNIQUE (cik, metric, period_id, source_fact_id)
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_norm_primary_current
  ON normalized_fact (cik, metric, period_id)
  WHERE is_primary = 1 AND superseded_by IS NULL;
CREATE INDEX IF NOT EXISTS idx_norm_cik_metric_period
  ON normalized_fact(cik, metric, period_id);
CREATE INDEX IF NOT EXISTS idx_norm_superseded_by
  ON normalized_fact(superseded_by)
  WHERE superseded_by IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS trg_norm_no_cycle_update
BEFORE UPDATE OF superseded_by ON normalized_fact
WHEN NEW.superseded_by IS NOT NULL AND EXISTS (
  WITH RECURSIVE chain(id) AS (
    SELECT NEW.superseded_by
    UNION ALL
    SELECT nf.superseded_by
    FROM normalized_fact nf JOIN chain ON nf.id = chain.id
    WHERE nf.superseded_by IS NOT NULL
  )
  SELECT 1 FROM chain WHERE id = NEW.id
)
BEGIN
  SELECT RAISE(ABORT, 'normalized_fact.superseded_by would create a cycle (update)');
END;

CREATE TRIGGER IF NOT EXISTS trg_norm_no_cycle_insert
BEFORE INSERT ON normalized_fact
WHEN NEW.superseded_by IS NOT NULL AND EXISTS (
  WITH RECURSIVE chain(id) AS (
    SELECT NEW.superseded_by
    UNION ALL
    SELECT nf.superseded_by
    FROM normalized_fact nf JOIN chain ON nf.id = chain.id
    WHERE nf.superseded_by IS NOT NULL
  )
  SELECT 1 FROM chain WHERE id = NEW.id
)
BEGIN
  SELECT RAISE(ABORT, 'normalized_fact.superseded_by would create a cycle (insert)');
END;

CREATE TABLE IF NOT EXISTS restatement_announcement (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  cik             TEXT NOT NULL REFERENCES company(cik) ON DELETE RESTRICT,
  accession_no    TEXT NOT NULL REFERENCES filing(accession_no) ON DELETE RESTRICT,
  affected_period_id INTEGER NOT NULL REFERENCES period(id) ON DELETE RESTRICT,
  filed_at        TEXT NOT NULL,
  ingested_at     TEXT NOT NULL,
  UNIQUE (accession_no, affected_period_id)
);
CREATE INDEX IF NOT EXISTS idx_restate_cik ON restatement_announcement(cik);

CREATE TABLE IF NOT EXISTS restatement_resolved_by (
  restatement_announcement_id INTEGER NOT NULL
    REFERENCES restatement_announcement(id) ON DELETE RESTRICT,
  resolving_accession_no TEXT NOT NULL
    REFERENCES filing(accession_no) ON DELETE RESTRICT,
  resolved_at     TEXT NOT NULL,
  PRIMARY KEY (restatement_announcement_id, resolving_accession_no)
);

CREATE TABLE IF NOT EXISTS amendment_coverage_gap (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  cik             TEXT NOT NULL REFERENCES company(cik) ON DELETE RESTRICT,
  amendment_accession_no TEXT NOT NULL
    REFERENCES filing(accession_no) ON DELETE RESTRICT,
  metric          TEXT NOT NULL,
  period_id       INTEGER NOT NULL REFERENCES period(id) ON DELETE RESTRICT,
  ingested_at     TEXT NOT NULL,
  UNIQUE (amendment_accession_no, metric, period_id)
);
CREATE INDEX IF NOT EXISTS idx_amend_gap_cik_period
  ON amendment_coverage_gap(cik, period_id);

CREATE TABLE IF NOT EXISTS historical_price (
  cik             TEXT NOT NULL REFERENCES company(cik) ON DELETE RESTRICT,
  date            TEXT NOT NULL,
  ticker          TEXT NOT NULL,
  close_micro     INTEGER NOT NULL,
  source          TEXT NOT NULL,
  ingested_at     TEXT NOT NULL,
  PRIMARY KEY (cik, date)
);

CREATE TABLE IF NOT EXISTS fx_rate (
  currency        TEXT NOT NULL,
  date            TEXT NOT NULL,
  rate_micro      INTEGER NOT NULL,
  source          TEXT NOT NULL,
  PRIMARY KEY (currency, date)
);

CREATE TABLE IF NOT EXISTS current_price (
  cik             TEXT PRIMARY KEY REFERENCES company(cik) ON DELETE RESTRICT,
  ticker          TEXT NOT NULL,
  price_micro     INTEGER NOT NULL,
  as_of           TEXT NOT NULL,
  source          TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS derived_metric (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  cik             TEXT NOT NULL REFERENCES company(cik) ON DELETE RESTRICT,
  formula_id      TEXT NOT NULL,
  period_id       INTEGER NOT NULL REFERENCES period(id) ON DELETE RESTRICT,
  value           INTEGER,
  is_complete     INTEGER NOT NULL,
  computed_at     TEXT NOT NULL,
  UNIQUE (cik, formula_id, period_id)
);

CREATE TABLE IF NOT EXISTS ingestion_event (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  cik             TEXT REFERENCES company(cik) ON DELETE RESTRICT,
  accession_no    TEXT REFERENCES filing(accession_no) ON DELETE RESTRICT,
  stage           TEXT NOT NULL,
  level           TEXT NOT NULL,
  user_visible    INTEGER NOT NULL DEFAULT 0,
  message         TEXT NOT NULL,
  detail_json     TEXT,
  occurred_at     TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_ing_cik_time ON ingestion_event(cik, occurred_at DESC);
