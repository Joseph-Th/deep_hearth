//! Controlled pre-admission logistics setup for tests and gameplay probes.

use crate::core::state::AppState;
use crate::energy::EnergyStoreId;
use crate::equipment::EquipmentId;
use crate::spatial::VoxelCoord;

/// Places one already-existing equipment instance before ordinary player admission.
///
/// This is fixture setup, not a gameplay command. The logistics owner still performs the location
/// mutation and advances its revision, so gameplay fixtures do not write logistics state directly.
pub(crate) fn place_equipment_for_fixture(
    state: &mut AppState,
    equipment: EquipmentId,
    position: VoxelCoord,
) {
    assert!(
        state.logistics().player().is_none(),
        "equipment fixture placement is only valid before player logistics admission"
    );
    assert!(
        state.equipment().get_equipment(equipment).is_some(),
        "equipment fixture placement references unknown equipment {}",
        equipment.value()
    );
    let expected_revision = state.logistics().revision();
    let next_revision = expected_revision
        .checked_add(1)
        .unwrap_or_else(|| panic!("equipment fixture placement exhausted logistics revisions"));
    state.logistics_state_mut().apply_equipment_placement(
        expected_revision,
        next_revision,
        equipment,
        position,
    );
}

/// Places one already-existing finite-energy store before ordinary player admission.
///
/// This mirrors fixture equipment placement: logistics remains the sole location owner and advances
/// its revision, while the fixture contributes only the pre-existing world position.
pub(crate) fn place_energy_store_for_fixture(
    state: &mut AppState,
    store: EnergyStoreId,
    position: VoxelCoord,
) {
    assert!(
        state.logistics().player().is_none(),
        "energy-store fixture placement is only valid before player logistics admission"
    );
    assert!(
        state.energy().get_store(store).is_some(),
        "energy-store fixture placement references unknown store {}",
        store.value()
    );
    let expected_revision = state.logistics().revision();
    let next_revision = expected_revision
        .checked_add(1)
        .unwrap_or_else(|| panic!("energy-store fixture placement exhausted logistics revisions"));
    state.logistics_state_mut().apply_energy_store_placement(
        expected_revision,
        next_revision,
        store,
        position,
    );
}
