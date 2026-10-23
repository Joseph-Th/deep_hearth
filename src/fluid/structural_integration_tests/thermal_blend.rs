//! Co-located fluid heat mixing and simultaneous structural mass projection.

use super::*;
use crate::energy::calculate_explicit_energy_accounting;

#[test]
fn local_thermal_blending_preserves_supported_weight_and_precise_energy_together() {
    let registries = registries();
    let mut state = AppState::new();
    let support = add_active_support(&registries, &mut state, 0);
    let source = add_filled(&registries, &mut state, 400_000);
    let destination = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        Volume::from_microliters(400_000),
        TEST_FLUID,
        Volume::from_microliters(100_000),
        Temperature::from_millikelvin(303_150),
    )
    .unwrap();
    let position = VoxelCoord::new(0, 0, 0);
    for store in [source, destination] {
        validate_place_fluid_store(&state, store, position)
            .unwrap()
            .commit(&mut state)
            .unwrap();
        let _ = mount(&registries, &mut state, store, support);
    }
    validate_admit_player(&registries, &state, position)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let water_before = calculate_fluid_volume_accounting(&state).unwrap();
    let heat_before = calculate_explicit_energy_accounting(&registries, &state)
        .unwrap()
        .total()
        .unwrap();
    let weight_before = state
        .structures()
        .get_element(support)
        .unwrap()
        .load(StructuralLoadKind::Fluid);
    let structure_revision = state.structures().revision();

    let blending = validate_transfer_fluid_between_local_stores(
        &registries,
        &state,
        source,
        destination,
        Volume::from_microliters(100_000),
    )
    .unwrap();
    let result = blending.commit(&mut state).unwrap();
    assert!(result.structural_analysis().is_none());
    assert_eq!(state.structures().revision(), structure_revision);
    assert_eq!(
        state
            .structures()
            .get_element(support)
            .unwrap()
            .load(StructuralLoadKind::Fluid),
        weight_before
    );
    assert_eq!(
        state
            .fluid()
            .get_store(destination)
            .unwrap()
            .contents()
            .unwrap()
            .temperature(),
        Temperature::from_millikelvin(298_150),
    );
    assert_eq!(
        calculate_fluid_volume_accounting(&state).unwrap(),
        water_before
    );
    assert_eq!(
        calculate_explicit_energy_accounting(&registries, &state)
            .unwrap()
            .total()
            .unwrap(),
        heat_before
    );
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let resumed = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(state, resumed);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
