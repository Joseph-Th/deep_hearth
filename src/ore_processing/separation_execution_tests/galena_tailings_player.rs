//! Actual hand-picked galena residues become furnace-ready sulfide by local mechanical work.

use super::*;
use crate::content::{
    EQUIPMENT_STONE_ROTARY_QUERN, MATERIAL_GALENA, PROCESS_CLEAN_GALENA_CONCENTRATE,
    PROCESS_HAND_PICK_GALENA, PROCESS_REGRIND_GALENA_PICKING_TAILINGS,
    PROCESS_SCAVENGE_GALENA_TAILINGS,
};
use crate::inventory::{MaterialLotSelection, StockpileId};
use crate::logistics::{place_energy_store_for_fixture, place_equipment_for_fixture};
use crate::ore_processing::{ComminutionRequest, resolve_comminution_process};
use crate::production::{validate_start_player_process, validate_start_player_process_routed};

fn selections(state: &AppState, source: StockpileId) -> Vec<MaterialLotSelection> {
    state
        .inventory()
        .lot_ids(source)
        .map(|id| {
            let lot = state.inventory().get_lot(id).unwrap();
            MaterialLotSelection::new(id, lot.mass())
        })
        .collect()
}

#[test]
fn selected_galena_prills_and_all_residue_lots_reach_separate_conserved_products() {
    let initial = Mass::from_milligrams(100_000);
    let composition = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_GALENA, 800_000),
        CompositionComponent::new(MATERIAL_STONE, 200_000),
    ])
    .unwrap();
    let mut fixture = fixture_with_host_and_particle_size(
        MATERIAL_GALENA,
        initial,
        composition,
        hand_sortable_particle_size(),
    );
    let assembly_profile = fixture
        .registries
        .equipment()
        .get_equipment(EQUIPMENT_STONE_ROTARY_QUERN)
        .unwrap()
        .assembly_profile()
        .unwrap();
    let parts =
        add_solid_stockpile_for_test(&mut fixture.state, assembly_profile.input_mass()).unwrap();
    for input in assembly_profile.inputs() {
        deposit_lot_for_test(
            &fixture.registries,
            &mut fixture.state,
            parts,
            input.commodity(),
            input.mass(),
            TEMPERATURE,
        )
        .unwrap();
    }
    let quern = validate_assemble_equipment(
        &fixture.registries,
        &fixture.state,
        EQUIPMENT_STONE_ROTARY_QUERN,
        parts,
    )
    .unwrap()
    .commit(&mut fixture.state)
    .unwrap();
    let reground_bin = add_solid_stockpile_for_test(&mut fixture.state, initial).unwrap();
    let concentrate_bin = add_solid_stockpile_for_test(&mut fixture.state, initial).unwrap();
    let exhausted_bin = add_solid_stockpile_for_test(&mut fixture.state, initial).unwrap();
    let clean_bin = add_solid_stockpile_for_test(&mut fixture.state, initial).unwrap();
    let at = VoxelCoord::new(0, 0, 0);
    for stockpile in [
        fixture.source,
        fixture.target,
        fixture.residue,
        reground_bin,
        concentrate_bin,
        exhausted_bin,
        clean_bin,
    ] {
        validate_place_ground_stockpile(&fixture.state, stockpile, at)
            .unwrap()
            .commit(&mut fixture.state)
            .unwrap();
    }
    for equipment in [quern, fixture.separator] {
        place_equipment_for_fixture(&mut fixture.state, equipment, at);
    }
    place_energy_store_for_fixture(&mut fixture.state, fixture.energy, at);
    validate_admit_player(&fixture.registries, &fixture.state, at)
        .unwrap()
        .commit(&mut fixture.state)
        .unwrap();
    let initial_matter = calculate_matter_accounting(&fixture.state).unwrap().total();

    let picked = resolve_manual_constituent_separation_process(
        &fixture.registries,
        &fixture.state,
        ManualConstituentSeparationRequest::new(
            PROCESS_HAND_PICK_GALENA,
            fixture.source,
            &[MaterialLotSelection::new(fixture.lot, initial)],
        ),
    )
    .unwrap();
    validate_start_manual_constituent_separation(
        &fixture.registries,
        &fixture.state,
        &picked,
        fixture.source,
        fixture.target,
        fixture.residue,
    )
    .unwrap()
    .commit(&mut fixture.state)
    .unwrap();
    for _ in 0..picked.duration().value() {
        let _ = advance_tick(&fixture.registries, &mut fixture.state).unwrap();
    }
    assert_eq!(
        fixture
            .state
            .inventory()
            .get_stockpile(fixture.target)
            .unwrap()
            .stored_mass()
            .milligrams(),
        52_000
    );
    assert_eq!(
        fixture
            .state
            .inventory()
            .get_stockpile(fixture.residue)
            .unwrap()
            .stored_mass()
            .milligrams(),
        48_000
    );
    let tailing_lots = selections(&fixture.state, fixture.residue);
    assert!(
        tailing_lots.len() >= 2,
        "fraction-exact separation must not flatten individual residue compositions"
    );
    let ground = resolve_comminution_process(
        &fixture.registries,
        &fixture.state,
        ComminutionRequest::new(
            PROCESS_REGRIND_GALENA_PICKING_TAILINGS,
            fixture.residue,
            &tailing_lots,
            quern,
            fixture.energy,
        ),
    )
    .unwrap();
    let ticks = ground.process_resolution().duration().value();
    validate_start_player_process(
        &fixture.registries,
        &fixture.state,
        ground.process_resolution(),
        fixture.residue,
        reground_bin,
    )
    .unwrap()
    .commit(&mut fixture.state)
    .unwrap();
    for _ in 0..ticks {
        let _ = advance_tick(&fixture.registries, &mut fixture.state).unwrap();
    }
    assert_eq!(
        fixture
            .state
            .inventory()
            .get_stockpile(reground_bin)
            .unwrap()
            .stored_mass()
            .milligrams(),
        48_000
    );

    let scavenged = resolve_constituent_separation_process(
        &fixture.registries,
        &fixture.state,
        ConstituentSeparationRequest::new(
            PROCESS_SCAVENGE_GALENA_TAILINGS,
            reground_bin,
            &selections(&fixture.state, reground_bin),
            fixture.separator,
            fixture.energy,
        ),
    )
    .unwrap();
    assert_eq!(scavenged.target_mass().milligrams(), 24_400);
    let ticks = scavenged.process_resolution().duration().value();
    validate_start_player_process_routed(
        &fixture.registries,
        &fixture.state,
        scavenged.process_resolution(),
        reground_bin,
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
    .commit(&mut fixture.state)
    .unwrap();
    for _ in 0..ticks {
        let _ = advance_tick(&fixture.registries, &mut fixture.state).unwrap();
    }

    let clean = resolve_constituent_separation_process(
        &fixture.registries,
        &fixture.state,
        ConstituentSeparationRequest::new(
            PROCESS_CLEAN_GALENA_CONCENTRATE,
            concentrate_bin,
            &selections(&fixture.state, concentrate_bin),
            fixture.separator,
            fixture.energy,
        ),
    )
    .unwrap();
    assert_eq!(clean.target_mass().milligrams(), 20_160);
    let ticks = clean.process_resolution().duration().value();
    validate_start_player_process_routed(
        &fixture.registries,
        &fixture.state,
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
    .commit(&mut fixture.state)
    .unwrap();
    let saved =
        serde_json::to_vec(&SaveEnvelope::new(&fixture.registries, &fixture.state)).unwrap();
    let mut resumed = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&fixture.registries)
        .unwrap();
    for _ in 0..ticks {
        let _ = advance_tick(&fixture.registries, &mut fixture.state).unwrap();
        let _ = advance_tick(&fixture.registries, &mut resumed).unwrap();
    }
    assert_eq!(fixture.state, resumed);
    assert_eq!(
        fixture
            .state
            .inventory()
            .get_stockpile(clean_bin)
            .unwrap()
            .stored_mass()
            .milligrams(),
        20_160
    );
    assert_eq!(
        fixture
            .state
            .inventory()
            .get_stockpile(exhausted_bin)
            .unwrap()
            .stored_mass()
            .milligrams(),
        27_840
    );
    for id in fixture.state.inventory().lot_ids(clean_bin) {
        let lot = fixture.state.inventory().get_lot(id).unwrap();
        assert_eq!(
            lot.commodity(),
            CommodityKey::new(MATERIAL_GALENA, FORM_CRUSHED)
        );
        assert_eq!(
            lot.composition(),
            &MaterialComposition::pure(MATERIAL_GALENA)
        );
    }
    assert_eq!(
        calculate_matter_accounting(&fixture.state).unwrap().total(),
        initial_matter
    );
    assert_eq!(
        validate_loaded_state(&fixture.registries, &fixture.state),
        Ok(())
    );
}
