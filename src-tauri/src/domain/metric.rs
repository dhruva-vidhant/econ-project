use serde::{Deserialize, Serialize};

/// Canonical metric catalog. See `docs/architecture.md` §6.2.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Metric {
    // Income
    Revenue,
    CostOfRevenue,
    GrossProfit,
    OperatingIncome,
    NetIncome,
    EpsBasic,
    EpsDiluted,
    SharesOutstandingBasic,
    SharesOutstandingDiluted,
    // Balance
    CashAndEquivalents,
    LongTermDebt,
    CurrentDebt,
    TotalDebt,
    TotalAssets,
    TotalLiabilities,
    TotalEquity,
    // Cash flow
    CashFromOperations,
    CapitalExpenditures,
    DepreciationAmortization,
    // Property, plant & equipment, net (instant). Used as an input for
    // deriving capital expenditures when a filer doesn't report
    // PaymentsToAcquirePropertyPlantAndEquipment directly.
    PropertyPlantAndEquipmentNet,
    // Bank-revenue inputs (used when canonical Revenue is missing for a
    // bank-style filer; see `bank_revenue_v1` derivation)
    NetInterestIncome,
    NoninterestIncome,
    InterestIncomeOperating,
    InterestExpense,
    // Derived (multi-input / ratio), computed at read time. See
    // `derived::series` and architecture §6.2.
    //   free_cash_flow  = net_income + depreciation_amortization − capital_expenditures
    //   operating_margin = operating_income ÷ revenue (decimal ratio, ×1e6 per §6.2)
    FreeCashFlow,
    OperatingMargin,
    // Trailing-twelve-month free cash flow (quarterly: sum of the trailing
    // 4 quarters). Annual periods reuse the plain annual free_cash_flow.
    FreeCashFlowTtm,
    // Free cash flow yield = free_cash_flow ÷ market cap (decimal ratio ×1e6).
    // Annual uses annual FCF; quarterly uses trailing-twelve-month FCF.
    FreeCashFlowYield,
    // Market (derived)
    HistoricalMarketCap,
    CurrentMarketCap,
}

