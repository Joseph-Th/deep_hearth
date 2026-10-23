//! Grade-bearing world ore enters ordinary excavation custody without losing gangue.

use super::*;
use crate::content::{
    BLOCK_STATE_GRADED_CASSITERITE_ORE, BLOCK_STATE_GRADED_GALENA_ORE, FORM_CRUSHED, FORM_ORE,
    MATERIAL_CASSITERITE, MATERIAL_GALENA, PROCESS_CONCENTRATE_CASSITERITE, PROCESS_HAND_BREAK_ORE,
    PROCESS_HAND_PICK_CASSITERITE, PROCESS_HAND_PICK_GALENA,
};
use crate::core::state::validate_loaded_state;
use crate::inventory::{MaterialLotSelection, add_solid_stockpile_for_test};
use crate::logistics::validate_place_ground_stockpile;
use crate::ore_processing::{
    ManualComminutionRequest, ManualConstituentSeparationRequest,
    resolve_manual_comminution_process, resolve_manual_constituent_separation_process,
    validate_start_manual_comminution, validate_start_manual_constituent_separation,
};
use crate::simulation::advance_tick;

#[test]
fn generated_grade_bearing_galena_mines_into_pure_hand_picked_furnace_charge() {
    let (registries, mut state, pick, target) = generated_excavation_fixture();
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        target,
        BLOCK_STATE_GRADED_GALENA_ORE,
    );
    let initial_matter = calculate_matter_accounting(&state).unwrap().total();
    let ore_grade = registries
        .world()
        .block_for_state(BLOCK_STATE_GRADED_GALENA_ORE)
        .unwrap()
        .excavation()
        .unwrap()
        .composition();
    assert_eq!(ore_grade.parts_per_million(MATERIAL_GALENA), 800_000);
    assert_eq!(ore_grade.parts_per_million(MATERIAL_STONE), 200_000);

    let batch = Mass::from_milligrams(100_000);
    let mining = validate_start_player_block_excavation(
        &registries,
        &state,
        BlockExcavationRequest::new(positive_x_view(), MINING_METHOD_HAND_PICK, pick, batch),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let mut mined = None;
    while state.control_tick() < mining.completes_at() {
        let tick = advance_control_tick(&registries, &mut state).unwrap();
        if let Some(outcome) = tick.block_excavation() {
            mined = Some(outcome);
        }
    }
    let BlockExcavationOutputReceipt::Carried { lot: ore_lot } =
        mined.expect("ore extraction did not complete").output()
    else {
        panic!("small galena extraction did not reach carried inventory");
    };
    assert_eq!(
        state.inventory().get_lot(ore_lot).unwrap().composition(),
        &ore_grade
    );

    let position = state.logistics().player().unwrap().position();
    let bins = (0..3)
        .map(|_| {
            let id = add_solid_stockpile_for_test(&mut state, batch).unwrap();
            validate_place_ground_stockpile(&state, id, position)
                .unwrap()
                .commit(&mut state)
                .unwrap();
            id
        })
        .collect::<Vec<_>>();
    let carried = state.logistics().player().unwrap().carried_stockpile();
    let break_selection = [MaterialLotSelection::new(ore_lot, batch)];
    let broken = resolve_manual_comminution_process(
        &registries,
        &state,
        ManualComminutionRequest::new(PROCESS_HAND_BREAK_ORE, carried, &break_selection),
    )
    .unwrap();
    validate_start_manual_comminution(&registries, &state, &broken, carried, bins[0])
        .unwrap()
        .commit(&mut state)
        .unwrap();
    for _ in 0..broken.duration().value() {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    let crushed_id = state.inventory().lot_ids(bins[0]).next().unwrap();
    assert_eq!(
        state.inventory().get_lot(crushed_id).unwrap().composition(),
        &ore_grade
    );
    let selection = [MaterialLotSelection::new(crushed_id, batch)];
    let picked = resolve_manual_constituent_separation_process(
        &registries,
        &state,
        ManualConstituentSeparationRequest::new(PROCESS_HAND_PICK_GALENA, bins[0], &selection),
    )
    .unwrap();
    assert_eq!(picked.target_mass().milligrams(), 52_000);
    assert_eq!(picked.residue_mass().milligrams(), 48_000);
    validate_start_manual_constituent_separation(
        &registries,
        &state,
        &picked,
        bins[0],
        bins[1],
        bins[2],
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    for _ in 0..picked.duration().value() {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }

    let refined_id = state.inventory().lot_ids(bins[1]).next().unwrap();
    let refined = state.inventory().get_lot(refined_id).unwrap();
    assert_eq!(
        refined.commodity(),
        CommodityKey::new(MATERIAL_GALENA, FORM_CRUSHED)
    );
    assert_eq!(refined.composition().pure_material(), Some(MATERIAL_GALENA));
    assert_eq!(refined.mass().milligrams(), 52_000);
    assert!(
        refined.mass().milligrams() >= 47_852,
        "one fixed-point lead furnace charge should fit"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(bins[2])
            .unwrap()
            .stored_mass()
            .milligrams(),
        48_000
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        initial_matter
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn digging_a_graded_world_cell_preserves_ore_assay_through_saved_player_work() {
    let (registries, mut state, pick, target) = generated_excavation_fixture();
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        target,
        BLOCK_STATE_GRADED_CASSITERITE_ORE,
    );
    let block = registries
        .world()
        .block_for_state(BLOCK_STATE_GRADED_CASSITERITE_ORE)
        .unwrap();
    let grade = block.excavation().unwrap().composition();
    assert_eq!(grade.parts_per_million(MATERIAL_CASSITERITE), 400_000);
    assert_eq!(grade.parts_per_million(MATERIAL_STONE), 600_000);
    assert!(
        registries
            .ore_processing()
            .get_constituent_separation(PROCESS_CONCENTRATE_CASSITERITE)
            .is_some()
    );
    let batch = Mass::from_milligrams(200_000);
    let source_before = state
        .world()
        .represented_mass_at(registries.world(), target)
        .unwrap();
    let accounted_before = calculate_matter_accounting(&state).unwrap().total();
    let start = validate_start_player_block_excavation(
        &registries,
        &state,
        BlockExcavationRequest::new(positive_x_view(), MINING_METHOD_HAND_PICK, pick, batch),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let save = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut continued = serde_json::from_slice::<LoadedSaveEnvelope>(&save)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let mut completed = None;
    while state.control_tick() < start.completes_at() {
        let first = advance_control_tick(&registries, &mut state).unwrap();
        let second = advance_control_tick(&registries, &mut continued).unwrap();
        if let Some(outcome) = first.block_excavation() {
            completed = Some(outcome);
        }
        assert_eq!(
            state, continued,
            "excavation continuation diverged after save/load"
        );
        assert_eq!(first, second);
    }
    let outcome = completed.expect("graded excavation did not complete");
    let BlockExcavationOutputReceipt::Carried { lot } = outcome.output() else {
        panic!("small mixed-ore batch unexpectedly bypassed player inventory");
    };
    let actual = state
        .inventory()
        .get_lot(lot)
        .expect("excavated lot disappeared");
    assert_eq!(
        actual.commodity(),
        CommodityKey::new(MATERIAL_CASSITERITE, FORM_ORE)
    );
    assert_eq!(actual.composition(), &grade);
    assert_eq!(actual.mass(), batch);
    assert_eq!(
        state
            .world()
            .represented_mass_at(registries.world(), target),
        source_before.checked_sub(batch),
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        accounted_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    // The mined lot is eligible for the actual pre-machine hand-breaking step, with no
    // fixture-only installation or fabricated mineral assay required.
    let carried = state.logistics().player().unwrap().carried_stockpile();
    let hand_batch = Mass::from_milligrams(100_000);
    let selections = [MaterialLotSelection::new(lot, hand_batch)];
    let crushed = resolve_manual_comminution_process(
        &registries,
        &state,
        ManualComminutionRequest::new(PROCESS_HAND_BREAK_ORE, carried, &selections),
    )
    .unwrap();
    let fragments = &crushed.process_resolution().outputs()[0];
    assert_eq!(
        fragments.commodity(),
        CommodityKey::new(MATERIAL_CASSITERITE, FORM_CRUSHED),
    );
    assert_eq!(fragments.composition(), &grade);
    assert_eq!(fragments.mass(), hand_batch);
    assert!(fragments.particle_size_distribution().is_some());

    // Ground processing has three finite local destinations. Nothing is created as a
    // fabricated mineral feed: it all originated in the excavated mixed-grade world cell.
    let bench_position = state.logistics().player().unwrap().position();
    let mut bins = Vec::new();
    for _ in 0..3 {
        let bin = add_solid_stockpile_for_test(&mut state, hand_batch).unwrap();
        validate_place_ground_stockpile(&state, bin, bench_position)
            .unwrap()
            .commit(&mut state)
            .unwrap();
        bins.push(bin);
    }
    let crushed_source = bins[0];
    let selected_crystals = bins[1];
    let rejected_gangue = bins[2];
    // Output-bin creation advances the inventory revision. Resolve after placement:
    // stale process resolutions must not bypass canonical source/destination admission.
    let crushed = resolve_manual_comminution_process(
        &registries,
        &state,
        ManualComminutionRequest::new(PROCESS_HAND_BREAK_ORE, carried, &selections),
    )
    .unwrap();
    validate_start_manual_comminution(&registries, &state, &crushed, carried, crushed_source)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    for _ in 0..crushed.duration().value() {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(state.player_work().active(), None);
    let crushed_lot = state.inventory().lot_ids(crushed_source).next().unwrap();
    assert_eq!(
        state
            .inventory()
            .get_lot(crushed_lot)
            .unwrap()
            .composition(),
        &grade
    );

    let sorting_selections = [MaterialLotSelection::new(crushed_lot, hand_batch)];
    let picked = resolve_manual_constituent_separation_process(
        &registries,
        &state,
        ManualConstituentSeparationRequest::new(
            PROCESS_HAND_PICK_CASSITERITE,
            crushed_source,
            &sorting_selections,
        ),
    )
    .unwrap();
    assert_eq!(picked.target_mass().milligrams(), 22_000);
    assert_eq!(picked.residue_mass().milligrams(), 78_000);
    let total_before_sorting = calculate_matter_accounting(&state).unwrap().total();
    validate_start_manual_constituent_separation(
        &registries,
        &state,
        &picked,
        crushed_source,
        selected_crystals,
        rejected_gangue,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    for _ in 0..picked.duration().value() {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(selected_crystals)
            .unwrap()
            .stored_mass()
            .milligrams(),
        22_000,
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(rejected_gangue)
            .unwrap()
            .stored_mass()
            .milligrams(),
        78_000,
    );
    let crystals = state
        .inventory()
        .get_lot(state.inventory().lot_ids(selected_crystals).next().unwrap())
        .unwrap();
    assert_eq!(
        crystals.commodity(),
        CommodityKey::new(MATERIAL_CASSITERITE, FORM_CRUSHED)
    );
    assert_eq!(
        crystals
            .composition()
            .parts_per_million(MATERIAL_CASSITERITE),
        1_000_000
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        total_before_sorting
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        accounted_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
