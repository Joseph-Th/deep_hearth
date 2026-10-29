//! A witnessed workshop makes loose industrial stockpiles worth raiding,
//! without allowing hidden caches, protected stores or instant den transfers.

use super::super::*;
use crate::content::{
    ENERGY_ELECTRICAL_BUFFER, FORM_REINFORCEMENT, FORM_ROUGH_BOX_BODY, MATERIAL_COPPER,
    MATERIAL_WOOD, STORAGE_ROUGH_TIMBER_FIELD_BOX, build_registries,
};
use crate::core::quantity::{Energy, Temperature};
use crate::core::state::validate_loaded_state;
use crate::energy::add_energy_store_with_initial_for_fixture;
use crate::inventory::{MaterialLotId, deposit_lot_for_test, validate_build_storage_enclosure};
use crate::logistics::{
    add_traversable_edge_for_test, place_energy_store_for_fixture,
    validate_allocate_ground_stockpile,
};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::{advance_control_tick, advance_tick};
use crate::surface::{
    ScavengedGroundStockpileRequest, scavenge_unenclosed_ground_stockpile_material,
};

const DEN: VoxelCoord = VoxelCoord::new(-1, 0, 0);
const JUNCTION: VoxelCoord = VoxelCoord::new(0, 0, 0);
const TIP: VoxelCoord = VoxelCoord::new(0, 0, 1);
const SIDE: VoxelCoord = VoxelCoord::new(1, 0, 0);
const FAR: VoxelCoord = VoxelCoord::new(2, 0, 0);
const STORED: Mass = Mass::from_milligrams(600_000);
const STOLEN: Mass = Mass::from_milligrams(500_000);
const COPPER: CommodityKey = CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT);

fn scenario(
    witnessed_machine: bool,
) -> (
    Registries,
    AppState,
    crate::inventory::StockpileId,
    MaterialLotId,
) {
    scenario_at(witnessed_machine, JUNCTION)
}

