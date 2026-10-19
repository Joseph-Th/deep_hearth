//! Contracts for ordinary recipe/batch crafting from player-carried custody.

use std::num::NonZeroU64;

use super::*;
use crate::content::{
    FORM_BOARD, FORM_BULK_CRATE_BODY, FORM_CHIP, FORM_LOG, FORM_LUMP, FORM_PACKED_CLAY_BINDER,
    FORM_TOOL, MATERIAL_CLAY, MATERIAL_STONE, MATERIAL_WOOD, PROCESS_KNAP_STONE_TOOL,
    PROCESS_SHAPE_WOOD_BOARDS, STORAGE_CLAY_LINED_SPOIL_BAY, build_registries,
};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::AppState;
use crate::inventory::{
    add_solid_stockpile_for_test, deposit_lot_for_test, project_material_stacks,
    validate_build_storage_enclosure,
};
use crate::logistics::validate_place_ground_stockpile;
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::advance_tick;
use crate::spatial::VoxelCoord;

fn one_batch() -> NonZeroU64 {
    NonZeroU64::new(1).unwrap_or_else(|| unreachable!("one batch is nonzero"))
}

#[test]
fn player_woodworking_into_constructed_spoil_bay_requires_output_segregation() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let carried = validate_admit_player(&registries, &state, position)
        .unwrap_or_else(|error| panic!("spoil-site actor admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("spoil-site actor admission commit failed: {error}"))
        .carried_stockpile();
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_BULK_CRATE_BODY),
            Mass::from_milligrams(3_200_000),
        ),
        (
            CommodityKey::new(MATERIAL_CLAY, FORM_PACKED_CLAY_BINDER),
            Mass::from_milligrams(4_000_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(1_000_000),
        ),
    ] {
        deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            commodity,
            mass,
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("spoil-site raw material admission failed: {error}"));
    }
    let site = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(80_000_000))
        .unwrap_or_else(|error| panic!("spoil site target failed: {error}"));
    validate_place_ground_stockpile(&state, site, position)
        .unwrap_or_else(|error| panic!("spoil site positioning failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("spoil site positioning commit failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("spoil site before-matter failed: {error}"))
        .total();
    validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_CLAY_LINED_SPOIL_BAY,
        site,
        carried,
    )
    .unwrap_or_else(|error| panic!("material-backed spoil construction failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("material-backed spoil commit failed: {error}"));

    let craft = plan_manual_craft_from_stockpile(
        &registries,
        &state,
        PROCESS_SHAPE_WOOD_BOARDS,
        carried,
        one_batch(),
    )
    .unwrap_or_else(|error| panic!("spoil woodworking input plan failed: {error}"));
    // The ordinary single-stream route would send useful boards to a refuse-only site.
    let before_rejection = state.clone();
    assert!(
        matches!(
            validate_start_manual_craft(
                &registries,
                &state,
                ManualCraftStartRequest::new(craft, site),
            ),
            Err(StartManualCraftError::Process(_))
        ),
        "a refuse-only site must reject mixed unsegregated woodworking outputs"
    );
    assert_eq!(state, before_rejection);

    let job = validate_start_player_manual_craft_with_residue_destination(
        &registries,
        &state,
        PROCESS_SHAPE_WOOD_BOARDS,
        one_batch(),
        None,
        site,
    )
    .unwrap_or_else(|error| panic!("segregated woodworking rejected: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("segregated woodworking commit failed: {error}"));
    let completes_at = state
        .production()
        .get_job(job)
        .unwrap_or_else(|| panic!("segregated woodworking job disappeared"))
        .completes_at();
    while state.tick() < completes_at {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("spoil woodworking tick failed: {error}"));
    }
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap_or_else(|| panic!("woodworking carrier disappeared"))
            .get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_BOARD)),
        Mass::from_milligrams(800_000)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(site)
            .unwrap_or_else(|| panic!("constructed spoil bay disappeared"))
            .get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_CHIP)),
        Mass::from_milligrams(200_000)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("spoil woodworking matter audit failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}

#[test]
fn carpentry_residue_requires_physical_bin_capacity_and_survives_save_load() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let carried = validate_admit_player(&registries, &state, position)
        .unwrap_or_else(|error| panic!("waste actor admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("waste actor commit failed: {error}"))
        .carried_stockpile();
    let place_bin = |state: &mut AppState| {
        let bin = add_solid_stockpile_for_test(state, Mass::from_milligrams(200_000))
            .unwrap_or_else(|error| panic!("waste stockpile creation failed: {error}"));
        validate_place_ground_stockpile(state, bin, position)
            .unwrap_or_else(|error| panic!("waste stockpile placement failed: {error}"))
            .commit(state)
            .unwrap_or_else(|error| panic!("waste stockpile placement commit failed: {error}"));
        bin
    };
    let first_bin = place_bin(&mut state);
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(2_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("wood feed setup failed: {error}"));
    let original_matter = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("waste initial accounting failed: {error}"))
        .total();
    let start = |state: &AppState, bin| {
        validate_start_player_manual_craft_with_residue_destination(
            &registries,
            state,
            PROCESS_SHAPE_WOOD_BOARDS,
            one_batch(),
            None,
            bin,
        )
    };
    let first = start(&state, first_bin)
        .unwrap_or_else(|error| panic!("first segregated craft rejected: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("first segregated craft commit failed: {error}"));
    let first_job = state
        .production()
        .get_job(first)
        .unwrap_or_else(|| panic!("first segregated job disappeared"));
    assert_eq!(first_job.output_streams().len(), 2);
    assert_eq!(first_job.output_streams()[0].destination(), carried);
    assert_eq!(first_job.output_streams()[1].destination(), first_bin);
    let completes_at = first_job.completes_at();
    let serialized = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("inflight waste save failed: {error}"));
    let envelope: LoadedSaveEnvelope = serde_json::from_slice(&serialized)
        .unwrap_or_else(|error| panic!("inflight waste decode failed: {error}"));
    state = envelope
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("inflight routed job failed trusted load: {error}"));
    while state.tick() < completes_at {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("first segregated work failed: {error}"));
    }
    let boards = CommodityKey::new(MATERIAL_WOOD, crate::content::FORM_BOARD);
    let chips = CommodityKey::new(MATERIAL_WOOD, FORM_CHIP);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap_or_else(|| panic!("carried output lost"))
            .get_mass(boards),
        Mass::from_milligrams(800_000)
    );
    let waste_stacks = project_material_stacks(&registries, &state, first_bin)
        .unwrap_or_else(|error| panic!("waste projection failed: {error}"));
    assert_eq!(waste_stacks.len(), 1);
    assert_eq!(waste_stacks[0].commodity(), chips);
    assert!(waste_stacks[0].is_residual(&registries));
    assert_eq!(waste_stacks[0].mass(), Mass::from_milligrams(200_000));
    let before_rejection = state.clone();
    assert!(
        matches!(
            start(&state, first_bin),
            Err(PlayerManualCraftError::Start(
                StartManualCraftError::Process(crate::production::StartProcessError::CapacityExceeded {
                    stockpile, requested_inbound, ..
                })
            )) if stockpile == first_bin && requested_inbound == Mass::from_milligrams(200_000)
        ),
        "full waste capacity must block otherwise feasible work"
    );
    assert_eq!(state, before_rejection);
    let second_bin = place_bin(&mut state);
    let second = start(&state, second_bin)
        .unwrap_or_else(|error| panic!("waste re-routing rejected: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("second segregated craft commit failed: {error}"));
    let completes_at = state
        .production()
        .get_job(second)
        .unwrap_or_else(|| panic!("second segregated job lost"))
        .completes_at();
    while state.tick() < completes_at {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("second segregated work failed: {error}"));
    }
    assert_eq!(
        state
            .inventory()
            .get_stockpile(second_bin)
            .unwrap_or_else(|| panic!("second waste bin vanished"))
            .get_mass(chips),
        Mass::from_milligrams(200_000)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("final waste accounting failed: {error}"))
            .total(),
        original_matter
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}

