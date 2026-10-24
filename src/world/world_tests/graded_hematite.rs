//! Generated gangue-bearing Fe2O3 passes through actual mining and hand-picking custody.

use super::*;
use crate::content::{
    BLOCK_STATE_GRADED_HEMATITE_ORE, EQUIPMENT_COPPER_REINFORCED_PICK, FORM_CRUSHED,
    MATERIAL_HEMATITE, PROCESS_HAND_BREAK_ORE, PROCESS_HAND_PICK_HEMATITE,
};
use crate::core::state::validate_loaded_state;
use crate::equipment::validate_upgrade_equipment;
use crate::inventory::{MaterialLotSelection, add_solid_stockpile_for_test, deposit_lot_for_test};
use crate::logistics::validate_place_ground_stockpile;
use crate::mining::MiningPhysicsError;
use crate::ore_processing::{
    ManualComminutionRequest, ManualConstituentSeparationRequest,
    resolve_manual_comminution_process, resolve_manual_constituent_separation_process,
    validate_start_manual_comminution, validate_start_manual_constituent_separation,
};
use crate::simulation::advance_tick;

#[test]
fn generated_grade_bearing_hematite_mines_into_pure_hand_picked_iron_oxide() {
    let (registries, mut state, pick, target) = generated_excavation_fixture();
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        target,
        BLOCK_STATE_GRADED_HEMATITE_ORE,
    );
    let ore_grade = registries
        .world()
        .block_for_state(BLOCK_STATE_GRADED_HEMATITE_ORE)
        .unwrap()
        .excavation()
        .unwrap()
        .composition();
    assert_eq!(ore_grade.parts_per_million(MATERIAL_HEMATITE), 650_000);
    assert_eq!(ore_grade.parts_per_million(MATERIAL_STONE), 350_000);

    let batch = Mass::from_milligrams(100_000);
    // Primitive stone is capped at 500 MPa. Hematite is a 550 MPa
    // deposit; the ordinary additive copper pick upgrade is compulsory.
    assert!(matches!(
        validate_start_player_block_excavation(
            &registries,
            &state,
            BlockExcavationRequest::new(positive_x_view(), MINING_METHOD_HAND_PICK, pick, batch),
        ),
        Err(BlockExcavationError::Physics(
            MiningPhysicsError::DepositTooHard { .. }
        )),
    ));
    let carried = state.logistics().player().unwrap().carried_stockpile();
    let upgrade = registries
        .equipment()
        .get_equipment(EQUIPMENT_COPPER_REINFORCED_PICK)
        .unwrap()
        .upgrade_profile()
        .unwrap();
    for input in upgrade.additions().inputs() {
        deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            input.commodity(),
            input.mass(),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap();
    }
    let initial_matter = calculate_matter_accounting(&state).unwrap().total();
    validate_drop_equipment(&registries, &state, pick)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_upgrade_equipment(
        &registries,
        &state,
        pick,
        EQUIPMENT_COPPER_REINFORCED_PICK,
        carried,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    validate_pickup_equipment(&registries, &state, pick)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::equipment(pick)),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        initial_matter
    );
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
        panic!("small hematite extraction did not reach carried inventory");
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
        ManualConstituentSeparationRequest::new(PROCESS_HAND_PICK_HEMATITE, bins[0], &selection),
    )
    .unwrap();
    assert_eq!(picked.target_mass().milligrams(), 39_000);
    assert_eq!(picked.residue_mass().milligrams(), 61_000);
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
        CommodityKey::new(MATERIAL_HEMATITE, FORM_CRUSHED)
    );
    assert_eq!(
        refined.composition().pure_material(),
        Some(MATERIAL_HEMATITE)
    );
    assert_eq!(refined.mass().milligrams(), 39_000);
    assert!(
        refined.mass().milligrams() >= 38_000,
        "one coarse hematite hand-picking batch should yield useful Fe2O3"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(bins[2])
            .unwrap()
            .stored_mass()
            .milligrams(),
        61_000
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        initial_matter
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