fn scenario_at(
    witnessed_machine: bool,
    storage_position: VoxelCoord,
) -> (
    Registries,
    AppState,
    crate::inventory::StockpileId,
    MaterialLotId,
) {
    let registries = build_registries();
    let mut state = AppState::new();
    for (from, to) in [
        (DEN, JUNCTION),
        (JUNCTION, TIP),
        (JUNCTION, SIDE),
        (SIDE, FAR),
    ] {
        add_traversable_edge_for_test(&mut state, from, to);
    }
    state
        .kobold_state_mut()
        .apply_opening_colony([DEN, JUNCTION, TIP]);
    let stockpile = validate_allocate_ground_stockpile(
        &state,
        storage_position,
        registries
            .storage()
            .get(STORAGE_ROUGH_TIMBER_FIELD_BOX)
            .unwrap()
            .maximum_stockpile_capacity(),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        COPPER,
        STORED,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    if witnessed_machine {
        let store = add_energy_store_with_initial_for_fixture(
            &registries,
            &mut state,
            ENERGY_ELECTRICAL_BUFFER,
            Energy::ZERO,
        )
        .unwrap();
        place_energy_store_for_fixture(&mut state, store, JUNCTION);
    }
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    (registries, state, stockpile, lot)
}

fn advance_to(registries: &Registries, state: &mut AppState, until: u64) {
    while state.tick().value() < until {
        if state.logistics().player().is_some() {
            let _ = advance_control_tick(registries, state).unwrap();
        } else {
            let _ = advance_tick(registries, state).unwrap();
        }
    }
}

#[test]
fn informed_scout_extracts_one_real_stockpile_lot_then_hauls_it_on_a_later_step() {
    let (registries, mut state, stockpile, _) = scenario(true);
    let initial = calculate_matter_accounting(&state).unwrap().total();
    advance_to(&registries, &mut state, 195);
    assert_eq!(
        state.kobold_state().colony.as_ref().unwrap().mobilized_at,
        Some(SimulationTick::new(195)),
        "only the scout's actual home report may authorize industrial raids"
    );
    advance_to(&registries, &mut state, 200);
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        JUNCTION
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(stockpile)
            .unwrap()
            .get_mass(COPPER),
        STORED
    );
    advance_to(&registries, &mut state, 205);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(stockpile)
            .unwrap()
            .get_mass(COPPER),
        Mass::from_milligrams(100_000)
    );
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        JUNCTION,
        "extracting stock requires one actual work step, not simultaneous movement"
    );
    let parcel = state
        .surface()
        .available_resources_at(Some(JUNCTION))
        .find(|resource| resource.commodity() == COPPER)
        .expect("stockpile egress must create one recoverable world parcel");
    assert_eq!(parcel.remaining_mass(), STOLEN);
    assert_eq!(parcel.created_at(), SimulationTick::new(205));
    let parcel_id = parcel.id();
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        initial
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let save = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&save)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    advance_to(&registries, &mut state, 210);
    advance_to(&registries, &mut replay, 210);
    assert_eq!(
        state, replay,
        "saved theft must replay its actual return trip"
    );
    assert_eq!(state.surface().get(parcel_id).unwrap().position(), DEN);
    assert_eq!(
        state.surface().get(parcel_id).unwrap().remaining_mass(),
        STOLEN
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        initial
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn scout_follows_a_visible_side_route_to_an_exposed_outpost_store() {
    let (registries, mut state, stockpile, _) = scenario_at(true, SIDE);
    let initial = calculate_matter_accounting(&state).unwrap().total();
    advance_to(&registries, &mut state, 200);
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        JUNCTION
    );
    advance_to(&registries, &mut state, 205);
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        SIDE,
        "the scout must actually walk the visible, reachable side edge"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(stockpile)
            .unwrap()
            .get_mass(COPPER),
        STORED,
        "arriving at the outpost must not instantly extract its goods"
    );
    advance_to(&registries, &mut state, 210);
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        SIDE,
        "one further work slot extracts the lot at its real position"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(stockpile)
            .unwrap()
            .get_mass(COPPER),
        Mass::from_milligrams(100_000)
    );
    let parcel = state
        .surface()
        .available_resources_at(Some(SIDE))
        .find(|resource| resource.commodity() == COPPER)
        .expect("outpost theft must leave a recoverable parcel");
    assert_eq!(parcel.created_at(), SimulationTick::new(210));
    let id = parcel.id();
    advance_to(&registries, &mut state, 215);
    assert_eq!(state.surface().get(id).unwrap().position(), JUNCTION);
    advance_to(&registries, &mut state, 220);
    assert_eq!(state.surface().get(id).unwrap().position(), DEN);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        initial
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn informed_scout_needs_three_real_steps_to_reach_and_extract_from_a_far_worksite() {
    let (registries, mut state, stockpile, _) = scenario_at(true, FAR);
    let initial = calculate_matter_accounting(&state).unwrap().total();
    advance_to(&registries, &mut state, 200);
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        JUNCTION
    );
    advance_to(&registries, &mut state, 205);
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        SIDE
    );
    advance_to(&registries, &mut state, 210);
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        FAR
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(stockpile)
            .unwrap()
            .get_mass(COPPER),
        STORED,
        "a two-edge approach does not permit remote extraction"
    );
    advance_to(&registries, &mut state, 215);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(stockpile)
            .unwrap()
            .get_mass(COPPER),
        Mass::from_milligrams(100_000)
    );
    let parcel = state
        .surface()
        .available_resources_at(Some(FAR))
        .find(|resource| resource.commodity() == COPPER)
        .expect("physical arrival permits a recoverable, conserved extraction");
    let id = parcel.id();
    assert_eq!(parcel.created_at(), SimulationTick::new(215));
    let save = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&save)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for (at, position) in [(220, SIDE), (225, JUNCTION), (230, DEN)] {
        advance_to(&registries, &mut state, at);
        advance_to(&registries, &mut replay, at);
        assert_eq!(
            state, replay,
            "same physically routed cargo after reloading"
        );
        assert_eq!(
            state.surface().get(id).unwrap().position(),
            position,
            "the finite parcel must traverse every real return edge"
        );
    }
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        initial
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn player_recovers_the_extracted_lot_before_it_can_be_hauled_away() {
    let (registries, mut state, stockpile, _) = scenario_at(true, FAR);
    let initial = calculate_matter_accounting(&state).unwrap().total();
    advance_to(&registries, &mut state, 215);
    let resource = state
        .surface()
        .available_resources_at(Some(FAR))
        .find(|record| record.commodity() == COPPER)
        .expect("physical raid must expose an ordinary recoverable parcel")
        .id();
    let carried = validate_admit_player(&registries, &state, FAR)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    let pickup = advance_control_tick(&registries, &mut state)
        .unwrap()
        .surface_drop_pickup()
        .expect("ordinary control-rate pickup must recover stolen matter");
    assert_eq!(pickup.resource(), resource);
    assert_eq!(pickup.picked_mass(), STOLEN);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(COPPER),
        STOLEN
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(stockpile)
            .unwrap()
            .get_mass(COPPER),
        Mass::from_milligrams(100_000)
    );
    advance_to(&registries, &mut state, 230);
    assert_eq!(
        state.surface().get(resource).unwrap().remaining_mass(),
        Mass::ZERO
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(COPPER),
        STOLEN
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        initial
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn enclosing_the_intermediate_route_cell_protects_a_distant_worksite() {
    let (registries, mut state, stockpile, _) = scenario_at(true, FAR);
    let definition = registries
        .storage()
        .get(STORAGE_ROUGH_TIMBER_FIELD_BOX)
        .unwrap();
    let barricade =
        validate_allocate_ground_stockpile(&state, SIDE, definition.maximum_stockpile_capacity())
            .unwrap()
            .commit(&mut state)
            .unwrap();
    deposit_lot_for_test(
        &registries,
        &mut state,
        barricade,
        CommodityKey::new(MATERIAL_WOOD, FORM_ROUGH_BOX_BODY),
        definition.assembly_profile().input_mass(),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_ROUGH_TIMBER_FIELD_BOX,
        barricade,
        barricade,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let initial = calculate_matter_accounting(&state).unwrap().total();
    advance_to(&registries, &mut state, 230);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(stockpile)
            .unwrap()
            .get_mass(COPPER),
        STORED,
        "the scout cannot pass through an enclosed intermediate route cell"
    );
    assert!(
        state
            .surface()
            .available_resources_at(Some(FAR))
            .next()
            .is_none()
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        initial
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn unreported_industry_cannot_trigger_a_far_stockpile_detour() {
    let (registries, mut state, stockpile, _) = scenario_at(false, FAR);
    advance_to(&registries, &mut state, 205);
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        TIP,
        "no report means the scout stays on its existing ordinary patrol"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(stockpile)
            .unwrap()
            .get_mass(COPPER),
        STORED,
    );
    assert!(
        state
            .surface()
            .available_resources_at(Some(FAR))
            .next()
            .is_none()
    );
}

#[test]
fn unwitnessed_ground_storage_does_not_trigger_an_industrial_raid() {
    let (registries, mut state, stockpile, _) = scenario(false);
    let initial = calculate_matter_accounting(&state).unwrap().total();
    advance_to(&registries, &mut state, 215);
    assert!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .mobilized_at
            .is_none()
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(stockpile)
            .unwrap()
            .get_mass(COPPER),
        STORED
    );
    assert!(
        state
            .surface()
            .available_resources_at(Some(JUNCTION))
            .next()
            .is_none()
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        initial
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn safely_distant_witness_sees_the_exact_stockpile_raid_without_deterring_it() {
    let (registries, mut state, stockpile, _) = scenario(true);
    advance_to(&registries, &mut state, 200);
    validate_admit_player(&registries, &state, VoxelCoord::new(2, 0, 0))
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let mut witnessed = None;
    while state.tick().value() < 205 {
        let outcome = advance_control_tick(&registries, &mut state).unwrap();
        if let Some(event) = outcome.simulation().and_then(|tick| tick.kobold_scout()) {
            witnessed = event.raided_stockpile();
        }
    }
    assert_eq!(
        witnessed,
        Some((stockpile, STOLEN)),
        "an observable theft needs exact local feedback without a hidden remote alert"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(stockpile)
            .unwrap()
            .get_mass(COPPER),
        Mass::from_milligrams(100_000),
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn physically_built_enclosure_prevents_the_raid_without_destroying_copper() {
    let (registries, mut state, stockpile, _) = scenario(true);
    let definition = registries
        .storage()
        .get(STORAGE_ROUGH_TIMBER_FIELD_BOX)
        .unwrap();
    deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_WOOD, FORM_ROUGH_BOX_BODY),
        definition.assembly_profile().input_mass(),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let before_build = calculate_matter_accounting(&state).unwrap().total();
    validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_ROUGH_TIMBER_FIELD_BOX,
        stockpile,
        stockpile,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        before_build,
        "enclosing stock must retain actual construction material"
    );
    let initial = calculate_matter_accounting(&state).unwrap().total();
    advance_to(&registries, &mut state, 225);
    assert!(
        state
            .inventory()
            .get_stockpile(stockpile)
            .unwrap()
            .enclosure()
            .is_some()
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(stockpile)
            .unwrap()
            .get_mass(COPPER),
        STORED,
        "a scavenger cannot enter or empty a finished material-backed enclosure"
    );
    assert!(
        state
            .surface()
            .available_resources_at(Some(JUNCTION))
            .next()
            .is_none()
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        initial
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn local_guard_prevents_the_stockpile_raid_and_remote_extraction_is_atomic() {
    let (registries, mut state, stockpile, lot) = scenario(true);
    let initial = calculate_matter_accounting(&state).unwrap().total();
    let before = state.clone();
    let at = state.tick();
    assert!(
        scavenge_unenclosed_ground_stockpile_material(
            &registries,
            &mut state,
            ScavengedGroundStockpileRequest::new(
                stockpile,
                lot,
                TIP,
                at,
                STOLEN,
                SCOUT_SAFE_SCAVENGE_TEMPERATURE,
            ),
        )
        .is_none(),
        "a source at a different voxel must not be transferred remotely"
    );
    assert_eq!(state, before);
    advance_to(&registries, &mut state, 200);
    validate_admit_player(&registries, &state, TIP)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    advance_to(&registries, &mut state, 205);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(stockpile)
            .unwrap()
            .get_mass(COPPER),
        STORED
    );
    assert!(
        state
            .surface()
            .available_resources_at(Some(JUNCTION))
            .next()
            .is_none()
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        initial
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
