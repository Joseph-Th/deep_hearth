//! Built-in wood-fueled ceramic firing through the canonical thermal and production owners.

use super::crafted_parts::CLAY_CERAMIC_CROCK_BODY_MASS;
use super::*;
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
use crate::energy::{validate_assemble_energy_store, validate_combust_fuel};
use crate::equipment::validate_assemble_equipment;
use crate::inventory::{
    MaterialLotSelection, add_solid_stockpile_for_test, deposit_lot_for_test,
    validate_build_storage_enclosure,
};
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::production::validate_start_process;
use crate::registry::{CommoditySource, CommodityUse};
use crate::simulation::advance_tick;
use crate::thermal::{
    SensibleCoolingRequest, SensibleHeatingRequest, SensibleHeatingResolutionError,
    resolve_sensible_cooling_process, resolve_sensible_heating_process,
};

#[test]
fn dry_clay_greenware_requires_a_hot_fueled_kiln_and_fires_without_creating_matter() {
    let registries = build_registries();
    let kiln_definition = registries
        .equipment()
        .get_equipment(EQUIPMENT_STONE_PIT_KILN)
        .unwrap();
    assert_eq!(
        kiln_definition
            .capabilities()
            .get_capability(crate::content::capabilities::CAPABILITY_HEATING_POWER),
        None,
        "wood-fired kiln must not be discoverable as an electrical furnace"
    );
    assert_eq!(
        registries
            .process_topology(PROCESS_FIRE_CLAY_CROCK)
            .unwrap()
            .nominal_providers(),
        &[EQUIPMENT_STONE_PIT_KILN],
        "the firing process must have one physical kiln provider"
    );
    assert!(
        !registries
            .process_topology(PROCESS_HEAT_MATERIAL_BATCH)
            .unwrap()
            .nominal_providers()
            .contains(&EQUIPMENT_STONE_PIT_KILN),
        "electrical sensible heating must not advertise a kiln provider"
    );
    let green = CommodityKey::new(MATERIAL_CLAY, FORM_GREENWARE_CROCK);
    let fired = CommodityKey::new(MATERIAL_CLAY, FORM_FIRED_CROCK);
    let greenware_handbook = registries.commodity_handbook_entry(green).unwrap();
    assert!(greenware_handbook.uses().iter().any(
        |usage| matches!(usage, CommodityUse::ThermalConditioning { process }
            if *process == PROCESS_FIRE_CLAY_CROCK)
    ));
    let fired_handbook = registries.commodity_handbook_entry(fired).unwrap();
    assert!(fired_handbook.sources().iter().any(
        |source| matches!(source, CommoditySource::ThermalConditioning { process }
            if *process == PROCESS_FIRE_CLAY_CROCK)
    ));
    assert!(fired_handbook.uses().iter().any(
        |usage| matches!(usage, CommodityUse::ThermalConditioning { process }
            if *process == PROCESS_COOL_FIRED_CLAY_CROCK)
    ));
    let temperature = Temperature::from_millikelvin(293_150);
    let firing_temperature = Temperature::from_millikelvin(1_100_000);
    let mut state = AppState::new();
    let source =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20_000_000)).unwrap();
    let destination =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(4_000_000)).unwrap();
    let empty_provisions_store =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(6_000_000)).unwrap();
    let mass = CLAY_CERAMIC_CROCK_BODY_MASS;
    let lot =
        deposit_lot_for_test(&registries, &mut state, source, green, mass, temperature).unwrap();
    let raw_clay_lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_CLAY, FORM_LUMP),
        Mass::from_milligrams(300_000),
        temperature,
    )
    .unwrap();
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(6_000_000),
        ),
        (
            CommodityKey::new(MATERIAL_CLAY, FORM_PACKED_CLAY_BINDER),
            Mass::from_milligrams(1_000_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
            Mass::from_milligrams(800_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            CAMPFIRE_FUEL_MASS,
        ),
    ] {
        deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            commodity,
            mass,
            temperature,
        )
        .unwrap();
    }
    let equipment =
        validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_PIT_KILN, source)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    let hearth =
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_PIT_KILN_HEARTH, source)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    let cooling_bed = validate_assemble_energy_store(
        &registries,
        &state,
        ENERGY_STONE_CERAMIC_COOLING_BED,
        source,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let baseline = calculate_matter_accounting(&state).unwrap().total();
    validate_combust_fuel(&registries, &state, hearth, source)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let fired_heat = state.energy().get_store(hearth).unwrap().stored();
    assert_eq!(fired_heat, CAMPFIRE_FUEL_THERMAL_ENERGY);
    let selection = [MaterialLotSelection::new(lot, mass)];
    let request = |process, target| {
        SensibleHeatingRequest::new(process, source, &selection, equipment, hearth, target)
    };
    let before_rejection = state.clone();
    assert_eq!(
        resolve_sensible_heating_process(
            &registries,
            &state,
            SensibleHeatingRequest::new(
                PROCESS_FIRE_CLAY_CROCK,
                source,
                &[MaterialLotSelection::new(
                    raw_clay_lot,
                    Mass::from_milligrams(300_000)
                )],
                equipment,
                hearth,
                firing_temperature,
            ),
        )
        .unwrap_err(),
        SensibleHeatingResolutionError::WrongFiringInput {
            expected: green,
            actual: CommodityKey::new(MATERIAL_CLAY, FORM_LUMP),
        },
        "raw clay must be shaped first; heat alone cannot bypass greenware"
    );
    assert_eq!(
        resolve_sensible_heating_process(
            &registries,
            &state,
            request(
                PROCESS_FIRE_CLAY_CROCK,
                Temperature::from_millikelvin(900_000)
            )
        )
        .unwrap_err(),
        SensibleHeatingResolutionError::BelowFiringTemperature {
            required: firing_temperature,
            actual: Temperature::from_millikelvin(900_000),
        },
    );
    assert_eq!(state, before_rejection);
    let resolved = resolve_sensible_heating_process(
        &registries,
        &state,
        request(PROCESS_FIRE_CLAY_CROCK, firing_temperature),
    )
    .unwrap();
    assert!(resolved.required_energy() < fired_heat);
    assert!(resolved.required_energy().nanojoules() > 0);
    let job = validate_start_process(
        &registries,
        &state,
        resolved.process_resolution(),
        source,
        destination,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let end = state.production().get_job(job).unwrap().completes_at();
    while state.tick() < end {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert!(state.production().get_job(job).is_none());
    let fired_lot = state
        .inventory()
        .lots()
        .find(|lot| lot.stockpile() == destination)
        .unwrap();
    assert_eq!(fired_lot.commodity(), fired);
    assert_eq!(fired_lot.mass(), mass);
    assert_eq!(fired_lot.temperature(), firing_temperature);
    let fired_lot_id = fired_lot.id();
    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .unwrap()
            .get_mass(green),
        Mass::ZERO
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        baseline
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    // Greenware must cool through actual heat custody before its body is
    // installed in a provisions enclosure; a timer alone would erase energy.
    let hot_crock = [MaterialLotSelection::new(fired_lot_id, mass)];
    let cooled = resolve_sensible_cooling_process(
        &registries,
        &state,
        SensibleCoolingRequest::new(
            PROCESS_COOL_FIRED_CLAY_CROCK,
            destination,
            &hot_crock,
            equipment,
            cooling_bed,
        ),
    )
    .unwrap();
    assert_eq!(cooled.target(), temperature);
    assert!(cooled.released_energy().nanojoules() > 0);
    let cool_job = validate_start_process(
        &registries,
        &state,
        cooled.process_resolution(),
        destination,
        destination,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let save = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay: AppState = serde_json::from_slice::<LoadedSaveEnvelope>(&save)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let cool_end = state.production().get_job(cool_job).unwrap().completes_at();
    while state.tick() < cool_end {
        let original = advance_tick(&registries, &mut state).unwrap();
        let restarted = advance_tick(&registries, &mut replay).unwrap();
        assert_eq!(original, restarted);
    }
    assert_eq!(
        state, replay,
        "hot ceramic cooling must replay identically across a save"
    );
    let cooled_lot = state
        .inventory()
        .lots()
        .find(|lot| lot.stockpile() == destination && lot.commodity() == fired)
        .unwrap();
    assert_eq!(cooled_lot.temperature(), temperature);
    assert_eq!(cooled_lot.mass(), mass);
    let already_cool = [MaterialLotSelection::new(cooled_lot.id(), mass)];
    let before_rejection = state.clone();
    assert_eq!(
        resolve_sensible_cooling_process(
            &registries,
            &state,
            SensibleCoolingRequest::new(
                PROCESS_COOL_FIRED_CLAY_CROCK,
                destination,
                &already_cool,
                equipment,
                cooling_bed,
            ),
        )
        .unwrap_err(),
        crate::thermal::SensibleCoolingResolutionError::Batch(
            crate::thermal::SensibleCoolingBatchError::InputNotHotter {
                input: temperature,
                target: temperature,
            },
        ),
    );
    assert_eq!(state, before_rejection);
    assert!(
        state
            .energy()
            .get_store(cooling_bed)
            .unwrap()
            .stored()
            .nanojoules()
            > 0,
        "heat released by cooling must reside in the finite cooling bed"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let ceramic = registries
        .storage()
        .get(STORAGE_FIRED_CERAMIC_PROVISIONS_CROCK)
        .unwrap();
    let stone = registries
        .storage()
        .get(STORAGE_CARVED_STONE_PROVISIONS_CROCK)
        .unwrap();
    assert!(
        ceramic.storage_profile().preservation_multiplier_ppm()
            > stone.storage_profile().preservation_multiplier_ppm()
    );
    assert!(
        ceramic.assembly_profile().input_mass() < stone.assembly_profile().input_mass(),
        "fired ceramic should be lighter than the carved-stone body"
    );
    assert!(
        ceramic.storage_profile().preservation_multiplier_ppm()
            >= stone.storage_profile().preservation_multiplier_ppm() + 1_000_000,
        "a kiln investment must buy more than a marginal preservation gain"
    );
    assert_eq!(ceramic.assembly_profile().inputs()[0].commodity(), fired);
    validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_FIRED_CERAMIC_PROVISIONS_CROCK,
        empty_provisions_store,
        destination,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let installed = state
        .inventory()
        .get_stockpile(empty_provisions_store)
        .unwrap();
    assert_eq!(installed.storage_profile(), ceramic.storage_profile());
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .unwrap()
            .get_mass(fired),
        Mass::ZERO,
        "fired vessel must be embodied in the storage enclosure, not duplicated"
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        baseline
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
