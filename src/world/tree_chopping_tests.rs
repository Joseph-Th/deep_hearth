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
fn retargeted_tree_chop_replaces_queued_structural_break_without_later_ghost_action() {
    use crate::content::{FORM_LOG, STRUCTURAL_PROFILE_AXIAL_COMPRESSION};
    use crate::core::quantity::{Area, Length};
    use crate::core::time::CONTROL_TICKS_PER_SIMULATION_TICK;
    use crate::spatial::VoxelBounds;
    use crate::structural::{
        StructuralElementGeometry, add_structural_element, materialize_structural_element_for_test,
    };

    let (registries, mut state, root) = equipped_young_tree();
    let player = state.logistics().player().unwrap().position();
    let sign = if player.z().rem_euclid(CHUNK_SIDE as i64) < CHUNK_SIDE as i64 - 3 {
        1_i64
    } else {
        -1_i64
    };
    let panel_voxel = VoxelCoord::new(player.x(), root.y() + 1, player.z() + 2 * sign);
    for distance in 1..=2 {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(player.x(), root.y() + 1, player.z() + distance * sign),
            BlockStateId::AIR,
        );
    }
    let panel = add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        StructuralElementGeometry::new(
            VoxelBounds::new(
                panel_voxel,
                VoxelCoord::new(
                    panel_voxel.x() + 1,
                    panel_voxel.y() + 1,
                    panel_voxel.z() + 1,
                ),
            )
            .unwrap(),
            Length::from_micrometers(1_000_000),
            Area::from_square_millimeters(1_000_000),
        )
        .unwrap(),
        true,
    )
    .unwrap();
    materialize_structural_element_for_test(&registries, &mut state, panel, FORM_LOG);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let _ = advance_control_tick(&registries, &mut state).unwrap();
    let toward_panel =
        ViewDirection::new(0, 0, sign as i32 * VIEW_DIRECTION_PARTS_PER_MILLION).unwrap();
    let queued = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL
            .with_primary(PrimaryControlIntent::break_target(toward_panel)),
    )
    .unwrap();
    assert_eq!(
        queued.primary_action(),
        Some(&PrimaryActionOutcome::QueuedUntilSimulationBoundary)
    );
    assert_eq!(
        state.logistics().pending_primary_break().unwrap().element(),
        panel
    );

    let toward_tree = ViewDirection::new(VIEW_DIRECTION_PARTS_PER_MILLION, 0, 0).unwrap();
    let mut without_tool = state.clone();
    validate_set_active_selection(&registries, &without_tool, None)
        .unwrap()
        .commit(&mut without_tool)
        .unwrap();
    let rejected = advance_control_tick_with_direct_input(
        &registries,
        &mut without_tool,
        PlayerDirectControlInput::NEUTRAL
            .with_primary(PrimaryControlIntent::break_target(toward_tree)),
    )
    .unwrap();
    assert!(matches!(
        rejected.primary_action(),
        Some(PrimaryActionOutcome::TreeFellingRejected(
            TreeFellingStartError::NoActiveChoppingTool
        ))
    ));
    assert!(without_tool.logistics().pending_primary_break().is_none());
    assert!(without_tool.structures().get_element(panel).is_some());
    assert_eq!(validate_loaded_state(&registries, &without_tool), Ok(()));
    let no_tool_saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &without_tool)).unwrap();
    assert_eq!(
        serde_json::from_slice::<LoadedSaveEnvelope>(&no_tool_saved)
            .unwrap()
            .into_state(&registries)
            .unwrap(),
        without_tool
    );

    let redirected = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL
            .with_primary(PrimaryControlIntent::break_target(toward_tree)),
    )
    .unwrap();
    assert!(matches!(
        redirected.primary_action(),
        Some(PrimaryActionOutcome::TreeFellingStarted(work)) if work.root() == root
    ));
    assert!(
        state.logistics().pending_primary_break().is_none(),
        "a new tree-chopping gesture must cancel the older structural break"
    );
    assert!(state.structures().get_element(panel).is_some());
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    while !state.control_tick().is_simulation_boundary() {
        let next = advance_control_tick(&registries, &mut state).unwrap();
        assert_eq!(
            next,
            advance_control_tick(&registries, &mut replay).unwrap()
        );
        assert_eq!(next.primary_action(), None);
    }
    assert_eq!(
        state.control_tick().value(),
        CONTROL_TICKS_PER_SIMULATION_TICK
    );
    assert_eq!(state, replay);
    assert!(state.structures().get_element(panel).is_some());
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
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
    let condition_before = work.equipment_trace().condition();
    let expected_condition = crate::world::tree_felling_condition_after(
        work.equipment_trace(),
        work.completes_at()
            .checked_duration_since(work.started_at())
            .unwrap(),
    );
    assert!(expected_condition < condition_before);
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
    assert_eq!(
        state
            .equipment()
            .get_equipment(work.equipment())
            .unwrap()
            .condition(),
        expected_condition
    );
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
    for corruption in ["deadline", "tool", "condition"] {
        let mut forged = baseline.clone();
        let record =
            &mut forged["state"]["systems"]["player_work"]["active"]["TreeFelling"]["work"];
        if corruption == "deadline" {
            record["completes_at"] = serde_json::json!(work.completes_at().value() + 1);
        } else if corruption == "tool" {
            record["equipment_trace"]["equipment"] = serde_json::json!(u32::MAX);
        } else {
            record["equipment_trace"]["condition"] = serde_json::json!(999_999);
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
                            | PlayerWorkValidationError::TreeFellingEquipmentConditionMismatch
                    )
                ))
            ),
            "{corruption}: {loaded:?}"
        );
    }
}

