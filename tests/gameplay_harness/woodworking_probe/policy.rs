//! Pre-action woodworking investment policy and its observable decision vocabulary.

use deep_hearth::core::quantity::Mass;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WoodworkingInvestmentPreference {
    ConserveScarceCopper,
    ConserveTimber,
}

impl WoodworkingInvestmentPreference {
    pub(super) const fn from_behavior_seed(seed: u64) -> Self {
        if seed.is_multiple_of(2) {
            Self::ConserveScarceCopper
        } else {
            Self::ConserveTimber
        }
    }

    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::ConserveScarceCopper => "conserve-scarce-copper",
            Self::ConserveTimber => "conserve-timber",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WoodworkingInvestmentReason {
    BareHandsAvoidsInvestmentCost,
    CopperSupplyLimited,
    CopperReserveProtected,
    PipelineTooShortForAttentionPayback,
    SurplusCopperAttentionPayback,
    PipelineTimberCostNotRecovered,
    PipelineTimberNeutralWithoutAttentionPayback,
    PipelineTimberNeutralAttentionPayback,
    PipelineNetTimberSaving,
}

impl WoodworkingInvestmentReason {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::BareHandsAvoidsInvestmentCost => "bare-hands-avoids-investment-cost",
            Self::CopperSupplyLimited => "copper-supply-limited",
            Self::CopperReserveProtected => "copper-reserve-protected",
            Self::PipelineTooShortForAttentionPayback => "pipeline-too-short-for-attention-payback",
            Self::SurplusCopperAttentionPayback => "surplus-copper-attention-payback",
            Self::PipelineTimberCostNotRecovered => "pipeline-timber-cost-not-recovered",
            Self::PipelineTimberNeutralWithoutAttentionPayback => {
                "pipeline-timber-neutral-without-attention-payback"
            }
            Self::PipelineTimberNeutralAttentionPayback => {
                "pipeline-timber-neutral-attention-payback"
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

pub(super) fn woodworking_investment_decision(
    preference: WoodworkingInvestmentPreference,
    reserve_safe: bool,
    attention_budget_met: bool,
    timber_balance: WoodworkingTimberBalance,
) -> (bool, WoodworkingInvestmentReason) {
    if timber_balance == WoodworkingTimberBalance::Unavailable {
        return (false, WoodworkingInvestmentReason::CopperSupplyLimited);
    }
    match preference {
        WoodworkingInvestmentPreference::ConserveScarceCopper => {
            conserve_copper_decision(reserve_safe, attention_budget_met)
        }
        WoodworkingInvestmentPreference::ConserveTimber => {
            conserve_timber_decision(timber_balance, attention_budget_met)
        }
    }
}

fn conserve_copper_decision(
    reserve_safe: bool,
    attention_budget_met: bool,
) -> (bool, WoodworkingInvestmentReason) {
    if !reserve_safe {
        return (false, WoodworkingInvestmentReason::CopperReserveProtected);
    }
    if !attention_budget_met {
        return (
            false,
            WoodworkingInvestmentReason::PipelineTooShortForAttentionPayback,
        );
    }
    (
        true,
        WoodworkingInvestmentReason::SurplusCopperAttentionPayback,
    )
}

fn conserve_timber_decision(
    timber_balance: WoodworkingTimberBalance,
    attention_budget_met: bool,
) -> (bool, WoodworkingInvestmentReason) {
    match timber_balance {
        WoodworkingTimberBalance::Costlier => (
            false,
            WoodworkingInvestmentReason::PipelineTimberCostNotRecovered,
        ),
        WoodworkingTimberBalance::Neutral if !attention_budget_met => (
            false,
            WoodworkingInvestmentReason::PipelineTimberNeutralWithoutAttentionPayback,
        ),
        WoodworkingTimberBalance::Neutral => (
            true,
            WoodworkingInvestmentReason::PipelineTimberNeutralAttentionPayback,
        ),
        WoodworkingTimberBalance::Saving => {
            (true, WoodworkingInvestmentReason::PipelineNetTimberSaving)
        }
        WoodworkingTimberBalance::Unavailable => {
            unreachable!("unavailable timber balance is handled before preference dispatch")
        }
    }
}

#[cfg(test)]
#[path = "policy_tests.rs"]
mod tests;
