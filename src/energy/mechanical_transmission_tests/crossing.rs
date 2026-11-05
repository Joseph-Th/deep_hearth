//! Intersecting materialized shafts must allocate their shared roller to useful work.

use super::*;
use crate::content::{
    ENERGY_MECHANICAL_SMALL_DRIVE, FORM_HANDLE, MATERIAL_WOOD,
    STRUCTURAL_PROFILE_TIMBER_DRIVE_COUPLING, build_registries,
};
use crate::core::quantity::{Area, Length};
use crate::core::state::validate_loaded_state;
use crate::energy::add_energy_store_with_initial_for_fixture;
use crate::logistics::place_energy_store_for_fixture;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::simulation::advance_tick;
use crate::spatial::VoxelBounds;
use crate::structural::{
    add_structural_element, make_test_structural_geometry, materialize_structural_element_for_test,
    validate_activate_structural_element,
};

pub(super) fn materialize_roller_at(
    registries: &Registries,
    state: &mut AppState,
    x: i64,
    z: i64,
) -> StructuralElementId {
    let roller = add_structural_element(
        registries,
        state,
        STRUCTURAL_PROFILE_TIMBER_DRIVE_COUPLING,
        MATERIAL_WOOD,
        make_test_structural_geometry(
            VoxelBounds::new(VoxelCoord::new(x, 0, z), VoxelCoord::new(x + 1, 1, z + 1)).unwrap(),
            Length::from_micrometers(1_000_000),
            Area::from_square_millimeters(300),
        ),
        true,
    )
    .unwrap();
    materialize_structural_element_for_test(registries, state, roller, FORM_HANDLE);
    let _ = validate_activate_structural_element(registries, state, roller)
        .unwrap()
        .commit(state)
        .unwrap();
    roller
}

