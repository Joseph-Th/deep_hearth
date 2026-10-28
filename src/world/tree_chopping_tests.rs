//! Player-control chopping, persisted active work, and timed wood custody.

use super::*;
use crate::content::{
    BLOCK_STATE_STONE, EQUIPMENT_STONE_WOODWORKING_ADZE, FORM_HANDLE, FORM_TOOL, MATERIAL_STONE,
    MATERIAL_WOOD, build_registries,
};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
use crate::equipment::validate_assemble_equipment;
use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};
use crate::logistics::{
    PlayerActiveSelection, validate_pickup_equipment, validate_place_ground_stockpile,
    validate_set_active_selection,
};
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::{
    PlayerDirectControlInput, PrimaryActionOutcome, PrimaryControlIntent, advance_control_tick,
    advance_control_tick_with_direct_input,
};
use crate::spatial::{VIEW_DIRECTION_PARTS_PER_MILLION, ViewDirection, VoxelCoord};

fn equipped_young_tree() -> (crate::registry::Registries, AppState, VoxelCoord) {
    let registries = build_registries();
    let (seed, root) = (1..500)
        .find_map(|value| {
            let seed = WorldGenerationSeed::new(value);
            let profile = WorldGenerationProfile::new(seed, BLOCK_STATE_STONE);
            let record = trees::seeded_tree_identity(profile, ChunkCoord::new(0, 0))?;
            let root = record.root();
            let neighbor = root.x().checked_sub(1)?;
            (record.initial_wood().milligrams() < 10_000_000
                && generation::terrain_height(seed, neighbor, root.z()) == root.y() - 1)
                .then_some((seed, root))
        })
        .expect("juvenile tree with level adjacent ground");
    let mut state = AppState::new();
    let at = VoxelCoord::new(root.x() - 1, root.y(), root.z());
    validate_admit_player(&registries, &state, at)
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
    let components =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000)).unwrap();
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(800_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(200_000),
        ),
    ] {
        deposit_lot_for_test(
            &registries,
            &mut state,
            components,
            commodity,
            mass,
            Temperature::from_millikelvin(293_150),
        )
        .unwrap();
    }
    validate_place_ground_stockpile(&state, components, at)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let adze = validate_assemble_equipment(
        &registries,
        &state,
        EQUIPMENT_STONE_WOODWORKING_ADZE,
        components,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    validate_pickup_equipment(&registries, &state, adze)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::Equipment(adze)),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    (registries, state, root)
}

#[test]
fn control_rate_chopping_persists_and_completes_with_exact_mass() {
    let (registries, mut state, root) = equipped_young_tree();
    let direction = ViewDirection::new(VIEW_DIRECTION_PARTS_PER_MILLION, 0, 0).unwrap();
    let started = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL
            .with_primary(PrimaryControlIntent::break_target(direction)),
    )
    .unwrap();
    let work = match started.primary_action() {
        Some(PrimaryActionOutcome::TreeFellingStarted(work)) => *work,
        other => panic!("direct primary must begin timed chopping: {other:?}"),
    };
    assert_eq!(work.root(), root);
    assert!(matches!(
        crate::equipment::equipment_occupancy(&state, work.equipment()),
        Some(crate::equipment::EquipmentOccupancy::TreeFelling { completes_at })
            if completes_at == work.completes_at()
    ));
    assert!(matches!(
        crate::logistics::validate_drop_equipment(&registries, &state, work.equipment()),
        Err(crate::logistics::PlayerEquipmentCarryError::PlayerBusy)
    ));
    assert!(matches!(
        crate::equipment::resolve_available_equipment_provider(
            &registries,
            &state,
            work.equipment()
        ),
        Err(crate::equipment::EquipmentProviderError::TreeFellingInProgress { .. })
    ));
    assert!(state.world().tree_at(root).is_some());
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let before = calculate_matter_accounting(&state).unwrap().total();
    let mut outcome = None;
    for _ in 0..work.completes_at().value() * 72 {
        let current = advance_control_tick(&registries, &mut state).unwrap();
        let repeated = advance_control_tick(&registries, &mut replay).unwrap();
        assert_eq!(current, repeated);
        if let Some(felled) = current.simulation().and_then(|tick| tick.tree_felling()) {
            outcome = Some(felled);
            break;
        }
    }
    let felled = outcome.expect("scheduled chopping must finish");
    assert_eq!(felled.root(), root);
    assert!(state.world().tree_at(root).is_none());
    assert_eq!(
        state.surface().get(felled.drop()).unwrap().remaining_mass(),
        felled.wood()
    );
    assert!(state.player_work().active().is_none());
    assert!(crate::equipment::equipment_occupancy(&state, work.equipment()).is_none());
    assert_eq!(state, replay);
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), before);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn chopping_requires_a_selected_tool_and_prechecked_admission_cannot_outlive_tool_change() {
    use crate::equipment::degrade_equipment_condition_for_test;
    let (registries, mut state, root) = equipped_young_tree();
    let equipment = match state.logistics().active_selection().unwrap() {
        PlayerActiveSelection::Equipment(id) => id,
        _ => unreachable!(),
    };
    let validation = validate_start_player_tree_felling(&registries, &state, root).unwrap();
    degrade_equipment_condition_for_test(&mut state, equipment, 1_000);
    let before = state.clone();
    assert!(matches!(
        validation.commit(&mut state),
        Err(TreeFellingCommitError::StaleEquipmentRevision { .. })
    ));
    assert_eq!(state, before);
    validate_set_active_selection(&registries, &state, None)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let unchanged = state.clone();
    assert!(matches!(
        validate_start_player_tree_felling(&registries, &state, root),
        Err(TreeFellingStartError::NoActiveChoppingTool)
    ));
    assert_eq!(state, unchanged);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn trusted_load_rejects_forged_chopping_schedule_and_equipment() {
    use crate::core::state::StateValidationError;
    use crate::labor::PlayerWorkValidationError;
    let (registries, mut state, root) = equipped_young_tree();
    let work = validate_start_player_tree_felling(&registries, &state, root)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let baseline = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    for corruption in ["deadline", "tool"] {
        let mut forged = baseline.clone();
        let record =
            &mut forged["state"]["systems"]["player_work"]["active"]["TreeFelling"]["work"];
        if corruption == "deadline" {
            record["completes_at"] = serde_json::json!(work.completes_at().value() + 1);
        } else {
            record["equipment"] = serde_json::json!(u32::MAX);
        }
        let parsed: LoadedSaveEnvelope = serde_json::from_value(forged).unwrap();
        let loaded = parsed.into_state(&registries);
        assert!(
            matches!(
                loaded,
                Err(crate::persistence::LoadError::InvalidState(
                    StateValidationError::PlayerWork(
                        PlayerWorkValidationError::TreeFellingDurationMismatch
                            | PlayerWorkValidationError::TreeFellingEquipmentInvalid
                    )
                ))
            ),
            "{corruption}: {loaded:?}"
        );
    }
}
