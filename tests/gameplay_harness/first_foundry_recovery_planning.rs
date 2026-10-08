//! Pure capacity and feed projections for the inherited first-foundry ore-processing line.

use deep_hearth::content::{
    ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE, EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
    EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR, PROCESS_CRUSH_ORE, PROCESS_SEPARATE_NATIVE_COPPER,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::maintenance::Condition;
use deep_hearth::ore_processing::project_powered_ore_replenished_batch_capacity;
use deep_hearth::registry::Registries;

pub(super) fn projected_inherited_processing_batch_limit(
    registries: &Registries,
    crusher_condition: Condition,
    separator_condition: Condition,
) -> Option<Mass> {
    let crusher = project_powered_ore_replenished_batch_capacity(
        registries,
        PROCESS_CRUSH_ORE,
        EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
        crusher_condition,
        ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE,
    )
    .ok()?;
    let separator = project_powered_ore_replenished_batch_capacity(
        registries,
        PROCESS_SEPARATE_NATIVE_COPPER,
        EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
        separator_condition,
        ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE,
    )
    .ok()?;
    Some(crusher.min(separator))
}

pub(super) fn minimum_powered_ore_feed_for_target_recovery(
    registries: &Registries,
    target: Mass,
    copper_ppm: u32,
    processing_batch_limit: Mass,
) -> Option<Mass> {
    let separator = registries
        .ore_processing()
        .get_constituent_separation(PROCESS_SEPARATE_NATIVE_COPPER)?;
    separator.minimum_batched_homogeneous_feed_mass_for_target_recovery(
        target,
        copper_ppm,
        processing_batch_limit,
    )
}
