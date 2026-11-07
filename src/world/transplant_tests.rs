//! Juvenile relocation uses one persistent biological identity and no new biomass.

use super::felling::validate_fell_tree;
use super::*;
use crate::content::{BLOCK_STATE_STONE, build_registries};
use crate::core::quantity::Mass;
use crate::core::state::{AppState, StateValidationError, validate_loaded_state};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::spatial::VoxelCoord;

fn sapling_fixture() -> (
    crate::registry::Registries,
    AppState,
    VoxelCoord,
    VoxelCoord,
) {
    tree_fixture(true)
}

fn tree_fixture(
    juvenile: bool,
) -> (
    crate::registry::Registries,
    AppState,
    VoxelCoord,
    VoxelCoord,
) {
    let registries = build_registries();
    let (seed, from) = (1..10_000)
        .find_map(|value| {
            let seed = WorldGenerationSeed::new(value);
            let profile = WorldGenerationProfile::new(seed, BLOCK_STATE_STONE);
            let original = trees::seeded_tree_identity(profile, ChunkCoord::new(0, 0))?;
            let from = original.root();
            let across = from.x().checked_sub(2)?;
            ((original.initial_wood().milligrams() == 8_000_000) == juvenile
                && generation::terrain_height(seed, from.x() - 1, from.z()) == from.y() - 1
                && generation::terrain_height(seed, across, from.z()) == from.y() - 1)
                .then_some((seed, from))
        })
        .expect("a generated tree with two level adjacent floor cells must exist");
    let to = VoxelCoord::new(from.x() - 2, from.y(), from.z());
    let mut state = AppState::new();
    validate_admit_player(
        &registries,
        &state,
        VoxelCoord::new(from.x() - 1, from.y(), from.z()),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert!(state.world().tree_at(from).is_some());
    assert!(state.world().has_standing_clearance(registries.world(), to));
    (registries, state, from, to)
}

#[test]
fn locally_transplanted_sapling_keeps_its_growth_clock_and_exact_wood() {
    let (registries, mut state, from, to) = sapling_fixture();
    let tree_before = *state.world().tree_at(from).unwrap();
    let matter_before = calculate_matter_accounting(&state).unwrap().total();
    let planted = validate_transplant_juvenile(&registries, &state, from, to)
        .unwrap()
        .commit(&registries, &mut state)
        .unwrap();
    assert_eq!(planted.from(), from);
    assert_eq!(planted.to(), to);
    assert_eq!(planted.wood(), tree_before.wood_at(state.tick()));
    assert!(state.world().tree_at(from).is_none());
    let moved = *state.world().tree_at(to).unwrap();
    assert_eq!(moved.origin_root(), from);
    assert_eq!(moved.initial_wood(), tree_before.initial_wood());
    assert_eq!(moved.sprouted_at(), tree_before.sprouted_at());
    assert_eq!(
        moved.wood_at(state.tick()),
        tree_before.wood_at(state.tick())
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let replay = serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(state, replay);

    // The moved tree may still be harvested. Its felling receipt must account
    // for the original seed origin, not a fictitious natural site at the new root.
    let felled = validate_fell_tree(&registries, &state, to)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(felled.wood(), planted.wood());
    assert!(state.world().tree_at(to).is_none());
    assert_eq!(
        state.world().felled_trees().next().unwrap().origin_root(),
        from
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    assert_eq!(
        serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
            .unwrap()
            .into_state(&registries)
            .unwrap(),
        state
    );
}

#[test]
fn sapling_transplant_prechecks_blockers_and_stale_revisions_without_mutating() {
    let (registries, mut state, from, to) = sapling_fixture();
    let blocked = validate_transplant_juvenile(&registries, &state, from, from);
    assert!(matches!(
        blocked,
        Err(SaplingTransplantError::DestinationBlocked { .. })
    ));
    let remote = validate_transplant_juvenile(
        &registries,
        &state,
        from,
        VoxelCoord::new(to.x() - 1, to.y(), to.z()),
    );
    assert!(matches!(
        remote,
        Err(SaplingTransplantError::DestinationRemote { .. })
    ));

    let transplant = validate_transplant_juvenile(&registries, &state, from, to).unwrap();
    let felled = validate_fell_tree(&registries, &state, from)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let after = state.clone();
    assert_eq!(
        transplant.commit(&registries, &mut state),
        Err(SaplingTransplantCommitError::WorldChanged),
    );
    assert_eq!(state, after);
    assert_eq!(
        state.world().felled_trees().next().unwrap().root(),
        felled.root()
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn remote_sapling_queries_reject_reach_before_disclosing_tree_presence_or_age() {
    let (registries, state, from, to) = sapling_fixture();
    let missing_remote = VoxelCoord::new(from.x() + 3, from.y(), from.z());
    assert!(state.world().tree_at(missing_remote).is_none());
    let before = state.clone();
    assert_eq!(
        validate_transplant_juvenile(&registries, &state, missing_remote, to).err(),
        Some(SaplingTransplantError::SourceRemote {
            root: missing_remote,
        })
    );
    assert_eq!(state, before);

    let (registries, state, mature, to) = tree_fixture(false);
    let distant_destination = VoxelCoord::new(to.x() - 1, to.y(), to.z());
    assert_eq!(
        validate_transplant_juvenile(&registries, &state, mature, distant_destination).err(),
        Some(SaplingTransplantError::DestinationRemote {
            root: distant_destination,
        })
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn trusted_load_rejects_forged_transplant_origin_and_duplicate_biomass() {
    let (registries, mut state, from, to) = sapling_fixture();
    let _ = validate_transplant_juvenile(&registries, &state, from, to)
        .unwrap()
        .commit(&registries, &mut state)
        .unwrap();
    let baseline = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    for corruption in ["missing_origin", "wrong_origin", "cloned_tree"] {
        let mut value = baseline.clone();
        let trees = value["state"]["systems"]["world"]["trees"]
            .as_array_mut()
            .unwrap();
        match corruption {
            "missing_origin" => {
                trees[0].as_object_mut().unwrap().remove("origin");
            }
            "wrong_origin" => {
                trees[0]["origin"]["x"] = serde_json::json!(from.x() + 1);
            }
            "cloned_tree" => {
                let mut clone = trees[0].clone();
                clone["root"]["x"] = serde_json::json!(from.x());
                trees.push(clone);
            }
            _ => unreachable!(),
        }
        let decoded = serde_json::from_value::<LoadedSaveEnvelope>(value);
        if corruption == "missing_origin" {
            assert!(decoded.is_err(), "required origin field cannot be omitted");
            continue;
        }
        let load = decoded.unwrap().into_state(&registries);
        assert!(
            matches!(
                load,
                Err(crate::persistence::LoadError::InvalidState(
                    StateValidationError::World(WorldValidationError::InvalidStandingTree { .. })
                ))
            ),
            "{corruption}: {load:?}"
        );
    }
}

#[test]
fn transplanting_back_to_the_seed_site_restores_canonical_origin_without_duplication() {
    let (registries, mut state, from, to) = sapling_fixture();
    let initial = *state.world().tree_at(from).unwrap();
    let _ = validate_transplant_juvenile(&registries, &state, from, to)
        .unwrap()
        .commit(&registries, &mut state)
        .unwrap();
    let _ = validate_transplant_juvenile(&registries, &state, to, from)
        .unwrap()
        .commit(&registries, &mut state)
        .unwrap();
    assert_eq!(state.world().trees().count(), 1);
    assert_eq!(state.world().tree_at(from), Some(&initial));
    assert!(state.world().tree_at(to).is_none());
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    assert_eq!(
        saved["state"]["systems"]["world"]["trees"][0]["origin"]["x"],
        serde_json::json!(from.x()),
    );
    assert_eq!(
        serde_json::from_value::<LoadedSaveEnvelope>(saved)
            .unwrap()
            .into_state(&registries)
            .unwrap(),
        state,
    );
}

#[test]
fn mature_standing_tree_is_not_eligible_for_juvenile_transplantation() {
    let (registries, state, from, to) = tree_fixture(false);
    let before = state.clone();
    assert!(matches!(
        validate_transplant_juvenile(&registries, &state, from, to),
        Err(SaplingTransplantError::TooMature { root }) if root == from,
    ));
    assert_eq!(state, before);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn trusted_load_rejects_missing_forged_or_future_transplant_receipts() {
    let (registries, mut state, from, to) = sapling_fixture();
    let _ = validate_transplant_juvenile(&registries, &state, from, to)
        .unwrap()
        .commit(&registries, &mut state)
        .unwrap();
    let baseline = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    assert_eq!(
        baseline["state"]["systems"]["world"]["transplants"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    for mutation in ["missing", "teleport", "wrong_source", "future"] {
        let mut value = baseline.clone();
        let history = &mut value["state"]["systems"]["world"]["transplants"];
        match mutation {
            "missing" => history.as_array_mut().unwrap().clear(),
            "teleport" => history[0]["to"]["x"] = serde_json::json!(to.x() - 5),
            "wrong_source" => history[0]["from"]["x"] = serde_json::json!(from.x() + 1),
            "future" => history[0]["transplanted_at"] = serde_json::json!(1u64),
            _ => unreachable!(),
        }
        let decoded = serde_json::from_value::<LoadedSaveEnvelope>(value).unwrap();
        let loaded = decoded.into_state(&registries);
        assert!(
            matches!(
                loaded,
                Err(crate::persistence::LoadError::InvalidState(
                    StateValidationError::World(
                        WorldValidationError::InvalidStandingTree { .. }
                            | WorldValidationError::InvalidTreeTransplant { .. }
                    )
                ))
            ),
            "{mutation}: {loaded:?}"
        );
    }
}

#[test]
fn transplanted_felling_receipt_cannot_claim_an_unrecorded_destination() {
    let (registries, mut state, from, to) = sapling_fixture();
    let _ = validate_transplant_juvenile(&registries, &state, from, to)
        .unwrap()
        .commit(&registries, &mut state)
        .unwrap();
    let _ = validate_fell_tree(&registries, &state, to)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let mut forged = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    forged["state"]["systems"]["world"]["felled_trees"][0]["root"]["x"] =
        serde_json::json!(from.x());
    let loaded = serde_json::from_value::<LoadedSaveEnvelope>(forged)
        .unwrap()
        .into_state(&registries);
    assert!(matches!(
        loaded,
        Err(crate::persistence::LoadError::InvalidState(
            StateValidationError::World(WorldValidationError::InvalidFelledTree { .. })
        ))
    ));
}

#[test]
fn different_seeded_trees_can_be_felled_at_the_same_replanted_site() {
    use crate::logistics::relocate_player_for_test;
    use crate::simulation::advance_control_tick;

    // Two juvenile seed identities share one supported straight planting path.
    // The second is physically transplanted into the first tree's harvested root.
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(48_219);
    let first = VoxelCoord::new(11, 3, 7);
    let second = VoxelCoord::new(19, 3, 7);
    let mut state = AppState::new();
    validate_admit_player(&registries, &state, VoxelCoord::new(12, 3, 7))
        .unwrap()
        .commit(&mut state)
        .unwrap();
    for chunk in [ChunkCoord::new(0, 0), ChunkCoord::new(1, 0)] {
        validate_generate_chunk(&registries, &state, seed, chunk, BLOCK_STATE_STONE)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    }
    assert_eq!(
        state.world().tree_at(first).unwrap().initial_wood(),
        Mass::from_milligrams(8_000_000)
    );
    assert_eq!(
        state.world().tree_at(second).unwrap().initial_wood(),
        Mass::from_milligrams(8_000_000)
    );
    let original_matter = calculate_matter_accounting(&state).unwrap().total();

    let first_felling = validate_fell_tree(&registries, &state, first)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    // Recover the actual parcel, freeing the original planting position.
    relocate_player_for_test(&mut state, first);
    let picked = advance_control_tick(&registries, &mut state)
        .unwrap()
        .surface_drop_pickup()
        .unwrap();
    assert_eq!(picked.resource(), first_felling.drop());
    assert_eq!(picked.picked_mass(), first_felling.wood());

    for from_x in [19, 17, 15, 13] {
        let from = VoxelCoord::new(from_x, 3, 7);
        let to = VoxelCoord::new(from_x - 2, 3, 7);
        relocate_player_for_test(&mut state, VoxelCoord::new(from_x - 1, 3, 7));
        let moved = validate_transplant_juvenile(&registries, &state, from, to)
            .unwrap()
            .commit(&registries, &mut state)
            .unwrap();
        assert_eq!((moved.from(), moved.to()), (from, to));
    }
    assert_eq!(state.world().tree_at(first).unwrap().origin_root(), second);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        original_matter
    );

    // A save must not move the first harvest into a future tick after the second
    // tree already occupied that same root. Check both the still-living successor
    // and the case where both trees have since been felled.
    let reject_forged_overlap = |game: &AppState| {
        let mut later = game.clone();
        let _ = crate::simulation::advance_tick(&registries, &mut later).unwrap();
        assert_eq!(later.tick(), crate::core::time::SimulationTick::new(1));
        let mut value = serde_json::to_value(SaveEnvelope::new(&registries, &later)).unwrap();
        value["state"]["systems"]["world"]["felled_trees"][0]["felled_at"] =
            serde_json::json!(1u64);
        let loaded = serde_json::from_value::<LoadedSaveEnvelope>(value)
            .unwrap()
            .into_state(&registries);
        assert!(
            matches!(
                loaded,
                Err(crate::persistence::LoadError::InvalidState(
                    StateValidationError::World(WorldValidationError::InvalidFelledTree { root })
                )) if root == first
            ),
            "overlapping felling history must reject with a felling error: {loaded:?}"
        );
    };
    reject_forged_overlap(&state);

    // The save must retain both the old felling and the new live biological identity.
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(state, replay);
    for game in [&mut state, &mut replay] {
        let second_felling = validate_fell_tree(&registries, game, first)
            .unwrap()
            .commit(game)
            .unwrap();
        assert_eq!(second_felling.root(), first);
        assert_eq!(second_felling.wood(), Mass::from_milligrams(8_000_000));
        assert_eq!(
            game.world()
                .felled_trees()
                .map(|record| (record.origin_root(), record.root()))
                .collect::<Vec<_>>(),
            vec![(first, first), (second, first)]
        );
        assert_eq!(validate_loaded_state(&registries, game), Ok(()));
        assert_eq!(
            calculate_matter_accounting(game).unwrap().total(),
            original_matter
        );
    }
    assert_eq!(state, replay);
    reject_forged_overlap(&state);
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    assert_eq!(
        serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
            .unwrap()
            .into_state(&registries)
            .unwrap(),
        state
    );
}

#[test]
fn trusted_load_rejects_intermediate_transplant_overlap_after_both_trees_move_away() {
    use crate::core::state::apply_clock_advance;
    use crate::logistics::relocate_player_for_test;

    // Both generated juveniles have a supported path to one shared planting
    // site. The real sequence vacates it before the other tree arrives.
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(48_219);
    let first = VoxelCoord::new(11, 3, 7);
    let second = VoxelCoord::new(19, 3, 7);
    let mut state = AppState::new();
    validate_admit_player(&registries, &state, VoxelCoord::new(12, 3, 7))
        .unwrap()
        .commit(&mut state)
        .unwrap();
    for chunk in [ChunkCoord::new(0, 0), ChunkCoord::new(1, 0)] {
        validate_generate_chunk(&registries, &state, seed, chunk, BLOCK_STATE_STONE)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    }
    let original_matter = calculate_matter_accounting(&state).unwrap().total();
    for (from_x, to_x, tick) in [
        (11, 13, 0),
        (13, 15, 0),
        (15, 13, 2),
        (19, 17, 2),
        (17, 15, 3),
    ] {
        if state.tick().value() != tick {
            apply_clock_advance(&mut state, crate::core::time::SimulationTick::new(tick));
        }
        relocate_player_for_test(&mut state, VoxelCoord::new((from_x + to_x) / 2, 3, 7));
        let from = VoxelCoord::new(from_x, 3, 7);
        let to = VoxelCoord::new(to_x, 3, 7);
        let _ = validate_transplant_juvenile(&registries, &state, from, to)
            .unwrap()
            .commit(&registries, &mut state)
            .unwrap();
    }
    assert_eq!(
        state
            .world()
            .tree_at(VoxelCoord::new(13, 3, 7))
            .unwrap()
            .origin_root(),
        first
    );
    assert_eq!(
        state
            .world()
            .tree_at(VoxelCoord::new(15, 3, 7))
            .unwrap()
            .origin_root(),
        second
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        original_matter
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let original = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    assert_eq!(
        serde_json::from_value::<LoadedSaveEnvelope>(original.clone())
            .unwrap()
            .into_state(&registries)
            .unwrap(),
        state
    );

    // Move the second tree's arrival to tick 1, while the first still owns
    // the shared site until tick 2. Both independent paths remain correctly
    // ordered, both current roots are distinct, and all biomass is unchanged.
    let mut forged = original;
    let history = forged["state"]["systems"]["world"]["transplants"]
        .as_array_mut()
        .unwrap();
    assert_eq!(history.len(), 5);
    history[3]["transplanted_at"] = serde_json::json!(1);
    history[4]["transplanted_at"] = serde_json::json!(1);
    let first_departure = history.remove(2);
    history.push(first_departure);
    let loaded = serde_json::from_value::<LoadedSaveEnvelope>(forged)
        .unwrap()
        .into_state(&registries);
    assert!(
        matches!(
            loaded,
            Err(crate::persistence::LoadError::InvalidState(
                StateValidationError::World(WorldValidationError::InvalidTreeTransplant { .. })
            ))
        ),
        "intermediate tree occupancy must be replayed, not inferred from final roots: {loaded:?}"
    );
}

#[test]
fn transplant_admission_becomes_stale_after_world_time_advances() {
    let (registries, mut state, from, to) = sapling_fixture();
    let transplant = validate_transplant_juvenile(&registries, &state, from, to).unwrap();
    for _ in 0..crate::core::time::CONTROL_TICKS_PER_SIMULATION_TICK {
        let _ = crate::simulation::advance_control_tick(&registries, &mut state).unwrap();
    }
    assert!(state.tick().value() > 0);
    let before = state.clone();
    assert_eq!(
        transplant.commit(&registries, &mut state),
        Err(SaplingTransplantCommitError::TickChanged),
    );
    assert_eq!(state, before);
    assert_eq!(state.world().tree_at(from).unwrap().origin_root(), from);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
