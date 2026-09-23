//! Prospecting player-work projection contracts.

use super::*;
use crate::content::{MATERIAL_COPPER, PROSPECTING_FIELD_INSPECTION, build_registries};
use crate::core::quantity::{Energy, Volume};
use crate::core::state::AppState;
use crate::geology::{FieldProspectingRequest, validate_start_field_prospecting};
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::survival::{NutritionReserves, Vitality, initialize_player_survival, player_record};

fn one_voxel() -> VoxelBounds {
    VoxelBounds::new(VoxelCoord::new(0, -1, 0), VoxelCoord::new(1, 0, 1))
        .unwrap_or_else(|error| panic!("prospecting projection bounds failed: {error}"))
}

#[test]
fn prospecting_projection_matches_admitted_work_budget() {
    let registries = build_registries();
    let region = one_voxel();
    let projection = project_prospecting_work(&registries, PROSPECTING_FIELD_INSPECTION, region)
        .unwrap_or_else(|error| panic!("prospecting projection failed: {error}"));
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("prospecting projection survival setup failed: {error}"));
    let admitted = validate_start_field_prospecting(
        &registries,
        &state,
        FieldProspectingRequest::new(PROSPECTING_FIELD_INSPECTION, region, MATERIAL_COPPER),
    )
    .unwrap_or_else(|error| panic!("prospecting admission failed: {error}"));
    let admitted_duration = admitted
        .work()
        .completes_at()
        .checked_duration_since(admitted.work().started_at())
        .unwrap_or_else(|| panic!("prospecting admitted duration underflowed"));

    assert_eq!(projection.duration(), admitted_duration);
    assert_eq!(projection.resource_budget(), admitted.resource_budget());
    assert_eq!(projection.observation_count(), 1);
    assert!(!projection.requires_equipment());
}

#[test]
fn prospecting_projection_remains_available_when_current_reserve_cannot_admit_work() {
    let registries = build_registries();
    let region = one_voxel();
    let projection = project_prospecting_work(&registries, PROSPECTING_FIELD_INSPECTION, region)
        .unwrap_or_else(|error| panic!("prospecting projection failed: {error}"));
    assert!(projection.resource_budget().hydration() > Volume::ZERO);

    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("prospecting low-reserve survival setup failed: {error}"));
    let expected_revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        expected_revision,
        expected_revision + 1,
        player_record(
            Energy::from_nanojoules(1),
            Volume::from_microliters(1),
            Vitality::MAXIMUM,
            NutritionReserves::FULL,
            0,
        ),
    );

    assert!(
        validate_start_field_prospecting(
            &registries,
            &state,
            FieldProspectingRequest::new(PROSPECTING_FIELD_INSPECTION, region, MATERIAL_COPPER),
        )
        .is_err(),
        "low current reserve must block runtime admission without hiding the authored work projection"
    );
}