impl Metric {
    pub fn as_str(&self) -> &'static str {
        match self {
            Metric::Revenue => "revenue",
            Metric::CostOfRevenue => "cost_of_revenue",
            Metric::GrossProfit => "gross_profit",
            Metric::OperatingIncome => "operating_income",
            Metric::NetIncome => "net_income",
            Metric::EpsBasic => "eps_basic",
            Metric::EpsDiluted => "eps_diluted",
            Metric::SharesOutstandingBasic => "shares_outstanding_basic",
            Metric::SharesOutstandingDiluted => "shares_outstanding_diluted",
            Metric::CashAndEquivalents => "cash_and_equivalents",
            Metric::LongTermDebt => "long_term_debt",
            Metric::CurrentDebt => "current_debt",
            Metric::TotalDebt => "total_debt",
            Metric::TotalAssets => "total_assets",
            Metric::TotalLiabilities => "total_liabilities",
            Metric::TotalEquity => "total_equity",
            Metric::CashFromOperations => "cash_from_operations",
            Metric::CapitalExpenditures => "capital_expenditures",
            Metric::DepreciationAmortization => "depreciation_amortization",
            Metric::PropertyPlantAndEquipmentNet => "property_plant_and_equipment_net",
            Metric::NetInterestIncome => "net_interest_income",
            Metric::NoninterestIncome => "noninterest_income",
            Metric::InterestIncomeOperating => "interest_income_operating",
            Metric::InterestExpense => "interest_expense",
            Metric::FreeCashFlow => "free_cash_flow",
            Metric::OperatingMargin => "operating_margin",
            Metric::FreeCashFlowTtm => "free_cash_flow_ttm",
            Metric::FreeCashFlowYield => "free_cash_flow_yield",
            Metric::HistoricalMarketCap => "historical_market_cap",
            Metric::CurrentMarketCap => "current_market_cap",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "revenue" => Metric::Revenue,
            "cost_of_revenue" => Metric::CostOfRevenue,
            "gross_profit" => Metric::GrossProfit,
            "operating_income" => Metric::OperatingIncome,
            "net_income" => Metric::NetIncome,
            "eps_basic" => Metric::EpsBasic,
            "eps_diluted" => Metric::EpsDiluted,
            "shares_outstanding_basic" => Metric::SharesOutstandingBasic,
            "shares_outstanding_diluted" => Metric::SharesOutstandingDiluted,
            "cash_and_equivalents" => Metric::CashAndEquivalents,
            "long_term_debt" => Metric::LongTermDebt,
            "current_debt" => Metric::CurrentDebt,
            "total_debt" => Metric::TotalDebt,
            "total_assets" => Metric::TotalAssets,
            "total_liabilities" => Metric::TotalLiabilities,
            "total_equity" => Metric::TotalEquity,
            "cash_from_operations" => Metric::CashFromOperations,
            "capital_expenditures" => Metric::CapitalExpenditures,
            "depreciation_amortization" => Metric::DepreciationAmortization,
            "property_plant_and_equipment_net" => Metric::PropertyPlantAndEquipmentNet,
            "net_interest_income" => Metric::NetInterestIncome,
            "noninterest_income" => Metric::NoninterestIncome,
            "interest_income_operating" => Metric::InterestIncomeOperating,
            "interest_expense" => Metric::InterestExpense,
            "free_cash_flow" => Metric::FreeCashFlow,
            "operating_margin" => Metric::OperatingMargin,
            "free_cash_flow_ttm" => Metric::FreeCashFlowTtm,
            "free_cash_flow_yield" => Metric::FreeCashFlowYield,
            "historical_market_cap" => Metric::HistoricalMarketCap,
            "current_market_cap" => Metric::CurrentMarketCap,
            _ => return None,
        })
    }

    /// All metrics in the catalog. Useful for tests and concept-map coverage checks.
    pub const ALL: &'static [Metric] = &[
        Metric::Revenue, Metric::CostOfRevenue, Metric::GrossProfit,
        Metric::OperatingIncome, Metric::NetIncome,
        Metric::EpsBasic, Metric::EpsDiluted,
        Metric::SharesOutstandingBasic, Metric::SharesOutstandingDiluted,
        Metric::CashAndEquivalents, Metric::LongTermDebt, Metric::CurrentDebt,
        Metric::TotalDebt, Metric::TotalAssets, Metric::TotalLiabilities, Metric::TotalEquity,
        Metric::CashFromOperations, Metric::CapitalExpenditures, Metric::DepreciationAmortization,
        Metric::PropertyPlantAndEquipmentNet,
        Metric::NetInterestIncome, Metric::NoninterestIncome,
        Metric::InterestIncomeOperating, Metric::InterestExpense,
        Metric::FreeCashFlow, Metric::OperatingMargin,
        Metric::FreeCashFlowTtm, Metric::FreeCashFlowYield,
        Metric::HistoricalMarketCap, Metric::CurrentMarketCap,
    ];

    /// Whether this metric is "instant" (balance-sheet point-in-time) vs "duration".
    pub fn is_instant(&self) -> bool {
        matches!(
            self,
            Metric::SharesOutstandingBasic | Metric::SharesOutstandingDiluted
            | Metric::CashAndEquivalents | Metric::LongTermDebt | Metric::CurrentDebt
            | Metric::TotalDebt | Metric::TotalAssets | Metric::TotalLiabilities
            | Metric::TotalEquity | Metric::PropertyPlantAndEquipmentNet
            | Metric::HistoricalMarketCap | Metric::CurrentMarketCap
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_metrics_round_trip() {
        for m in Metric::ALL {
            assert_eq!(Metric::from_str(m.as_str()), Some(*m));
        }
    }

    #[test]
    fn instant_classification() {
        assert!(Metric::TotalAssets.is_instant());
        assert!(!Metric::Revenue.is_instant());
    }
}
