//! Dispatcher for code-owned, SEC-reviewed custom metric datasets.

use crate::domain::Cik;
use crate::errors::RepoError;
use crate::repos::custom_metric::CustomMetricRepo;
use crate::repos::period::PeriodRepo;

use super::{bbw_metrics, psx_metrics};

pub async fn sync(
    cik: &Cik,
    periods: &dyn PeriodRepo,
    metrics: &dyn CustomMetricRepo,
) -> Result<(), RepoError> {
    bbw_metrics::sync(cik, periods, metrics).await?;
    psx_metrics::sync(cik, periods, metrics).await?;
    Ok(())
}

pub fn is_managed(cik: &Cik, key: &str) -> bool {
    (cik.0 == "0001113809" && key.starts_with("bbw_")) || psx_metrics::is_managed(cik, key)
}
