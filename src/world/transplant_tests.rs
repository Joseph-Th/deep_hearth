//! Juvenile relocation uses one persistent biological identity and no new biomass.

use super::*;
use crate::content::{BLOCK_STATE_STONE, build_registries};
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
