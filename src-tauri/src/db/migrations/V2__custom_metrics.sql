-- V2. Custom metrics (user-supplied, per-company, period-aligned).
-- Separate from the authoritative normalized_fact chain by design: custom
-- values never enter normalization and carry no filing lineage.
-- Values stored as INTEGER micro-units (× 1,000,000), consistent with V1.

CREATE TABLE IF NOT EXISTS custom_metric (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  cik           TEXT NOT NULL REFERENCES company(cik) ON DELETE RESTRICT,
  key           TEXT NOT NULL,              -- slug: ^[a-z0-9][a-z0-9_]*$
  label         TEXT NOT NULL,              -- display name, e.g. "Total Payment Volume"
  unit_kind     TEXT NOT NULL,             -- 'currency' | 'count' | 'ratio' | 'number'
  currency      TEXT,                      -- ISO 4217 when unit_kind='currency' (default 'USD')
  decimals      INTEGER NOT NULL DEFAULT 2,-- display precision for 'number'/'ratio'
  description   TEXT,                       -- optional user definition/note
  created_at    TEXT NOT NULL,             -- RFC3339
  updated_at    TEXT NOT NULL,
  CHECK (unit_kind IN ('currency','count','ratio','number')),
  UNIQUE (cik, key)
);
CREATE INDEX IF NOT EXISTS idx_custom_metric_cik ON custom_metric(cik);

CREATE TABLE IF NOT EXISTS custom_metric_point (
  id               INTEGER PRIMARY KEY AUTOINCREMENT,
  custom_metric_id INTEGER NOT NULL REFERENCES custom_metric(id) ON DELETE RESTRICT,
  period_id        INTEGER REFERENCES period(id) ON DELETE SET NULL,  -- NULL = unaligned
  observed_date    TEXT,                    -- raw import date (RFC3339 date), retained for re-resolution
  value_micro      INTEGER NOT NULL,        -- value × 1e6 (i64)
  source_note      TEXT,                    -- filename or 'manual'
  imported_at      TEXT NOT NULL,
  -- At most one point per (metric, period) when aligned; unaligned points
  -- (period_id IS NULL) are disambiguated by observed_date.
  UNIQUE (custom_metric_id, period_id),
  UNIQUE (custom_metric_id, observed_date)
);
CREATE INDEX IF NOT EXISTS idx_cmp_metric_period
  ON custom_metric_point(custom_metric_id, period_id);
CREATE INDEX IF NOT EXISTS idx_cmp_unaligned
  ON custom_metric_point(custom_metric_id)
  WHERE period_id IS NULL;
