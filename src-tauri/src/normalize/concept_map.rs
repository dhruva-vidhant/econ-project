//! M18 — Canonical metric ↔ XBRL concept map.
//! Ordered candidates: the catalog primary first, then fallbacks.

use crate::domain::Metric;

/// Returns the ordered list of `(taxonomy, concept)` candidates for a metric.
pub fn concepts_for(metric: Metric) -> &'static [(&'static str, &'static str)] {
    match metric {
        Metric::Revenue => &[
            ("us-gaap", "Revenues"),
            ("us-gaap", "RevenueFromContractWithCustomerExcludingAssessedTax"),
            ("us-gaap", "RevenueFromContractWithCustomerIncludingAssessedTax"),
            ("us-gaap", "SalesRevenueNet"),
            ("us-gaap", "SalesRevenueGoodsNet"),
        ],
        Metric::CostOfRevenue => &[
            ("us-gaap", "CostOfRevenue"),
            ("us-gaap", "CostOfGoodsAndServicesSold"),
            ("us-gaap", "CostOfGoodsSold"),
            // Bank fallback: banks don't have a cost-of-goods structure.
            // NoninterestExpense (salaries, occupancy, tech, professional
            // services, etc.) is the closest analog — operating costs
            // incurred to generate revenue. Caveat: this excludes the
            // provision for credit losses, which some bank-investor
            // models treat as an additional cost of revenue. We surface
            // operating expenses only; the provision is left aside.
            ("us-gaap", "NoninterestExpense"),
        ],
        Metric::GrossProfit => &[("us-gaap", "GrossProfit")],
        Metric::OperatingIncome => &[
            ("us-gaap", "OperatingIncomeLoss"),
            // Bank fallback: filers that do not separate "operating" from
            // "non-operating" report pre-tax income from continuing ops
            // (the closest GAAP analog). Used by WFC, JPM, and other large
            // bank holding companies.
            ("us-gaap", "IncomeLossFromContinuingOperationsBeforeIncomeTaxesExtraordinaryItemsNoncontrollingInterest"),
            ("us-gaap", "IncomeLossFromContinuingOperationsBeforeIncomeTaxesMinorityInterestAndIncomeLossFromEquityMethodInvestments"),
        ],
        Metric::NetIncome => &[
            ("us-gaap", "NetIncomeLoss"),
            ("us-gaap", "ProfitLoss"),
        ],
        Metric::EpsBasic => &[("us-gaap", "EarningsPerShareBasic")],
        Metric::EpsDiluted => &[("us-gaap", "EarningsPerShareDiluted")],
        Metric::SharesOutstandingBasic => &[
            ("us-gaap", "WeightedAverageNumberOfSharesOutstandingBasic"),
            ("dei", "EntityCommonStockSharesOutstanding"),
        ],
        Metric::SharesOutstandingDiluted => &[
            ("us-gaap", "WeightedAverageNumberOfDilutedSharesOutstanding"),
        ],
        Metric::CashAndEquivalents => &[
            ("us-gaap", "CashAndCashEquivalentsAtCarryingValue"),
            ("us-gaap", "Cash"),
            // Post-ASU 2016-18 successor used by many filers (combines
            // cash, equivalents, and restricted cash on the cashflow side).
            ("us-gaap", "CashCashEquivalentsRestrictedCashAndRestrictedCashEquivalents"),
            // Bank fallback: bank holding companies report "Cash and due
            // from banks" rather than the generic concept above.
            ("us-gaap", "CashAndDueFromBanks"),
        ],
        Metric::LongTermDebt => &[
            ("us-gaap", "LongTermDebt"),
            ("us-gaap", "LongTermDebtNoncurrent"),
        ],
        Metric::CurrentDebt => &[
            ("us-gaap", "DebtCurrent"),
            ("us-gaap", "LongTermDebtCurrent"),
            // Bank fallback: short-term wholesale funding (commercial
            // paper, fed-funds purchased, repo agreements) is the bank
            // analog to "current debt" for non-bank issuers.
            ("us-gaap", "ShortTermBorrowings"),
        ],
        Metric::TotalDebt => &[], // derived at read time from LongTermDebt + CurrentDebt
        // Purely derived at read time (no source XBRL concept):
        //   free_cash_flow   = net_income + depreciation_amortization − capital_expenditures
        //   operating_margin = operating_income ÷ revenue
        Metric::FreeCashFlow => &[],
        Metric::OperatingMargin => &[],
        // Trailing-twelve-month FCF, FCF yield: derived at read time (no source concept).
        Metric::FreeCashFlowTtm => &[],
        Metric::FreeCashFlowYield => &[],
        Metric::TotalAssets => &[("us-gaap", "Assets")],
        Metric::TotalLiabilities => &[("us-gaap", "Liabilities")],
        Metric::TotalEquity => &[
            ("us-gaap", "StockholdersEquity"),
            ("us-gaap", "StockholdersEquityIncludingPortionAttributableToNoncontrollingInterest"),
        ],
        Metric::CashFromOperations => &[
            ("us-gaap", "NetCashProvidedByUsedInOperatingActivities"),
            ("us-gaap", "NetCashProvidedByUsedInOperatingActivitiesContinuingOperations"),
        ],
        Metric::CapitalExpenditures => &[
            ("us-gaap", "PaymentsToAcquirePropertyPlantAndEquipment"),
            ("us-gaap", "PaymentsToAcquireProductiveAssets"),
        ],
        Metric::DepreciationAmortization => &[
            ("us-gaap", "DepreciationAndAmortization"),
            ("us-gaap", "DepreciationDepletionAndAmortization"),
            // Bank fallback: WFC and other large banks file
            // DepreciationAmortizationAndAccretionNet quarterly (and
            // FY), while the canonical DepreciationAndAmortization is
            // only filed annually. Adding this fallback fills the
            // quarterly series.
            ("us-gaap", "DepreciationAmortizationAndAccretionNet"),
        ],
        // Used internally as an input to capital-expenditures derivation
        // when a filer doesn't report PaymentsToAcquirePropertyPlantAndEquipment.
        Metric::PropertyPlantAndEquipmentNet => &[
            ("us-gaap", "PropertyPlantAndEquipmentNet"),
        ],
        // Bank-revenue inputs. Used by the `bank_revenue_v1` derivation
        // (see pipeline::orchestrator) when canonical Revenue is missing.
        //
        // Important: do NOT add `InterestIncomeExpenseAfterProvisionForLoanLoss`
        // here. That concept is gross NII *minus* the provision for loan
        // losses — a different (smaller) metric. Counting it as NII
        // under-reports bank revenue by the provision amount, which
        // surfaces as a multi-billion-dollar gap in pre-2015 WFC data.
        Metric::NetInterestIncome => &[
            ("us-gaap", "InterestIncomeExpenseNet"),
        ],
        Metric::NoninterestIncome => &[
            ("us-gaap", "NoninterestIncome"),
            ("us-gaap", "NonInterestIncome"),
        ],
        Metric::InterestIncomeOperating => &[
            ("us-gaap", "InterestAndDividendIncomeOperating"),
            ("us-gaap", "InterestIncomeOperating"),
        ],
        Metric::InterestExpense => &[
            ("us-gaap", "InterestExpense"),
        ],
        Metric::HistoricalMarketCap | Metric::CurrentMarketCap => &[],
    }
}

