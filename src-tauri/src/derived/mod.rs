//! Derived-metric formulas — M28.
//!
//! This module holds the **pure** math for fixed derived metrics: each
//! function takes already-normalized inputs (micro-units per architecture
//! §6.2) and returns a derived value with no I/O. The read-time *series
//! assembly* — joining per-period inputs from the repositories and applying
//! these formulas — lives in [`series`], which is the layer the IPC handlers
//! and the production-mode integration tests both call.
//!
//! Keeping the arithmetic here (pure, no `async`, no repos) makes it cheap to
//! exhaustively unit-test the formulas independently of any database, which is
//! where accuracy regressions are easiest to catch.

pub mod series;

/// Free cash flow `= net_income + depreciation_amortization − capital_expenditures`.
///
/// All inputs are in micro-units (USD × 1,000,000). Capital expenditures is
/// stored **positive** (sign-normalized per architecture §6.2), so it is
/// subtracted here; depreciation & amortization is a positive non-cash add-back.
/// Net income is signed. Arithmetic saturates rather than panicking on the
/// (practically impossible) overflow of summing three `i64` micro-unit values.
pub fn free_cash_flow(
    net_income: i64,
    depreciation_amortization: i64,
    capital_expenditures: i64,
) -> i64 {
    net_income
        .saturating_add(depreciation_amortization)
        .saturating_sub(capital_expenditures)
}

/// Operating margin `= operating_income ÷ revenue`, returned as a **decimal
/// ratio in micro-units** (ratio × 1,000,000) to match the §6.2 "pure →
/// ×1,000,000" storage convention used for all dimensionless values. For
/// example a 25.3% margin is `253_000`.
///
/// Returns `None` when revenue is non-positive: division by zero is undefined,
/// and a zero/negative revenue denominator makes the margin meaningless, so we
/// omit the period rather than emit a misleading number (accuracy rule).
///
/// `operating_income` may be negative (an operating loss) — a negative margin
/// is a valid, meaningful result and is preserved.
pub fn operating_margin_micro(operating_income: i64, revenue: i64) -> Option<i64> {
    if revenue <= 0 {
        return None;
    }
    // The micro-unit scales of operating_income and revenue cancel in the
    // ratio, so we scale the quotient back up by 1e6. Compute in i128 to
    // avoid overflowing i64 on the intermediate `operating_income × 1e6`
    // (a $9T operating income in micro-units × 1e6 exceeds i64::MAX).
    let scaled = (operating_income as i128) * 1_000_000 / (revenue as i128);
    Some(scaled as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── free_cash_flow ────────────────────────────────────────────────────

    #[test]
    fn fcf_textbook_formula() {
        // net income + D&A − capex
        assert_eq!(free_cash_flow(100, 30, 20), 110);
    }

    #[test]
    fn fcf_real_world_zoetis_fy2025() {
        // From the production DB (micro-units): NI=2.673e15, D&A=0.487e15,
        // CapEx=0.621e15 → FCF = 2.539e15 (= $2.539B).
        let fcf = free_cash_flow(2_673_000_000_000_000, 487_000_000_000_000, 621_000_000_000_000);
        assert_eq!(fcf, 2_539_000_000_000_000);
    }

    #[test]
    fn fcf_handles_operating_loss() {
        // Negative net income still produces a well-defined (possibly negative) FCF.
        assert_eq!(free_cash_flow(-50, 10, 40), -80);
    }

    #[test]
    fn fcf_saturates_on_overflow_instead_of_panicking() {
        assert_eq!(free_cash_flow(i64::MAX, i64::MAX, 0), i64::MAX);
        assert_eq!(free_cash_flow(i64::MIN, 0, i64::MAX), i64::MIN);
    }

    // ── operating_margin_micro ────────────────────────────────────────────

    #[test]
    fn margin_basic_ratio() {
        // 250 / 1000 = 0.25 → 250_000 micro-ratio (25.0%).
        assert_eq!(operating_margin_micro(250, 1000), Some(250_000));
    }

    #[test]
    fn margin_real_world_zoetis_fy2025() {
        // OI=3.360e15, Rev=9.467e15 → 0.354917… → 354_917 micro (35.49%).
        let m = operating_margin_micro(3_360_000_000_000_000, 9_467_000_000_000_000).unwrap();
        assert_eq!(m, 354_917);
    }

    #[test]
    fn margin_real_world_dollar_general_fy2026() {
        // OI=1.964592e15, Rev=42.724369e15 → 0.0459829… → 45_982 micro (4.60%).
        // Integer division truncates toward zero (45_982.9 → 45_982).
        let m = operating_margin_micro(1_964_592_000_000_000, 42_724_369_000_000_000).unwrap();
        assert_eq!(m, 45_982);
    }

    #[test]
    fn margin_negative_when_operating_loss() {
        // −100 / 1000 = −0.10 → −100_000 micro (−10.0%).
        assert_eq!(operating_margin_micro(-100, 1000), Some(-100_000));
    }

    #[test]
    fn margin_undefined_for_nonpositive_revenue() {
        assert_eq!(operating_margin_micro(100, 0), None);
        assert_eq!(operating_margin_micro(100, -500), None);
    }

    #[test]
    fn margin_does_not_overflow_on_large_inputs() {
        // $1T operating income in micro-units, $2T revenue → 0.5 → 500_000.
        let m = operating_margin_micro(1_000_000_000_000_000_000, 2_000_000_000_000_000_000);
        assert_eq!(m, Some(500_000));
    }
}
