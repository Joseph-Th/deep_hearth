//! Physical crowded mechanical-shaft selection and energy custody proof.

use super::crossing::materialize_roller_at;
use super::*;
use crate::core::state::validate_loaded_state;
use crate::energy::add_energy_store_with_initial_for_fixture;
use crate::logistics::place_energy_store_for_fixture;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::simulation::advance_tick;

/// A real five-path crossing workshop has two strong but competing horizontal
/// shafts and three lower-output independent paths. Adding twelve weak source
/// stores forces arbitration past its exact solver without increasing usable
/// roller capacity or reducing the aggregate useful work.
#[test]
fn crowded_physical_shafts_preserve_delivery_losses_and_save_replay() {
    use crate::content::make_test_registries_with_energy_store;
    use crate::core::quantity::Power;
    use crate::energy::{EnergyCarrier, EnergyStoreDefinition, EnergyStoreDefinitionId};
    use crate::matter::calculate_matter_accounting;

    let definition = EnergyStoreDefinitionId::new(990_739);
    let registries =
        make_test_registries_with_energy_store(EnergyStoreDefinition::new_with_transfer_limits(
            definition,
            "crowded workshop mechanical accumulator",
            EnergyCarrier::Mechanical,
            Energy::from_nanojoules(300_000_000_000),
            Power::from_microwatts(50_000_000),
            Power::from_microwatts(50_000_000),
        ));
    let mut state = AppState::new();
    let rollers = [(1, 0), (2, 0), (1, 1), (1, 2), (2, 2)]
        .into_iter()
        .map(|(x, z)| ((x, z), materialize_roller_at(&registries, &mut state, x, z)))
        .collect::<BTreeMap<_, _>>();

    // Two horizontal two-roller shafts each deliver 80 J. One vertical
    // three-roller shaft and two separate one-roller shafts deliver 60 J each.
    // Co-located stores at (2, 0, 1) remain distinct physical custody.
    let mut stores = Vec::new();
    for (position, charge) in [
        (VoxelCoord::new(0, 0, 0), 205_000_000_000_u128),
        (VoxelCoord::new(3, 0, 0), 0),
        (VoxelCoord::new(0, 0, 2), 205_000_000_000),
        (VoxelCoord::new(3, 0, 2), 0),
        (VoxelCoord::new(1, 0, -1), 177_187_500_000),
        (VoxelCoord::new(1, 0, 3), 0),
        (VoxelCoord::new(2, 0, -1), 135_000_000_000),
        (VoxelCoord::new(2, 0, 1), 0),
        (VoxelCoord::new(2, 0, 1), 135_000_000_000),
        (VoxelCoord::new(2, 0, 3), 0),
    ] {
        let store = add_energy_store_with_initial_for_fixture(
            &registries,
            &mut state,
            definition,
            Energy::from_nanojoules(charge),
        )
        .unwrap();
        place_energy_store_for_fixture(&mut state, store, position);
        stores.push(store);
    }
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let passive =
        crate::energy::passive_dissipation::decide_passive_energy_dissipation(&registries, &state);
    let candidate = |roller, segments, source, destination| {
        plan_pair(
            &registries,
            &state,
            &passive,
            rollers[&roller],
            segments,
            stores[source],
            stores[destination],
        )
        .unwrap()
    };
    let horizontal = [candidate((1, 0), 2, 0, 1), candidate((1, 2), 2, 2, 3)];
    let vertical = [
        candidate((1, 0), 3, 4, 5),
        candidate((2, 0), 1, 6, 7),
        candidate((2, 2), 1, 8, 9),
    ];
    for path in horizontal {
        assert_eq!(path.delivered().nanojoules(), 80_000_000_000);
    }
    for path in vertical {
        assert_eq!(path.delivered().nanojoules(), 60_000_000_000);
    }
    let baseline = decide_mechanical_transmission(&registries, &state, &passive, None);
    assert_eq!(baseline.transfers.len(), 3);
    for path in vertical {
        assert!(baseline.transfers.contains(&path));
    }

    // Each of the twelve extra sources can deliver positive energy to the
    // horizontal receiver, giving seventeen feasible candidates in one
    // connected roller-conflict group instead of sixteen.
    for added in 0..12 {
        let weak = add_energy_store_with_initial_for_fixture(
            &registries,
            &mut state,
            definition,
            Energy::from_nanojoules(3_000_000_000),
        )
        .unwrap();
        place_energy_store_for_fixture(&mut state, weak, VoxelCoord::new(0, 0, 0));
        assert!(
            plan_pair(
                &registries,
                &state,
                &passive,
                rollers[&(1, 0)],
                2,
                weak,
                stores[1]
            )
            .is_some()
        );
        if added == 10 {
            // Exactly sixteen physically feasible candidates still use the
            // exact arbiter. The next real source crosses its solver boundary.
            let at_limit = decide_mechanical_transmission(&registries, &state, &passive, None);
            assert_eq!(at_limit.transfers, baseline.transfers);
        }
    }
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let planned = decide_mechanical_transmission(&registries, &state, &passive, None);
    assert_eq!(planned.transfers, baseline.transfers);
    let before = state.clone();
    let total_before = before
        .energy()
        .stores()
        .map(|store| store.stored().nanojoules())
        .sum::<u128>();
    let matter_before = calculate_matter_accounting(&before).unwrap().total();
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let outcome = advance_tick(&registries, &mut state).unwrap();
    assert_eq!(outcome, advance_tick(&registries, &mut replay).unwrap());
    assert_eq!(state, replay);
    assert_eq!(
        outcome.mechanical_transmissions(),
        planned.transfers.as_slice()
    );
    let friction = outcome
        .mechanical_transmissions()
        .iter()
        .map(|path| path.lost().nanojoules())
        .sum::<u128>();
    let total_after = state
        .energy()
        .stores()
        .map(|store| store.stored().nanojoules())
        .sum::<u128>();
    assert_eq!(total_after + friction, total_before);
    for path in outcome.mechanical_transmissions() {
        let source = before.energy().get_store(path.source()).unwrap().stored();
        let destination = before
            .energy()
            .get_store(path.destination())
            .unwrap()
            .stored();
        assert_eq!(
            state.energy().get_store(path.source()).unwrap().stored(),
            source.checked_sub(path.drawn()).unwrap()
        );
        assert_eq!(
            state
                .energy()
                .get_store(path.destination())
                .unwrap()
                .stored(),
            destination.checked_add(path.delivered()).unwrap()
        );
    }
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
