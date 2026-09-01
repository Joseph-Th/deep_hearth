//! Provides registry-derived condition variation and bounded production advancement for gameplay probes.

use deep_hearth::equipment::EquipmentDefinitionId;
use deep_hearth::maintenance::{CONDITION_PARTS_PER_MILLION, Condition};
use deep_hearth::registry::Registries;

pub(super) fn varied_healthy_condition(
    registries: &Registries,
    equipment: EquipmentDefinitionId,
    roll: u64,
) -> Condition {
    let definition = registries
        .equipment()
        .get_equipment(equipment)
        .unwrap_or_else(|| panic!("gameplay harness equipment definition disappeared"));
    let warning = definition
        .maintenance_thresholds()
        .warning_below()
        .parts_per_million();
    let healthy_span = CONDITION_PARTS_PER_MILLION.saturating_sub(warning);
    let lower = warning
        .saturating_add(healthy_span.div_ceil(2))
        .min(CONDITION_PARTS_PER_MILLION);
    let span = CONDITION_PARTS_PER_MILLION - lower;
    let value = lower
        + u32::try_from(roll % (u64::from(span) + 1)).unwrap_or_else(|_| {
            unreachable!("normalized gameplay condition variation always fits u32")
        });
    Condition::new(value)
        .unwrap_or_else(|error| panic!("gameplay harness varied condition failed: {error}"))
}
