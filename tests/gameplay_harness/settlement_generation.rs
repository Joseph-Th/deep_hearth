//! Replayable settlement investment-policy and maintained crossover generation.

use deep_hearth::registry::Registries;

use super::capital_investment_policy::CapitalInvestmentPolicy;
use super::inherited_condition::healthy_used_equipment_condition;
pub(super) use super::settlement_demand::{
    SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES, organic_lumber_batch_limits,
    organic_lumber_batches,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CrossoverWorkloads {
    pub(super) short_batches: u64,
    pub(super) marginal_batches: u64,
    pub(super) project_batches: u64,
}

fn workload_roll(value: u64, salt: u64) -> u64 {
    let mixed = (value ^ salt).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    mixed ^ mixed.rotate_right(29)
}

/// Selects workload sizes relative to a live production crossover.
///
/// Maintained witnesses use stable positions in the current regimes. Exploratory witnesses vary
/// within the same regimes so reports sample different real workloads without changing which side
/// of the discovered crossover they are intended to exercise.
pub(super) fn crossover_workloads(
    crossover_batches: u64,
    maximum_batches: u64,
    entropy: Option<u64>,
) -> CrossoverWorkloads {
    assert!(
        crossover_batches > 1 && crossover_batches <= maximum_batches,
        "settlement specialization requires both a pre-investment niche and a bounded crossover"
    );
    let short_minimum = (crossover_batches / 2).max(1);
    let short_maximum = crossover_batches - 1;
    let project_minimum = crossover_batches;
    let project_maximum = crossover_batches
        .saturating_add((crossover_batches / 2).max(1))
        .min(maximum_batches);
    let short_batches = entropy.map_or(short_minimum, |value| {
        let roll = workload_roll(value, 0x5348_4F52_545F_574C);
        short_minimum + roll % (short_maximum - short_minimum + 1)
    });
    let project_batches = entropy.map_or(project_minimum, |value| {
        let roll = workload_roll(value, 0x5052_4F4A_4543_5457);
        project_minimum + roll % (project_maximum - project_minimum + 1)
    });
    CrossoverWorkloads {
        short_batches,
        marginal_batches: crossover_batches - 1,
        project_batches,
    }
}

pub(super) fn organic_investment_policy(behavior_seed: u64) -> CapitalInvestmentPolicy {
    CapitalInvestmentPolicy::from_behavior_seed(behavior_seed)
}

pub(super) fn organic_inherited_equipment_condition(
    registries: &Registries,
    definition: deep_hearth::equipment::EquipmentDefinitionId,
    entropy: u64,
) -> deep_hearth::maintenance::Condition {
    healthy_used_equipment_condition(registries, definition, entropy)
}