#[test]
fn chopping_consumes_no_wood_on_fatal_interruption_but_wears_the_adze() {
    use crate::core::quantity::Energy;
    use crate::core::time::TickSpan;
    use crate::survival::{Vitality, player_record};

    let (registries, mut state, root) = equipped_young_tree();
    let work = validate_start_player_tree_felling(&registries, &state, root)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert!(work.completes_at().value() > 1);
    let original_tree = *state.world().tree_at(root).unwrap();
    let physiology = registries.survival().physiology();
    let original = state.survival().player().copied().unwrap();
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record(
            Energy::ZERO,
            original.hydration(),
            Vitality::from_parts_per_million_unchecked(
                physiology.starvation_vitality_loss_ppm_per_tick(),
            ),
            original.nutrition(),
            original.vitality_recovery_remainder(),
        ),
    );
    let matter_before = calculate_matter_accounting(&state).unwrap().total();
    while state.tick().value() < 1 {
        let outcome = advance_control_tick(&registries, &mut state).unwrap();
        assert!(
            outcome
                .simulation()
                .and_then(|tick| tick.tree_felling())
                .is_none()
        );
    }
    assert!(state.player_work().active().is_none());
    assert_eq!(state.world().tree_at(root), Some(&original_tree));
    assert_eq!(
        state.survival().player().unwrap().vitality(),
        Vitality::ZERO
    );
    assert_eq!(
        state
            .equipment()
            .get_equipment(work.equipment())
            .unwrap()
            .condition(),
        crate::world::tree_felling_condition_after(work.equipment_trace(), TickSpan::new(1)),
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let recovered = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(state, recovered);
}

#[test]
fn depleted_adze_cannot_begin_a_tree_it_would_fail_to_finish() {
    use crate::equipment::degrade_equipment_condition_for_test;
    let (registries, mut state, root) = equipped_young_tree();
    let adze = match state.logistics().active_selection() {
        Some(PlayerActiveSelection::Equipment(equipment)) => equipment,
        _ => unreachable!(),
    };
    degrade_equipment_condition_for_test(&mut state, adze, 999_900);
    let before = state.clone();
    assert!(matches!(
        validate_start_player_tree_felling(&registries, &state, root),
        Err(TreeFellingStartError::ChoppingToolTooWorn)
    ));
    assert_eq!(state, before);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn tree_due_on_fatal_tick_still_settles_logs_and_full_tool_wear() {
    use crate::core::quantity::Energy;
    use crate::survival::{Vitality, player_record};

    let (registries, mut state, root) = equipped_young_tree();
    let work = validate_start_player_tree_felling(&registries, &state, root)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    while state.tick().value() + 1 < work.completes_at().value() {
        let _ = advance_control_tick(&registries, &mut state).unwrap();
    }
    let tree_mass = state
        .world()
        .tree_at(root)
        .unwrap()
        .wood_at(work.completes_at());
    let physiology = registries.survival().physiology();
    let player = state.survival().player().copied().unwrap();
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record(
            Energy::ZERO,
            player.hydration(),
            Vitality::from_parts_per_million_unchecked(
                physiology.starvation_vitality_loss_ppm_per_tick(),
            ),
            player.nutrition(),
            player.vitality_recovery_remainder(),
        ),
    );
    let matter_before = calculate_matter_accounting(&state).unwrap().total();
    let mut felled = None;
    while state.tick() < work.completes_at() {
        let outcome = advance_control_tick(&registries, &mut state).unwrap();
        if let Some(done) = outcome.simulation().and_then(|tick| tick.tree_felling()) {
            felled = Some(done);
        }
    }
    assert_eq!(felled.unwrap().wood(), tree_mass);
    assert!(state.world().tree_at(root).is_none());
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state.survival().player().unwrap().vitality(),
        Vitality::ZERO
    );
    assert_eq!(
        state
            .equipment()
            .get_equipment(work.equipment())
            .unwrap()
            .condition(),
        crate::world::tree_felling_condition_after(
            work.equipment_trace(),
            work.completes_at()
                .checked_duration_since(work.started_at())
                .unwrap(),
        ),
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn copper_adze_chops_faster_and_accumulates_less_wear_than_stone() {
    use crate::content::{
        EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE, EQUIPMENT_STONE_WOODWORKING_ADZE,
    };
    use crate::equipment::{EquipmentId, EquipmentOperationTrace};
    use crate::maintenance::Condition;

    let wood = Mass::from_milligrams(400_000_000);
    let stone_duration =
        crate::world::tree_felling_duration(wood, EQUIPMENT_STONE_WOODWORKING_ADZE);
    let copper_duration =
        crate::world::tree_felling_duration(wood, EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE);
    assert!(copper_duration < stone_duration);
    let stone_after = crate::world::tree_felling_condition_after(
        EquipmentOperationTrace::new(
            EquipmentId::new(1),
            EQUIPMENT_STONE_WOODWORKING_ADZE,
            Condition::PRISTINE,
        ),
        stone_duration,
    );
    let copper_after = crate::world::tree_felling_condition_after(
        EquipmentOperationTrace::new(
            EquipmentId::new(2),
            EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE,
            Condition::PRISTINE,
        ),
        copper_duration,
    );
    assert!(stone_after < copper_after);
    assert!(copper_after < Condition::PRISTINE);
}
