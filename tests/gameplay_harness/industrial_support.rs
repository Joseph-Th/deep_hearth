//! Structural installation fixtures shared only by industrial gameplay capability targets.

use super::structural_fixture::support_area_for_utilization;
use deep_hearth::content::gameplay_fixture::seed_grounded_active_structure;
use deep_hearth::content::{FORM_LOG, MATERIAL_WOOD, STRUCTURAL_PROFILE_AXIAL_COMPRESSION};
use deep_hearth::core::quantity::Length;
use deep_hearth::core::state::AppState;
use deep_hearth::equipment::{EquipmentId, validate_mount_equipment};
use deep_hearth::registry::Registries;
use deep_hearth::spatial::{VoxelBounds, VoxelCoord};
use deep_hearth::structural::{
    StructuralElementGeometry, StructuralElementId, StructuralLifecycle, StructuralStage,
    calculate_weight_force_ceiling,
};

const INDUSTRIAL_SUPPORT_LENGTH: Length = Length::from_micrometers(1_000_000);

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
        "gameplay harness installation setup received portable equipment {}",
        equipment.value()
    );
    let bounds = VoxelBounds::new(VoxelCoord::new(x, 0, 0), VoxelCoord::new(x + 1, 1, 1))
        .unwrap_or_else(|error| panic!("gameplay harness machine-support bounds failed: {error}"));
    let support_profile = registries
        .structural()
        .get_profile(STRUCTURAL_PROFILE_AXIAL_COMPRESSION)
        .unwrap_or_else(|| panic!("gameplay harness compression profile disappeared"));
    let stable_target_ppm = support_profile.strained_at_ppm().div_ceil(2).max(1);
    let equipment_weight =
        calculate_weight_force_ceiling(definition.mass(), registries.core().gravity());
    let support_area = support_area_for_utilization(
        registries,
        MATERIAL_WOOD,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        INDUSTRIAL_SUPPORT_LENGTH,
        equipment_weight,
        stable_target_ppm,
    );
    let geometry = StructuralElementGeometry::new(bounds, INDUSTRIAL_SUPPORT_LENGTH, support_area)
        .unwrap_or_else(|error| {
            panic!("gameplay harness machine-support geometry failed: {error}")
        });
    let support = seed_grounded_active_structure(
        registries,
        state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        geometry,
        FORM_LOG,
    );
    let mounting = validate_mount_equipment(registries, state, equipment, support)
        .unwrap_or_else(|error| panic!("gameplay harness machine installation failed: {error}"));
    let assessment = mounting
        .structural_analysis()
        .assessments()
        .iter()
        .find(|assessment| assessment.element() == support)
        .copied()
        .unwrap_or_else(|| panic!("gameplay harness machine-support assessment disappeared"));
    assert_eq!(
        assessment.stage(),
        StructuralStage::Stable,
        "gameplay capability foundation sizing drifted out of the stable structural band"
    );
    assert!(
        assessment.utilization_ppm() <= u128::from(stable_target_ppm),
        "gameplay capability foundation sizing drifted above its production utilization target"
    );
    let _ = mounting.commit(state).unwrap_or_else(|error| {
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
