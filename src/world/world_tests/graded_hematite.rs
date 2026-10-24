//! Generated gangue-bearing Fe2O3 passes through actual mining and hand-picking custody.

use super::*;
use crate::content::{
    BLOCK_STATE_GRADED_HEMATITE_ORE, ENERGY_STONE_FLYWHEEL_DRIVE, EQUIPMENT_COPPER_REINFORCED_PICK,
    EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_STONE_ROTARY_QUERN, EQUIPMENT_STONE_SEPARATOR,
    FORM_CONCENTRATE, FORM_CRUSHED, FORM_EXHAUSTED_TAILINGS, MANUAL_POWER_HAND_CRANK,
    MATERIAL_HEMATITE, PROCESS_CLEAN_HEMATITE_CONCENTRATE, PROCESS_HAND_BREAK_ORE,
    PROCESS_HAND_PICK_HEMATITE, PROCESS_REGRIND_HEMATITE_PICKING_TAILINGS,
    PROCESS_SCAVENGE_HEMATITE_TAILINGS,
};
use crate::core::quantity::Energy;
use crate::core::state::{AppState, validate_loaded_state};
use crate::energy::validate_assemble_energy_store;
use crate::equipment::{
    EquipmentDefinitionId, validate_assemble_equipment, validate_upgrade_equipment,
};
use crate::inventory::{MaterialLotSelection, add_solid_stockpile_for_test, deposit_lot_for_test};
use crate::labor::{ManualPowerRequest, validate_start_manual_power};
use crate::logistics::validate_place_ground_stockpile;
use crate::mining::MiningPhysicsError;
use crate::ore_processing::{
    ComminutionRequest, ConstituentSeparationProcessDefinition, ConstituentSeparationRequest,
    ManualComminutionRequest, ManualConstituentSeparationRequest, resolve_comminution_process,
    resolve_constituent_separation_process, resolve_manual_comminution_process,
    resolve_manual_constituent_separation_process, validate_start_manual_comminution,
    validate_start_manual_constituent_separation,
};
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::production::{
    ProcessOutputRoute, validate_start_player_process, validate_start_player_process_routed,
};
use crate::simulation::advance_tick;

fn select_every_lot(
    state: &AppState,
    stockpile: crate::inventory::StockpileId,
) -> Vec<MaterialLotSelection> {
    state
        .inventory()
        .lot_ids(stockpile)
        .map(|id| MaterialLotSelection::new(id, state.inventory().get_lot(id).unwrap().mass()))
        .collect()
}

