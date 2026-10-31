-- Aligned custom-metric points previously stored no observed_date, so a period
-- rebuild left them unrecoverable. Backfill the fiscal label from the period.
UPDATE custom_metric_point
SET observed_date = (
  SELECT CASE WHEN p.fiscal_quarter = 0 THEN 'FY' || p.fiscal_year
              ELSE 'FY' || p.fiscal_year || ' Q' || p.fiscal_quarter END
  FROM period p WHERE p.id = custom_metric_point.period_id
)
WHERE period_id IS NOT NULL AND observed_date IS NULL;
