//! Structural installation fixtures shared only by industrial gameplay capability targets.

use deep_hearth::content::gameplay_fixture::materialize_structure;
use deep_hearth::content::{FORM_LOG, MATERIAL_WOOD, STRUCTURAL_PROFILE_AXIAL_COMPRESSION};
use deep_hearth::core::quantity::{Area, Length};
use deep_hearth::core::state::AppState;
use deep_hearth::equipment::{EquipmentId, validate_mount_equipment};
use deep_hearth::registry::Registries;
use deep_hearth::spatial::{VoxelBounds, VoxelCoord};
use deep_hearth::structural::{
    StructuralElementGeometry, StructuralElementId, StructuralLifecycle, add_structural_element,
    validate_activate_structural_element,
};

/// Installs one authored fixed machine on a simple materialized grounded timber foundation.
///
/// Capability probes bootstrap acquisition, not operation. Once the equipment exists it must obey
/// the same structural installation contract as runtime industrial machinery.
pub(super) fn install_equipment_on_grounded_support(
    registries: &Registries,
    state: &mut AppState,
    equipment: EquipmentId,
    x: i64,
) -> StructuralElementId {
    let definition = state
        .equipment()
        .get_equipment(equipment)
        .and_then(|record| registries.equipment().get_equipment(record.definition()))
        .unwrap_or_else(|| panic!("gameplay harness installed equipment definition disappeared"));
    assert!(
        definition.requires_structural_support(),
        "gameplay harness installation helper received portable equipment {}",
        equipment.value()
    );
    let bounds = VoxelBounds::new(VoxelCoord::new(x, 0, 0), VoxelCoord::new(x + 1, 1, 1))
        .unwrap_or_else(|error| panic!("gameplay harness machine-support bounds failed: {error}"));
    let geometry = StructuralElementGeometry::new(
        bounds,
        Length::from_micrometers(1_000_000),
        Area::from_square_millimeters(10_000),
    )
    .unwrap_or_else(|error| panic!("gameplay harness machine-support geometry failed: {error}"));
    let support = add_structural_element(
        registries,
        state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        geometry,
        true,
    )
    .unwrap_or_else(|error| panic!("gameplay harness machine support failed: {error}"));
    materialize_structure(registries, state, support, FORM_LOG);
    validate_activate_structural_element(registries, state, support)
        .unwrap_or_else(|error| {
            panic!("gameplay harness machine-support activation failed: {error}")
        })
        .commit(state)
        .unwrap_or_else(|error| panic!("gameplay harness machine-support commit failed: {error}"));
    validate_mount_equipment(registries, state, equipment, support)
        .unwrap_or_else(|error| panic!("gameplay harness machine installation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| {
            panic!("gameplay harness machine installation commit failed: {error}")
        });
    assert_eq!(
        state
            .structures()
            .get_element(support)
            .map(|record| record.lifecycle()),
        Some(StructuralLifecycle::Active),
        "gameplay harness foundation must remain active after machine installation"
    );
    support
}
