//! Ceramic cavity and the first copper casting share real equipment, heat, and custody.

use super::*;
use crate::content::crafted_parts::CLAY_CERAMIC_INGOT_MOLD_BODY_MASS;
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
use crate::energy::validate_assemble_energy_store;
use crate::equipment::{EquipmentAssemblyError, validate_assemble_equipment};
use crate::inventory::{
    MaterialLotSelection, StockpileStorageProfile, add_solid_stockpile_for_test, add_stockpile,
    deposit_lot_for_test,
};
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::production::validate_start_process;
use crate::simulation::advance_tick;
use crate::thermal::{CastingRequest, resolve_casting_process};

#[test]
fn fired_ceramic_mold_is_cool_assembled_and_actually_casts_molten_copper() {
    let registries = build_registries();
    let fired = CommodityKey::new(MATERIAL_CLAY, FORM_FIRED_INGOT_MOLD);
    let temperature = Temperature::from_millikelvin(293_150);
    let mut state = AppState::new();
    let parts = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(5_000_000)).unwrap();
    deposit_lot_for_test(
        &registries,
        &mut state,
        parts,
        fired,
        CLAY_CERAMIC_INGOT_MOLD_BODY_MASS,
        temperature,
    )
    .unwrap();
    deposit_lot_for_test(
        &registries,
        &mut state,
        parts,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(1_000_000),
        temperature,
    )
    .unwrap();
    deposit_lot_for_test(
        &registries,
        &mut state,
        parts,
        CommodityKey::new(MATERIAL_STONE, FORM_STONE_CROCK_BODY),
        Mass::from_milligrams(2_400_000),
        temperature,
    )
    .unwrap();
    let ceramic = validate_assemble_equipment(
        &registries,
        &state,
        EQUIPMENT_FIRED_CERAMIC_INGOT_MOLD,
        parts,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let stone = validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_INGOT_MOLD, parts)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let sink = validate_assemble_energy_store(
        &registries,
        &state,
        ENERGY_STONE_CERAMIC_COOLING_BED,
        parts,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();

    let molten_source = add_stockpile(
        &mut state,
        Mass::from_milligrams(100_000),
        StockpileStorageProfile::new(false, true, Temperature::from_millikelvin(1_600_000))
            .unwrap(),
    )
    .unwrap();
    let cast_destination =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000)).unwrap();
    let batch_mass = Mass::from_milligrams(20_000);
    let copper = deposit_lot_for_test(
        &registries,
        &mut state,
        molten_source,
        CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN),
        batch_mass,
        Temperature::from_millikelvin(1_357_770),
    )
    .unwrap();
    let baseline = calculate_matter_accounting(&state).unwrap().total();
    let selection = [MaterialLotSelection::new(copper, batch_mass)];
    let resolve = |mold| {
        resolve_casting_process(
            &registries,
            &state,
            CastingRequest::new(
                PROCESS_CAST_PURE_COPPER,
                molten_source,
                &selection,
                mold,
                sink,
            ),
        )
    };
    let ceramic_cast = resolve(ceramic).unwrap();
    let stone_cast = resolve(stone).unwrap();
    assert!(
        ceramic_cast.process_resolution().duration() < stone_cast.process_resolution().duration(),
        "reusable kiln ceramic should cast a first-foundry batch faster than carved stone"
    );
    assert!(
        registries
            .process_topology(PROCESS_CAST_PURE_COPPER)
            .unwrap()
            .nominal_providers()
            .contains(&EQUIPMENT_FIRED_CERAMIC_INGOT_MOLD)
    );
    assert!(ceramic_cast.released_energy().nanojoules() > 0);
    let job = validate_start_process(
        &registries,
        &state,
        ceramic_cast.process_resolution(),
        molten_source,
        cast_destination,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let end = state.production().get_job(job).unwrap().completes_at();
    while state.tick() < end {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    let cast_lot = state
        .inventory()
        .lots()
        .find(|lot| lot.stockpile() == cast_destination)
        .unwrap();
    assert_eq!(
        cast_lot.commodity(),
        CommodityKey::new(MATERIAL_COPPER, FORM_INGOT)
    );
    assert_eq!(cast_lot.mass(), batch_mass);
    assert_eq!(cast_lot.temperature(), temperature);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(molten_source)
            .unwrap()
            .get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN)),
        Mass::ZERO
    );
    assert!(
        state
            .energy()
            .get_store(sink)
            .unwrap()
            .stored()
            .nanojoules()
            > 0
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        baseline
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn kiln_hot_ceramic_is_not_equipment_ready_until_cooled() {
    let registries = build_registries();
    let mut state = AppState::new();
    let source =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000)).unwrap();
    let hot = Temperature::from_millikelvin(1_100_000);
    deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_CLAY, FORM_FIRED_INGOT_MOLD),
        CLAY_CERAMIC_INGOT_MOLD_BODY_MASS,
        hot,
    )
    .unwrap();
    let before = state.clone();
    assert_eq!(
        validate_assemble_equipment(
            &registries,
            &state,
            EQUIPMENT_FIRED_CERAMIC_INGOT_MOLD,
            source,
        )
        .err(),
        Some(EquipmentAssemblyError::AssemblyMaterialTooHot {
            commodity: CommodityKey::new(MATERIAL_CLAY, FORM_FIRED_INGOT_MOLD),
            temperature: hot,
            maximum: Temperature::from_millikelvin(353_150),
        })
    );
    assert_eq!(state, before);
}
