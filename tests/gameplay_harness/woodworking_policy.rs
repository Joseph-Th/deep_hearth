//! Woodworking investment criterion and its observable decision vocabulary.

use deep_hearth::core::quantity::Mass;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WoodworkingInvestmentReason {
    #[allow(dead_code, reason = "assigned by the lived woodworking evaluator")]
    BareHandsAvoidsInvestmentCost,
    CopperSupplyLimited,
    CopperReserveProtected,
    SetupAttentionBudgetExceeded,
    #[allow(dead_code, reason = "assigned by the lived woodworking evaluator")]
    ReinforcedAdzePreservesCopper,
    #[allow(dead_code, reason = "assigned by the lived woodworking evaluator")]
    ReinforcedAdzeRepaysAttention,
    PipelineTimberCostNotRecovered,
    PipelineTimberNeutralOutsideSetupBudget,
    PipelineTimberNeutralWithinSetupBudget,
    PipelineNetTimberSaving,
}

impl WoodworkingInvestmentReason {
    #[cfg(not(test))]
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::BareHandsAvoidsInvestmentCost => "bare-hands-avoids-investment-cost",
            Self::CopperSupplyLimited => "copper-supply-limited",
            Self::CopperReserveProtected => "copper-reserve-protected",
            Self::SetupAttentionBudgetExceeded => "setup-attention-budget-exceeded",
            Self::ReinforcedAdzePreservesCopper => "reinforced-adze-preserves-copper",
            Self::ReinforcedAdzeRepaysAttention => "reinforced-adze-repays-attention",
            Self::PipelineTimberCostNotRecovered => "pipeline-timber-cost-not-recovered",
            Self::PipelineTimberNeutralOutsideSetupBudget => {
                "pipeline-timber-neutral-outside-setup-budget"
            }
            Self::PipelineTimberNeutralWithinSetupBudget => {
                "pipeline-timber-neutral-within-setup-budget"
            }
            Self::PipelineNetTimberSaving => "pipeline-net-timber-saving",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WoodworkingTimberBalance {
    Unavailable,
    Saving,
    Neutral,
    Costlier,
}

impl WoodworkingTimberBalance {
    #[cfg(not(test))]
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::Saving => "saving",
            Self::Neutral => "neutral",
            Self::Costlier => "costlier",
        }
    }
}

pub(super) fn woodworking_timber_balance(
    saw: Option<Mass>,
    adze: Mass,
) -> WoodworkingTimberBalance {
    match saw.map(|mass| mass.cmp(&adze)) {
        None => WoodworkingTimberBalance::Unavailable,
        Some(std::cmp::Ordering::Less) => WoodworkingTimberBalance::Saving,
        Some(std::cmp::Ordering::Equal) => WoodworkingTimberBalance::Neutral,
        Some(std::cmp::Ordering::Greater) => WoodworkingTimberBalance::Costlier,
    }
}

/// Decide whether a saw is worth its setup for the disclosed workload.
///
/// The maintained criterion protects both finite inputs: never spend the protected copper reserve,
/// never buy a saw that costs more timber than the adze route, and never pay setup attention that
/// the disclosed workload cannot repay. The reviewer's own play decides the real opportunity.
pub(super) fn woodworking_investment_decision(
    reserve_safe: bool,
    setup_attention_budget_met: bool,
    timber_balance: WoodworkingTimberBalance,
) -> (bool, WoodworkingInvestmentReason) {
    if timber_balance == WoodworkingTimberBalance::Unavailable {
        return (false, WoodworkingInvestmentReason::CopperSupplyLimited);
    }
    if !reserve_safe {
        return (false, WoodworkingInvestmentReason::CopperReserveProtected);
    }
    if timber_balance == WoodworkingTimberBalance::Costlier {
        return (
            false,
            WoodworkingInvestmentReason::PipelineTimberCostNotRecovered,
        );
    }
    if !setup_attention_budget_met {
        return (
            false,
            if timber_balance == WoodworkingTimberBalance::Neutral {
                WoodworkingInvestmentReason::PipelineTimberNeutralOutsideSetupBudget
            } else {
                WoodworkingInvestmentReason::SetupAttentionBudgetExceeded
            },
        );
    }
    (
        true,
        if timber_balance == WoodworkingTimberBalance::Neutral {
            WoodworkingInvestmentReason::PipelineTimberNeutralWithinSetupBudget
        } else {
            WoodworkingInvestmentReason::PipelineNetTimberSaving
        },
    )
}

#[cfg(test)]
include_woodworking_policy_contract_tests!();