#[test]
fn crossing_shaft_prioritizes_useful_transfer_over_coordinate_scan_order() {
    let registries = build_registries();
    let mut state = AppState::new();
    let definition = ENERGY_MECHANICAL_SMALL_DRIVE;
    let capacity = registries
        .energy()
        .get_store(definition)
        .unwrap()
        .capacity();
    // Both axes require the central roller. The horizontal pair is nearly
    // equalized; the vertical pair has a full source and empty receiver.
    let nearly_full = capacity
        .checked_sub(Energy::from_nanojoules(10_000_000_000))
        .unwrap();
    let mut stores = Vec::new();
    for (position, charge) in [
        (VoxelCoord::new(-1, 0, 0), capacity),
        (VoxelCoord::new(3, 0, 0), nearly_full),
        (VoxelCoord::new(1, 0, -2), capacity),
        (VoxelCoord::new(1, 0, 2), Energy::ZERO),
    ] {
        let id =
            add_energy_store_with_initial_for_fixture(&registries, &mut state, definition, charge)
                .unwrap();
        place_energy_store_for_fixture(&mut state, id, position);
        stores.push(id);
    }
    let rollers = [(0, 0), (1, 0), (2, 0), (1, -1), (1, 1)]
        .into_iter()
        .map(|(x, z)| materialize_roller_at(&registries, &mut state, x, z))
        .collect::<Vec<_>>();
    let passive =
        crate::energy::passive_dissipation::decide_passive_energy_dissipation(&registries, &state);
    let horizontal = plan_pair(
        &registries,
        &state,
        &passive,
        rollers[0],
        3,
        stores[0],
        stores[1],
    )
    .unwrap();
    let vertical = plan_pair(
        &registries,
        &state,
        &passive,
        rollers[3],
        3,
        stores[2],
        stores[3],
    )
    .unwrap();
    assert!(vertical.delivered() > horizontal.delivered());
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let planned = decide_mechanical_transmission(&registries, &state, &passive, None);
    assert_eq!(planned.transfers, vec![vertical]);

    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let receipt = advance_tick(&registries, &mut state).unwrap();
    assert_eq!(receipt, advance_tick(&registries, &mut replay).unwrap());
    assert_eq!(state, replay);
    assert_eq!(receipt.mechanical_transmissions(), &[vertical]);
    assert_eq!(
        state.energy().get_store(stores[0]).unwrap().stored(),
        capacity
    );
    assert_eq!(
        state.energy().get_store(stores[1]).unwrap().stored(),
        nearly_full
    );
    assert_eq!(
        state.energy().get_store(stores[2]).unwrap().stored(),
        capacity.checked_sub(vertical.drawn()).unwrap()
    );
    assert_eq!(
        state.energy().get_store(stores[3]).unwrap().stored(),
        vertical.delivered()
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn disjoint_shaft_runs_can_use_different_stores_at_one_shared_endpoint() {
    let registries = build_registries();
    let mut state = AppState::new();
    let definition = ENERGY_MECHANICAL_SMALL_DRIVE;
    let capacity = registries
        .energy()
        .get_store(definition)
        .unwrap()
        .capacity();
    let half = Energy::from_nanojoules(capacity.nanojoules() / 2);
    let mut stores = Vec::new();
    for (position, charge) in [
        (VoxelCoord::new(2, 0, 0), capacity),
        (VoxelCoord::new(2, 0, 0), half),
        (VoxelCoord::new(0, 0, 0), Energy::ZERO),
        (VoxelCoord::new(2, 0, 2), Energy::ZERO),
    ] {
        let id =
            add_energy_store_with_initial_for_fixture(&registries, &mut state, definition, charge)
                .unwrap();
        place_energy_store_for_fixture(&mut state, id, position);
        stores.push(id);
    }
    let x_roller = materialize_roller_at(&registries, &mut state, 1, 0);
    let z_roller = materialize_roller_at(&registries, &mut state, 2, 1);
    let passive =
        crate::energy::passive_dissipation::decide_passive_energy_dissipation(&registries, &state);
    let planned = decide_mechanical_transmission(&registries, &state, &passive, None);
    assert_eq!(planned.transfers.len(), 2);
    let [x, z] = [planned.transfers[0], planned.transfers[1]];
    assert_eq!(x.coupling(), x_roller);
    assert_eq!(x.source(), stores[0]);
    assert_eq!(x.destination(), stores[2]);
    assert_eq!(z.coupling(), z_roller);
    assert_eq!(z.source(), stores[1]);
    assert_eq!(z.destination(), stores[3]);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let before = state.clone();
    let settled = advance_tick(&registries, &mut state).unwrap();
    assert_eq!(settled.mechanical_transmissions(), &[x, z]);
    assert_eq!(
        state.energy().get_store(stores[0]).unwrap().stored(),
        capacity.checked_sub(x.drawn()).unwrap()
    );
    assert_eq!(
        state.energy().get_store(stores[1]).unwrap().stored(),
        half.checked_sub(z.drawn()).unwrap()
    );
    assert_eq!(
        state.energy().get_store(stores[2]).unwrap().stored(),
        x.delivered()
    );
    assert_eq!(
        state.energy().get_store(stores[3]).unwrap().stored(),
        z.delivered()
    );
    let before_total = [stores[0], stores[1], stores[2], stores[3]]
        .into_iter()
        .map(|id| before.energy().get_store(id).unwrap().stored().nanojoules())
        .sum::<u128>();
    let after_total = [stores[0], stores[1], stores[2], stores[3]]
        .into_iter()
        .map(|id| state.energy().get_store(id).unwrap().stored().nanojoules())
        .sum::<u128>();
    assert_eq!(
        after_total + x.lost().nanojoules() + z.lost().nanojoules(),
        before_total
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

/// One high-output crossing run can still be worse than two independently usable
/// shafts. Selecting the single largest transfer must not discard their combined work.
#[test]
fn two_independent_shafts_outdeliver_one_conflicting_stronger_shaft() {
    let registries = build_registries();
    let mut state = AppState::new();
    let definition = ENERGY_MECHANICAL_SMALL_DRIVE;
    let capacity = registries
        .energy()
        .get_store(definition)
        .unwrap()
        .capacity();
    let mut stores = Vec::new();
    for (position, charge) in [
        (VoxelCoord::new(-1, 0, 0), capacity),
        (VoxelCoord::new(2, 0, 0), Energy::ZERO),
        (VoxelCoord::new(0, 0, -2), capacity),
        (VoxelCoord::new(0, 0, 2), Energy::ZERO),
        (VoxelCoord::new(1, 0, -2), capacity),
        (VoxelCoord::new(1, 0, 2), Energy::ZERO),
    ] {
        let id =
            add_energy_store_with_initial_for_fixture(&registries, &mut state, definition, charge)
                .unwrap();
        place_energy_store_for_fixture(&mut state, id, position);
        stores.push(id);
    }
    let rollers = [(0, -1), (0, 0), (0, 1), (1, -1), (1, 0), (1, 1)]
        .into_iter()
        .map(|(x, z)| materialize_roller_at(&registries, &mut state, x, z))
        .collect::<Vec<_>>();
    let passive =
        crate::energy::passive_dissipation::decide_passive_energy_dissipation(&registries, &state);
    let horizontal = plan_pair(
        &registries,
        &state,
        &passive,
        rollers[1],
        2,
        stores[0],
        stores[1],
    )
    .unwrap();
    let left = plan_pair(
        &registries,
        &state,
        &passive,
        rollers[0],
        3,
        stores[2],
        stores[3],
    )
    .unwrap();
    let right = plan_pair(
        &registries,
        &state,
        &passive,
        rollers[3],
        3,
        stores[4],
        stores[5],
    )
    .unwrap();
    assert!(horizontal.delivered() > left.delivered());
    assert!(horizontal.delivered() > right.delivered());
    assert!(
        horizontal.delivered().nanojoules()
            < left.delivered().nanojoules() + right.delivered().nanojoules()
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let planned = decide_mechanical_transmission(&registries, &state, &passive, None);
    assert_eq!(planned.transfers, vec![left, right]);

    let before_total = stores
        .iter()
        .map(|&id| state.energy().get_store(id).unwrap().stored().nanojoules())
        .sum::<u128>();
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let outcome = advance_tick(&registries, &mut state).unwrap();
    assert_eq!(outcome, advance_tick(&registries, &mut replay).unwrap());
    assert_eq!(state, replay);
    assert_eq!(outcome.mechanical_transmissions(), &[left, right]);
    let after_total = stores
        .iter()
        .map(|&id| state.energy().get_store(id).unwrap().stored().nanojoules())
        .sum::<u128>();
    assert_eq!(
        after_total + left.lost().nanojoules() + right.lost().nanojoules(),
        before_total
    );
    assert_eq!(
        state.energy().get_store(stores[0]).unwrap().stored(),
        capacity,
    );
    assert_eq!(
        state.energy().get_store(stores[1]).unwrap().stored(),
        Energy::ZERO,
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
