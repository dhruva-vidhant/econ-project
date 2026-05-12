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
        ],
        Metric::GrossProfit => &[("us-gaap", "GrossProfit")],
        Metric::OperatingIncome => &[
            ("us-gaap", "OperatingIncomeLoss"),
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
        ],
        Metric::LongTermDebt => &[
            ("us-gaap", "LongTermDebt"),
            ("us-gaap", "LongTermDebtNoncurrent"),
        ],
        Metric::CurrentDebt => &[
            ("us-gaap", "DebtCurrent"),
            ("us-gaap", "LongTermDebtCurrent"),
        ],
        Metric::TotalDebt => &[], // derived from LongTermDebt + CurrentDebt
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
        ],
        // Bank-revenue inputs. Used by the `bank_revenue_v1` derivation
        // (see pipeline::orchestrator) when canonical Revenue is missing.
        Metric::NetInterestIncome => &[
            ("us-gaap", "InterestIncomeExpenseNet"),
            ("us-gaap", "InterestIncomeExpenseAfterProvisionForLoanLoss"),
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
}
