//! Trusted-load contracts for direct manual-power replay.

use super::*;

use crate::content::{EQUIPMENT_JAW_CRUSHER, build_registries};
use crate::core::state::AppState;
use crate::equipment::{EquipmentOperationTrace, add_equipment};
use crate::maintenance::Condition;

#[test]
fn trusted_replay_rejects_unmounted_equipment_that_requires_installation() {
    let registries = build_registries();
    let mut state = AppState::new();
    let equipment = add_equipment(
        &registries,
        &mut state,
        EQUIPMENT_JAW_CRUSHER,
        Condition::PRISTINE,
    )
    .unwrap_or_else(|error| panic!("fixed-equipment replay fixture failed: {error}"));
    let trace = EquipmentOperationTrace::new(equipment, EQUIPMENT_JAW_CRUSHER, Condition::PRISTINE);

    assert_eq!(
        validate_manual_power_equipment_record(&registries, &state, trace).err(),
        Some(PlayerWorkValidationError::ManualPowerEquipmentRequiresStructuralSupport)
    );
}