/// Reverse lookup: given a (taxonomy, concept), find the canonical metric.
/// Returns the first metric whose candidate list contains the concept.
pub fn metric_for(taxonomy: &str, concept: &str) -> Option<Metric> {
    for m in Metric::ALL {
        if concepts_for(*m).iter().any(|(t, c)| *t == taxonomy && *c == concept) {
            return Some(*m);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revenue_has_candidates() {
        assert!(!concepts_for(Metric::Revenue).is_empty());
    }

    #[test]
    fn known_concept_resolves() {
        assert_eq!(metric_for("us-gaap", "Revenues"), Some(Metric::Revenue));
        assert_eq!(metric_for("us-gaap", "Assets"), Some(Metric::TotalAssets));
    }

    #[test]
    fn unknown_concept_returns_none() {
        assert_eq!(metric_for("us-gaap", "MadeUpConcept"), None);
    }

    #[test]
    fn total_debt_has_no_direct_candidates_it_is_derived() {
        assert!(concepts_for(Metric::TotalDebt).is_empty());
    }

    #[test]
    fn bank_specific_concepts_route_to_canonical_metrics() {
        // Bank-style cash, short-term funding, and pre-tax income concepts
        // resolve to the right canonical metrics. Regression guard for
        // WFC-style filers that don't use the non-bank GAAP defaults.
        assert_eq!(
            metric_for("us-gaap", "CashAndDueFromBanks"),
            Some(Metric::CashAndEquivalents)
        );
        assert_eq!(
            metric_for("us-gaap", "ShortTermBorrowings"),
            Some(Metric::CurrentDebt)
        );
        assert_eq!(
            metric_for(
                "us-gaap",
                "IncomeLossFromContinuingOperationsBeforeIncomeTaxesExtraordinaryItemsNoncontrollingInterest"
            ),
            Some(Metric::OperatingIncome)
        );
    }
}
