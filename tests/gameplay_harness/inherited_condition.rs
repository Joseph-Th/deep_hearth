//! Shared healthy-wear generation for ordinary episodes that inherit previously used equipment.

use deep_hearth::equipment::EquipmentDefinitionId;
use deep_hearth::maintenance::Condition;
use deep_hearth::registry::Registries;

pub(super) fn healthy_used_equipment_condition(
    registries: &Registries,
    definition: EquipmentDefinitionId,
    entropy: u64,
) -> Condition {
    let warning = registries
        .equipment()
        .get_equipment(definition)
        .unwrap_or_else(|| panic!("gameplay inherited equipment definition disappeared"))
        .maintenance_thresholds()
        .warning_below()
        .parts_per_million();
    let lower = warning
        .checked_add((Condition::PRISTINE.parts_per_million() - warning) / 2)
        .unwrap_or_else(|| unreachable!("healthy-condition midpoint fits u32"));
    let span = Condition::PRISTINE.parts_per_million() - lower;
    let offset = u32::try_from(entropy % u64::from(span))
        .unwrap_or_else(|_| unreachable!("bounded inherited-condition offset fits u32"));
    Condition::new(lower + offset)
        .unwrap_or_else(|error| panic!("gameplay inherited condition invalid: {error}"))
}