fn assemble_local_ore_machine(
    registries: &crate::registry::Registries,
    state: &mut AppState,
    carried: crate::inventory::StockpileId,
    definition: EquipmentDefinitionId,
    position: VoxelCoord,
) -> crate::equipment::EquipmentId {
    let profile = registries
        .equipment()
        .get_equipment(definition)
        .unwrap()
        .assembly_profile()
        .unwrap();
    for input in profile.inputs() {
        deposit_lot_for_test(
            registries,
            state,
            carried,
            input.commodity(),
            input.mass(),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap();
    }
    let machine = validate_assemble_equipment(registries, state, definition, carried)
        .unwrap()
        .commit(state)
        .unwrap();
    assert_eq!(
        state.logistics().stationary_equipment_position(machine),
        Some(position)
    );
    machine
}

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

    // Continue with *the same 61 g* of world-excavated coarse tailings.
    // Machines consume authored solid parts, not a fixture-created separator.
    let quern = assemble_local_ore_machine(
        &registries,
        &mut state,
        carried,
        EQUIPMENT_STONE_ROTARY_QUERN,
        position,
    );
    let separator = assemble_local_ore_machine(
        &registries,
        &mut state,
        carried,
        EQUIPMENT_STONE_SEPARATOR,
        position,
    );
    let crank = assemble_local_ore_machine(
        &registries,
        &mut state,
        carried,
        EQUIPMENT_STONE_HAND_CRANK,
        position,
    );
    let flywheel_profile = registries
        .energy()
        .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
        .unwrap()
        .assembly_profile()
        .unwrap();
    for input in flywheel_profile.inputs() {
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
    let drive =
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_FLYWHEEL_DRIVE, carried)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    assert_eq!(
        state.logistics().energy_store_position(drive),
        Some(position)
    );
    let mut processing_bins = Vec::new();
    for _ in 0..4 {
        let bin = add_solid_stockpile_for_test(&mut state, batch).unwrap();
        validate_place_ground_stockpile(&state, bin, position)
            .unwrap()
            .commit(&mut state)
            .unwrap();
        processing_bins.push(bin);
    }
    let [fine_bin, concentrate_bin, exhausted_bin, clean_bin] =
        <[_; 4]>::try_from(processing_bins).expect("four finite processing destinations");
    let matter_before_mechanical = calculate_matter_accounting(&state).unwrap().total();
    let charge = validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(
            MANUAL_POWER_HAND_CRANK,
            crank,
            drive,
            Energy::from_nanojoules(400_000_000_000),
        ),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let charge_duration = charge.completes_at().value() - charge.started_at().value();
    for _ in 0..charge_duration {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    let energy_before_mechanical = state.energy().get_store(drive).unwrap().stored();
    assert!(energy_before_mechanical > Energy::ZERO);

    let ground = resolve_comminution_process(
        &registries,
        &state,
        ComminutionRequest::new(
            PROCESS_REGRIND_HEMATITE_PICKING_TAILINGS,
            bins[2],
            &select_every_lot(&state, bins[2]),
            quern,
            drive,
        ),
    )
    .unwrap();
    let grinding_ticks = ground.process_resolution().duration().value();
    validate_start_player_process(
        &registries,
        &state,
        ground.process_resolution(),
        bins[2],
        fine_bin,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    for _ in 0..grinding_ticks {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(
        state
            .inventory()
            .get_stockpile(fine_bin)
            .unwrap()
            .stored_mass()
            .milligrams(),
        61_000
    );

    let scavenged = resolve_constituent_separation_process(
        &registries,
        &state,
        ConstituentSeparationRequest::new(
            PROCESS_SCAVENGE_HEMATITE_TAILINGS,
            fine_bin,
            &select_every_lot(&state, fine_bin),
            separator,
            drive,
        ),
    )
    .unwrap();
    assert_eq!(scavenged.target_mass().milligrams(), 25_000);
    let scavenging_ticks = scavenged.process_resolution().duration().value();
    validate_start_player_process_routed(
        &registries,
        &state,
        scavenged.process_resolution(),
        fine_bin,
        &[
            ProcessOutputRoute::new(
                ConstituentSeparationProcessDefinition::TARGET_STREAM,
                concentrate_bin,
            ),
            ProcessOutputRoute::new(
                ConstituentSeparationProcessDefinition::RESIDUE_STREAM,
                exhausted_bin,
            ),
        ],
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    for _ in 0..scavenging_ticks {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    let concentrate = state
        .inventory()
        .get_lot(state.inventory().lot_ids(concentrate_bin).next().unwrap())
        .unwrap();
    assert_eq!(
        concentrate.commodity(),
        CommodityKey::new(MATERIAL_HEMATITE, FORM_CONCENTRATE)
    );
    assert!(
        concentrate.composition().parts_per_million(MATERIAL_STONE) > 0,
        "density separation cannot magically clean away all remaining stone",
    );

    let clean = resolve_constituent_separation_process(
        &registries,
        &state,
        ConstituentSeparationRequest::new(
            PROCESS_CLEAN_HEMATITE_CONCENTRATE,
            concentrate_bin,
            &select_every_lot(&state, concentrate_bin),
            separator,
            drive,
        ),
    )
    .unwrap();
    assert_eq!(clean.target_mass().milligrams(), 19_136);
    let cleaning_ticks = clean.process_resolution().duration().value();
    validate_start_player_process_routed(
        &registries,
        &state,
        clean.process_resolution(),
        concentrate_bin,
        &[
            ProcessOutputRoute::new(
                ConstituentSeparationProcessDefinition::TARGET_STREAM,
                clean_bin,
            ),
            ProcessOutputRoute::new(
                ConstituentSeparationProcessDefinition::RESIDUE_STREAM,
                exhausted_bin,
            ),
        ],
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();

    // The job and both reserved output routes must survive trusted-load
    // reconstruction before completing; continuation cannot fabricate solids.
    let serialized = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut resumed = serde_json::from_slice::<LoadedSaveEnvelope>(&serialized)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for _ in 0..cleaning_ticks {
        let _ = advance_tick(&registries, &mut state).unwrap();
        let _ = advance_tick(&registries, &mut resumed).unwrap();
    }
    assert_eq!(resumed, state);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(clean_bin)
            .unwrap()
            .stored_mass()
            .milligrams(),
        19_136
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(exhausted_bin)
            .unwrap()
            .stored_mass()
            .milligrams(),
        41_864
    );
    for id in state.inventory().lot_ids(exhausted_bin) {
        assert_eq!(
            state.inventory().get_lot(id).unwrap().commodity().form(),
            FORM_EXHAUSTED_TAILINGS
        );
    }
    for id in state.inventory().lot_ids(clean_bin) {
        let lot = state.inventory().get_lot(id).unwrap();
        assert_eq!(
            lot.commodity(),
            CommodityKey::new(MATERIAL_HEMATITE, FORM_CRUSHED)
        );
        assert_eq!(lot.composition().pure_material(), Some(MATERIAL_HEMATITE));
    }
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        matter_before_mechanical
    );
    assert!(state.energy().get_store(drive).unwrap().stored() < energy_before_mechanical);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
