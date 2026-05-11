use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use super::fact::{NormalizedFact, RawFact};
use super::filing::Filing;
use super::Micro;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FxConversion {
    pub original_value: Micro,
    pub original_unit: String,
    pub rate_micro: i64,
    pub rate_source: String,
    pub rate_date: NaiveDate,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineageRecord {
    pub fact: RawFact,
    pub filing: Filing,
    pub fx_conversion: Option<FxConversion>,
    /// Backward chain: prior versions of this normalized fact, oldest → newest.
    /// The current value is `supersedes.last()` if non-empty, otherwise the
    /// `RawFact` is the only version.
    pub supersedes: Vec<NormalizedFact>,
}
