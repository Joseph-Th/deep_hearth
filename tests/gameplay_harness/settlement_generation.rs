//! Replayable settlement demand and investment-policy generation.

use deep_hearth::equipment::EquipmentDefinitionId;
use deep_hearth::maintenance::Condition;
use deep_hearth::registry::Registries;

use super::capital_investment_policy::CapitalInvestmentPolicy;

pub(super) fn organic_investment_policy(behavior_seed: u64) -> CapitalInvestmentPolicy {
    CapitalInvestmentPolicy::from_behavior_seed(behavior_seed)
}

pub(super) fn organic_inherited_equipment_condition(
    registries: &Registries,
    definition: EquipmentDefinitionId,
    entropy: u64,
) -> Condition {
    let warning = registries
        .equipment()
        .get_equipment(definition)
        .unwrap_or_else(|| panic!("settlement inherited equipment definition disappeared"))
        .maintenance_thresholds()
        .warning_below()
        .parts_per_million();
    let lower = warning
        .checked_add((Condition::PRISTINE.parts_per_million() - warning) / 2)
        .unwrap_or_else(|| unreachable!("settlement healthy-condition midpoint fits u32"));
    let span = Condition::PRISTINE.parts_per_million() - lower;
    let offset = u32::try_from(entropy % u64::from(span))
        .unwrap_or_else(|_| unreachable!("bounded settlement inherited-condition offset fits u32"));
    Condition::new(lower + offset)
        .unwrap_or_else(|error| panic!("settlement inherited condition invalid: {error}"))
}

pub(super) fn organic_lumber_batches(
    entropy: u64,
    baseline_crossover_batches: Option<u64>,
    opportunity_batches: u64,
) -> u64 {
    assert!(
        opportunity_batches > 1,
        "settlement organic demand needs room for a lived follow-up order"
    );
    let maximum_order = opportunity_batches - 1;
    let Some(crossover) = baseline_crossover_batches else {
        return 1 + entropy % maximum_order;
    };
    let spread = (crossover / 2).max(1);
    let lower = crossover.saturating_sub(spread).max(1).min(maximum_order);
    let upper = crossover.saturating_add(spread).min(maximum_order);
    lower + entropy % (upper - lower + 1)
}
