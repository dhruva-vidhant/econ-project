use std::collections::BTreeSet;

use chrono::{Datelike, NaiveDate};

use crate::domain::{Period, RawFact};

const CLOSE_DRIFT_DAYS: i64 = 17;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FiscalCalendarSegment {
    pub id: usize,
    pub effective_from: Option<NaiveDate>,
    pub fiscal_year_end: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FiscalCalendar {
    segments: Vec<FiscalCalendarSegment>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FiscalPeriod {
    pub segment_id: usize,
    pub fiscal_year_end: String,
    pub fiscal_year: i32,
    pub fiscal_quarter: Option<u8>,
}

#[derive(Clone, Debug)]
struct CloseRegime {
    closes: Vec<NaiveDate>,
}

impl FiscalCalendar {
    /// Infer date-effective fiscal calendars from recurring, normal-length FY
    /// facts. SEC companyfacts repeats the same close across concepts and
    /// filings, so evidence is de-duplicated by date before regimes are built.
    /// A one-off historical close cannot establish a regime; the terminal
    /// regime may additionally be corroborated by the SEC's current FYE.
    pub fn infer(raw: &[RawFact], current_fye: &str) -> Self {
        let current_fye = normalize_mmdd(current_fye).unwrap_or_else(|| "1231".into());
        let mut closes = BTreeSet::new();
        for fact in raw {
            let Some(start) = fact.period_start else {
                continue;
            };
            if fact.fp.as_deref() != Some("FY") || fact.is_instant {
                continue;
            }
            let days = (fact.period_end - start).num_days();
            if (340..=380).contains(&days)
                && crate::normalize::concept_map::metric_for(&fact.taxonomy, &fact.concept)
                    .is_some_and(|metric| !metric.is_instant())
            {
                closes.insert(fact.period_end);
            }
        }

        let mut regimes: Vec<CloseRegime> = Vec::new();
        for close in closes {
            match regimes.last_mut() {
                Some(regime)
                    if circular_day_distance(
                        *regime.closes.last().expect("regime is non-empty"),
                        close,
                    ) <= CLOSE_DRIFT_DAYS =>
                {
                    regime.closes.push(close);
                }
                _ => regimes.push(CloseRegime {
                    closes: vec![close],
                }),
            }
        }

        // Isolated historical outliers are not calendar changes. Merge them
        // into an adjacent regime, preferring the closer recurring pattern.
        let mut i = 0;
        while regimes.len() > 1 && i < regimes.len() {
            let terminal_current =
                i + 1 == regimes.len() && regime_matches_fye(&regimes[i], &current_fye);
            if regimes[i].closes.len() > 1 || terminal_current {
                i += 1;
                continue;
            }
            let close = regimes[i].closes[0];
            let previous = i
                .checked_sub(1)
                .map(|p| circular_day_distance(close, *regimes[p].closes.last().unwrap()));
            let next = (i + 1 < regimes.len())
                .then(|| circular_day_distance(close, regimes[i + 1].closes[0]));
            if previous.unwrap_or(i64::MAX) <= next.unwrap_or(i64::MAX) {
                let orphan = regimes.remove(i);
                regimes[i - 1].closes.extend(orphan.closes);
            } else {
                let orphan = regimes.remove(i);
                regimes[i].closes.splice(0..0, orphan.closes);
            }
        }

        let mut i = 1;
        while i < regimes.len() {
            let previous_pattern = *regimes[i - 1].closes.first().expect("regime is non-empty");
            let current_pattern = *regimes[i].closes.last().expect("regime is non-empty");
            if circular_day_distance(previous_pattern, current_pattern) <= CLOSE_DRIFT_DAYS {
                let current = regimes.remove(i);
                regimes[i - 1].closes.extend(current.closes);
            } else {
                i += 1;
            }
        }

        if regimes.is_empty() {
            return Self::single(current_fye);
        }

        let last = regimes.len() - 1;
        let mut segments = Vec::with_capacity(regimes.len());
        for (id, regime) in regimes.iter().enumerate() {
            let effective_from = if id == 0 {
                None
            } else {
                regimes[id - 1]
                    .closes
                    .last()
                    .and_then(|date| date.succ_opt())
            };
            let fiscal_year_end = if id == last && regime_matches_fye(regime, &current_fye) {
                current_fye.clone()
            } else {
                mmdd(*regime.closes.last().expect("regime is non-empty"))
            };
            segments.push(FiscalCalendarSegment {
                id,
                effective_from,
                fiscal_year_end,
            });
        }
        Self { segments }
    }

    pub fn single(fiscal_year_end: String) -> Self {
        Self {
            segments: vec![FiscalCalendarSegment {
                id: 0,
                effective_from: None,
                fiscal_year_end,
            }],
        }
    }

    pub fn period_for(&self, end: NaiveDate) -> FiscalPeriod {
        let segment = self
            .segments
            .iter()
            .rev()
            .find(|segment| segment.effective_from.is_none_or(|from| end >= from))
            .expect("a fiscal calendar always has an initial segment");
        FiscalPeriod {
            segment_id: segment.id,
            fiscal_year_end: segment.fiscal_year_end.clone(),
            fiscal_year: Period::compute_fiscal_year(end, &segment.fiscal_year_end),
            fiscal_quarter: Period::compute_fiscal_quarter(end, &segment.fiscal_year_end),
        }
    }

    pub fn segments(&self) -> &[FiscalCalendarSegment] {
        &self.segments
    }
}

fn normalize_mmdd(value: &str) -> Option<String> {
    if value.len() != 4 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let month: u32 = value[0..2].parse().ok()?;
    let day: u32 = value[2..4].parse().ok()?;
    NaiveDate::from_ymd_opt(2000, month, day)?;
    Some(value.to_owned())
}

fn mmdd(date: NaiveDate) -> String {
    format!("{:02}{:02}", date.month(), date.day())
}

fn regime_matches_fye(regime: &CloseRegime, fye: &str) -> bool {
    let Some(anchor) = mmdd_date(fye) else {
        return false;
    };
    regime
        .closes
        .iter()
        .any(|close| circular_day_distance(*close, anchor) <= CLOSE_DRIFT_DAYS)
}

fn mmdd_date(value: &str) -> Option<NaiveDate> {
    let normalized = normalize_mmdd(value)?;
    let month = normalized[0..2].parse().ok()?;
    let day = normalized[2..4].parse().ok()?;
    NaiveDate::from_ymd_opt(2000, month, day)
}

fn circular_day_distance(left: NaiveDate, right: NaiveDate) -> i64 {
    let left = NaiveDate::from_ymd_opt(2000, left.month(), left.day()).unwrap();
    let right = NaiveDate::from_ymd_opt(2000, right.month(), right.day()).unwrap();
    let direct = (left - right).num_days().abs();
    direct.min(366 - direct)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AccessionNo, Cik, SourceKind};
    use chrono::Utc;

    fn d(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    fn fact(fp: &str, start: NaiveDate, end: NaiveDate) -> RawFact {
        RawFact {
            id: 0,
            cik: Cik("0001113809".into()),
            accession_no: AccessionNo(format!("{end}")),
            taxonomy: "us-gaap".into(),
            concept: "Revenues".into(),
            unit: "USD".into(),
            value_numeric: 1,
            period_start: Some(start),
            period_end: end,
            is_instant: false,
            fy: Some(2099),
            fp: Some(fp.into()),
            filed: Some(end),
            source_kind: SourceKind::XbrlApi,
            ingested_at: Utc::now(),
        }
    }

    #[test]
    fn bbw_history_uses_old_and_current_calendars() {
        let raw = vec![
            fact("FY", d(2015, 1, 4), d(2016, 1, 2)),
            fact("FY", d(2016, 1, 3), d(2016, 12, 31)),
            fact("FY", d(2017, 1, 1), d(2017, 12, 30)),
            fact("FY", d(2018, 2, 4), d(2019, 2, 2)),
            fact("FY", d(2019, 2, 3), d(2020, 2, 1)),
        ];

        let calendar = FiscalCalendar::infer(&raw, "0130");

        assert_eq!(calendar.segments().len(), 2);
        assert_eq!(calendar.period_for(d(2017, 12, 30)).fiscal_year_end, "1230");
        assert_eq!(calendar.period_for(d(2017, 12, 30)).fiscal_year, 2017);
        assert_eq!(calendar.period_for(d(2018, 5, 5)).fiscal_year_end, "0130");
        assert_eq!(calendar.period_for(d(2018, 5, 5)).fiscal_year, 2019);
        assert_eq!(calendar.period_for(d(2019, 2, 2)).fiscal_quarter, Some(0));
    }

    #[test]
    fn stable_retail_drift_remains_one_regime() {
        let raw = vec![
            fact("FY", d(2020, 2, 3), d(2021, 1, 30)),
            fact("FY", d(2021, 1, 31), d(2022, 1, 29)),
            fact("FY", d(2022, 1, 30), d(2023, 1, 28)),
            fact("FY", d(2023, 1, 29), d(2024, 2, 3)),
        ];

        let calendar = FiscalCalendar::infer(&raw, "0202");

        assert_eq!(calendar.segments().len(), 1);
        assert_eq!(calendar.segments()[0].fiscal_year_end, "0202");
    }

    #[test]
    fn isolated_close_does_not_create_a_historical_regime() {
        let raw = vec![
            fact("FY", d(2020, 1, 1), d(2020, 12, 31)),
            fact("FY", d(2021, 1, 1), d(2021, 12, 31)),
            fact("FY", d(2022, 2, 1), d(2023, 1, 31)),
            fact("FY", d(2023, 1, 1), d(2023, 12, 31)),
            fact("FY", d(2024, 1, 1), d(2024, 12, 31)),
        ];

        let calendar = FiscalCalendar::infer(&raw, "1231");

        assert_eq!(calendar.segments().len(), 1);
        assert_eq!(calendar.segments()[0].fiscal_year_end, "1231");
    }

    #[test]
    fn short_transition_report_is_not_calendar_evidence() {
        let raw = vec![
            fact("FY", d(2020, 1, 1), d(2020, 12, 31)),
            fact("FY", d(2021, 1, 1), d(2021, 12, 31)),
            fact("FY", d(2022, 1, 1), d(2022, 6, 30)),
            fact("FY", d(2022, 7, 1), d(2023, 6, 30)),
            fact("FY", d(2023, 7, 1), d(2024, 6, 30)),
        ];

        let calendar = FiscalCalendar::infer(&raw, "0630");

        assert_eq!(calendar.segments().len(), 2);
        assert_eq!(calendar.period_for(d(2021, 12, 31)).fiscal_year_end, "1231");
        assert_eq!(calendar.period_for(d(2022, 6, 30)).fiscal_year_end, "0630");
    }
}
