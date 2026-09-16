//! Contract tests for represented-matter accounting.

use super::*;
use crate::content::{
    FORM_LOG, MATERIAL_WOOD, STANDARD_TEST_HEATER, STANDARD_TEST_HEATING_ENERGY,
    make_test_registries_with_standard_sensible_heating,
};
use crate::core::quantity::{Energy, Mass, Temperature};
use crate::core::time::WorldSeed;
use crate::energy::add_energy_store_with_initial_for_fixture;
use crate::equipment::add_equipment;
use crate::inventory::{
    MaterialLotSelection, StockpileId, add_solid_stockpile_for_test, deposit_bulk_for_test,
    validate_material_transfer_for_test,
};
use crate::maintenance::Condition;
use crate::material::CommodityKey;
use crate::production::{ProcessId, validate_start_process};
use crate::simulation::advance_tick;
use crate::thermal::{
    ResolvedSensibleHeating, SensibleHeatingRequest, resolve_sensible_heating_process,
};

const PROCESS: ProcessId = ProcessId::new(910_001);

fn resolve_all_source_matter(
    registries: &crate::registry::Registries,
    state: &mut AppState,
    source: StockpileId,
) -> ResolvedSensibleHeating {
    let selections = state
        .inventory()
        .lot_ids(source)
        .map(|lot| {
            let mass = state
                .inventory()
                .get_lot(lot)
                .unwrap_or_else(|| panic!("matter-accounting fixture lost lot {}", lot.value()))
                .mass();
            MaterialLotSelection::new(lot, mass)
        })
        .collect::<Vec<_>>();
    let equipment = add_equipment(registries, state, STANDARD_TEST_HEATER, Condition::PRISTINE)
        .unwrap_or_else(|error| panic!("matter-accounting heater fixture failed: {error}"));
    let energy = add_energy_store_with_initial_for_fixture(
        registries,
        state,
        STANDARD_TEST_HEATING_ENERGY,
        Energy::from_nanojoules(10_000_000_000),
    )
    .unwrap_or_else(|error| panic!("matter-accounting energy fixture failed: {error}"));
    resolve_sensible_heating_process(
        registries,
        state,
        SensibleHeatingRequest::new(
            PROCESS,
            source,
            &selections,
            equipment,
            energy,
            Temperature::from_millikelvin(294_150),
        ),
    )
    .unwrap_or_else(|error| panic!("matter-accounting heating resolution failed: {error}"))
}

#[test]
fn process_start_and_completion_preserve_world_matter_ownership_total() {
    let registries = make_test_registries_with_standard_sensible_heating(PROCESS);
    let mut state = AppState::new(WorldSeed::new(0x0ACC_0017));
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
        Ok(id) => id,
        Err(error) => panic!("source fixture failed: {error}"),
    };
    let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
        Ok(id) => id,
        Err(error) => panic!("destination fixture failed: {error}"),
    };
    if let Err(error) = deposit_bulk_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(10),
    ) {
        panic!("matter fixture deposit failed: {error}");
    }
    let resolution = resolve_all_source_matter(&registries, &mut state, source);
    let duration = resolution.process_resolution().duration();
    let before = match calculate_matter_accounting(&state) {
        Ok(accounting) => accounting,
        Err(error) => panic!("initial accounting failed: {error}"),
    };

    let token = match validate_start_process(
        &registries,
        &state,
        resolution.process_resolution(),
        source,
        destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("process validation failed: {error}"),
    };
    if let Err(error) = token.commit(&mut state) {
        panic!("process commit failed: {error}");
    }
    let running = match calculate_matter_accounting(&state) {
        Ok(accounting) => accounting,
        Err(error) => panic!("running accounting failed: {error}"),
    };

    assert_eq!(before.total(), running.total());
    assert_eq!(running.stored(), AggregateMass::ZERO);
    assert_eq!(running.in_process(), AggregateMass::from_milligrams(10));

    for _ in 0..duration.value() {
        if let Err(error) = advance_tick(&registries, &mut state) {
            panic!("completion tick failed: {error}");
        }
    }
    let completed = match calculate_matter_accounting(&state) {
        Ok(accounting) => accounting,
        Err(error) => panic!("completed accounting failed: {error}"),
    };

    assert_eq!(before.total(), completed.total());
    assert_eq!(completed.in_process(), AggregateMass::ZERO);
    assert_eq!(completed.stored(), AggregateMass::from_milligrams(10));
}

#[test]
fn transfer_split_then_process_lifecycle_preserves_world_matter_total() {
    let registries = make_test_registries_with_standard_sensible_heating(PROCESS);
    let mut state = AppState::new(WorldSeed::new(0x0ACC_0018));
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(id) => id,
        Err(error) => panic!("source fixture failed: {error}"),
    };
    let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(id) => id,
        Err(error) => panic!("destination fixture failed: {error}"),
    };
    let holding = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(id) => id,
        Err(error) => panic!("holding fixture failed: {error}"),
    };
    for chunk in [Mass::from_milligrams(4), Mass::from_milligrams(6)] {
        if let Err(error) = deposit_bulk_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            chunk,
        ) {
            panic!("split deposit fixture failed: {error}");
        }
    }
    let token = match validate_material_transfer_for_test(
        &registries,
        &state,
        source,
        holding,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(10),
    ) {
        Ok(token) => token,
        Err(error) => panic!("move-to-holding validation failed: {error}"),
    };
    if let Err(error) = token.commit(&mut state) {
        panic!("move-to-holding commit failed: {error}");
    }

    let resolution = resolve_all_source_matter(&registries, &mut state, holding);
    let duration = resolution.process_resolution().duration();
    let before = match calculate_matter_accounting(&state) {
        Ok(accounting) => accounting,
        Err(error) => panic!("pre-process accounting failed: {error}"),
    };
    let start = match validate_start_process(
        &registries,
        &state,
        resolution.process_resolution(),
        holding,
        destination,
    ) {
        Ok(start) => start,
        Err(error) => panic!("process start validation failed: {error}"),
    };
    if let Err(error) = start.commit(&mut state) {
        panic!("process start commit failed: {error}");
    }
    let running = match calculate_matter_accounting(&state) {
        Ok(accounting) => accounting,
        Err(error) => panic!("running accounting failed: {error}"),
    };
    assert_eq!(before.total(), running.total());
    assert_eq!(running.stored(), AggregateMass::ZERO);
    assert_eq!(running.in_process(), AggregateMass::from_milligrams(10));

    for _ in 0..duration.value() {
        if let Err(error) = advance_tick(&registries, &mut state) {
            panic!("completion tick failed: {error}");
        }
    }
    let completed = match calculate_matter_accounting(&state) {
        Ok(accounting) => accounting,
        Err(error) => panic!("completed accounting failed: {error}"),
    };
    assert_eq!(before.total(), completed.total());
    assert_eq!(completed.in_process(), AggregateMass::ZERO);
    assert_eq!(completed.stored(), AggregateMass::from_milligrams(10));
}
