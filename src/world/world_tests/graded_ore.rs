//! Grade-bearing world ore enters ordinary excavation custody without losing gangue.

use super::*;
use crate::content::{
    BLOCK_STATE_GRADED_CASSITERITE_ORE, FORM_CRUSHED, FORM_ORE, MATERIAL_CASSITERITE,
    PROCESS_CONCENTRATE_CASSITERITE, PROCESS_HAND_BREAK_ORE,
};
use crate::core::state::validate_loaded_state;
use crate::inventory::MaterialLotSelection;
use crate::ore_processing::{ManualComminutionRequest, resolve_manual_comminution_process};

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
}