#[test]
fn personal_recipe_catalog_reads_only_carried_material_availability() {
    let registries = build_registries();
    let mut state = AppState::new();
    let carried = validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap_or_else(|error| panic!("personal recipe player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("personal recipe player commit failed: {error}"))
        .carried_stockpile();
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(2_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("personal recipe carried stone failed: {error}"));

    let options = player_manual_craft_options(&registries, &state)
        .unwrap_or_else(|error| panic!("personal recipe catalog failed: {error}"));
    let stone = options
        .iter()
        .copied()
        .find(|option| option.process() == PROCESS_KNAP_STONE_TOOL)
        .unwrap_or_else(|| panic!("personal stone recipe disappeared"));
    assert!(matches!(
        stone.input_mode(),
        crate::crafting::ManualCraftInputMode::Automatic(availability)
            if availability.maximum_batches() == 2
    ));
    let boards = options
        .iter()
        .copied()
        .find(|option| option.process() == PROCESS_SHAPE_WOOD_BOARDS)
        .unwrap_or_else(|| panic!("personal board recipe disappeared"));
    assert!(matches!(
        boards.input_mode(),
        crate::crafting::ManualCraftInputMode::Automatic(availability)
            if availability.maximum_batches() == 0
    ));
}

#[test]
fn personal_recipe_batch_crafts_in_place_without_player_lot_identity() {
    let registries = build_registries();
    let mut state = AppState::new();
    let carried = validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap_or_else(|error| panic!("personal craft player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("personal craft player commit failed: {error}"))
        .carried_stockpile();
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("personal craft carried stone failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("personal craft initial matter audit failed: {error}"))
        .total();

    let option = player_manual_craft_options(&registries, &state)
        .unwrap_or_else(|error| panic!("personal craft option projection failed: {error}"))
        .into_iter()
        .find(|option| option.process() == PROCESS_KNAP_STONE_TOOL)
        .unwrap_or_else(|| panic!("personal craft stone option disappeared"));
    let job =
        validate_start_player_manual_craft_option(&registries, &state, option, one_batch(), None)
            .unwrap_or_else(|error| panic!("personal recipe/batch start failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("personal recipe/batch commit failed: {error}"));
    let completes_at = state
        .production()
        .get_job(job)
        .unwrap_or_else(|| panic!("personal craft job disappeared"))
        .completes_at();
    while state.tick() < completes_at {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("personal craft tick failed: {error}"));
    }

    let record = state
        .inventory()
        .get_stockpile(carried)
        .unwrap_or_else(|| panic!("personal craft carried stockpile disappeared"));
    assert_eq!(
        record.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_TOOL)),
        Mass::from_milligrams(800_000)
    );
    assert_eq!(
        record.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_CHIP)),
        Mass::from_milligrams(200_000)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("personal craft final matter audit failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(
        validate_start_player_manual_craft_option(&registries, &state, option, one_batch(), None,)
            .err(),
        Some(PlayerManualCraftError::InputPlan(
            ManualCraftInputPlanError::InsufficientInput {
                input: CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
                available: Mass::ZERO,
                required: Mass::from_milligrams(1_000_000),
            }
        )),
        "a previously visible recipe row must re-read current carried input instead of authorizing stale work"
    );
}

#[test]
fn explicit_personal_stack_choice_is_revision_bound_and_must_match_recipe_input() {
    let registries = build_registries();
    let mut state = AppState::new();
    let carried = validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap_or_else(|error| panic!("explicit personal craft admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("explicit personal craft commit failed: {error}"))
        .carried_stockpile();
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("explicit personal stone failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("explicit personal wood failed: {error}"));
    let stacks = project_material_stacks(&registries, &state, carried)
        .unwrap_or_else(|error| panic!("explicit personal stack projection failed: {error}"));
    let stone = stacks
        .iter()
        .find(|stack| stack.commodity() == CommodityKey::new(MATERIAL_STONE, FORM_LUMP))
        .unwrap_or_else(|| panic!("explicit personal stone stack disappeared"));
    let wood = stacks
        .iter()
        .find(|stack| stack.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_LOG))
        .unwrap_or_else(|| panic!("explicit personal wood stack disappeared"));

    assert_eq!(
        validate_start_player_manual_craft_from_stack(
            &registries,
            &state,
            PROCESS_KNAP_STONE_TOOL,
            one_batch(),
            wood,
            None,
        )
        .err(),
        Some(PlayerManualCraftError::StackCommodityMismatch {
            expected: CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            actual: CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        })
    );

    let stale_stone = stone.clone();
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(100_000),
        Temperature::from_millikelvin(303_150),
    )
    .unwrap_or_else(|error| panic!("explicit personal stale mutation failed: {error}"));
    assert!(matches!(
        validate_start_player_manual_craft_from_stack(
            &registries,
            &state,
            PROCESS_KNAP_STONE_TOOL,
            one_batch(),
            &stale_stone,
            None,
        ),
        Err(PlayerManualCraftError::Stack(
            MaterialStackProjectionError::StaleInventoryRevision { .. }
        ))
    ));
}
