//! Downward fluid movement redistributes real member loads, not just free volume.

use super::*;
use crate::energy::calculate_explicit_energy_accounting;
use crate::fluid::validate_gravity_feed_fluid_to_lower_store;

fn lower_support(registries: &Registries, state: &mut AppState) -> StructuralElementId {
    let extent = VoxelBounds::new(VoxelCoord::new(0, -1, 0), VoxelCoord::new(1, 0, 1)).unwrap();
    let support = add_structural_element(
        registries,
        state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        crate::structural::make_test_structural_geometry(
            extent,
            crate::core::quantity::Length::from_micrometers(1),
            Area::from_square_millimeters(1_000),
        ),
        true,
    )
    .unwrap();
    materialize_structural_element_for_test(registries, state, support, FORM_LOG);
    let _ = validate_activate_structural_element(registries, state, support)
        .unwrap()
        .commit(state)
        .unwrap();
    support
}

#[test]
fn downhill_feed_recalculates_two_distinct_support_loads_and_preserves_world_heat() {
    let registries = registries();
    let mut state = AppState::new();
    let upper = add_active_support(&registries, &mut state, 0);
    let lower = lower_support(&registries, &mut state);
    let source = add_filled(&registries, &mut state, 1_000_000);
    let destination = add_fluid_store(&mut state, Volume::from_microliters(1_000_000)).unwrap();
    let start = VoxelCoord::new(0, 0, 0);
    let bottom = VoxelCoord::new(0, -1, 0);
    for (store, position, support) in [(source, start, upper), (destination, bottom, lower)] {
        validate_place_fluid_store(&state, store, position)
            .unwrap()
            .commit(&mut state)
            .unwrap();
        let _ = mount(&registries, &mut state, store, support);
    }
    validate_admit_player(&registries, &state, start)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let water_before = calculate_fluid_volume_accounting(&state).unwrap();
    let heat_before = calculate_explicit_energy_accounting(&registries, &state)
        .unwrap()
        .total()
        .unwrap();
    let lower_before = state
        .structures()
        .get_element(lower)
        .unwrap()
        .load(StructuralLoadKind::Fluid);
    assert_eq!(
        lower_before,
        calculate_aggregate_weight_force_ceiling(AggregateMass::ZERO, registries.core().gravity())
            .unwrap()
    );

    let token = validate_gravity_feed_fluid_to_lower_store(
        &registries,
        &state,
        source,
        destination,
        Volume::from_microliters(300_000),
    )
    .unwrap();
    let outcome = token.commit(&mut state).unwrap();
    assert!(outcome.structural_analysis().is_some());
    let expected_weight = |milligrams| {
        calculate_aggregate_weight_force_ceiling(
            AggregateMass::from_milligrams(milligrams),
            registries.core().gravity(),
        )
        .unwrap()
    };
    assert_eq!(
        state
            .structures()
            .get_element(upper)
            .unwrap()
            .load(StructuralLoadKind::Fluid),
        expected_weight(700_000)
    );
    assert_eq!(
        state
            .structures()
            .get_element(lower)
            .unwrap()
            .load(StructuralLoadKind::Fluid),
        expected_weight(300_000)
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
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(loaded, state);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
