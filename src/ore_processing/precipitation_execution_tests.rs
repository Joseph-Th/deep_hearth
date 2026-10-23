//! Chemical reaction mass references agree with the fixed-point furnace resolver.
use super::*;
use crate::content::{
    ENERGY_ELECTRICAL_BUFFER, ENERGY_THERMAL_SINK, EQUIPMENT_CASTING_MOLD,
    EQUIPMENT_ELECTRIC_FURNACE, FORM_INGOT, FORM_LOG, MATERIAL_WOOD, PROCESS_CAST_LEAD_INGOT,
    PROCESS_MELT_RECYCLED_LEAD, PROCESS_PRECIPITATE_LEAD_FROM_GALENA,
    STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
};
use crate::core::quantity::{Area, Length};
use crate::core::state::{StateValidationError, validate_loaded_state};
use crate::energy::add_energy_store_with_initial_for_fixture;
use crate::equipment::{add_equipment, validate_mount_equipment};
use crate::inventory::{
    StockpileStorageProfile, add_solid_stockpile_for_test, add_stockpile, deposit_lot_for_test,
    deposit_lot_spec_for_test,
};
use crate::maintenance::Condition;
use crate::material::{MaterialComposition, ParticleSizeRange};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::production::{
    ProcessOutputRoute, validate_start_process, validate_start_process_routed,
};
use crate::simulation::advance_tick;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::structural::{
    add_structural_element, materialize_structural_element_for_test,
    validate_activate_structural_element,
};
use crate::thermal::{
    CastingRequest, MeltingRequest, resolve_casting_process, resolve_melting_process,
};

#[path = "precipitation_execution_tests/galena_chain.rs"]
mod galena_chain;

#[path = "precipitation_execution_tests/lead_recycling.rs"]
mod lead_recycling;

fn mount_machine_fixture(
    registries: &Registries,
    state: &mut AppState,
    equipment: EquipmentId,
    x: i64,
) {
    let bounds = VoxelBounds::new(VoxelCoord::new(x, 0, 0), VoxelCoord::new(x + 1, 1, 1)).unwrap();
    let support = add_structural_element(
        registries,
        state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        crate::structural::make_test_structural_geometry(
            bounds,
            Length::from_micrometers(1),
            Area::from_square_millimeters(500_000),
        ),
        true,
    )
    .unwrap();
    materialize_structural_element_for_test(registries, state, support, FORM_LOG);
    let _ = validate_activate_structural_element(registries, state, support)
        .unwrap()
        .commit(state)
        .unwrap();
    let _ = validate_mount_equipment(registries, state, equipment, support)
        .unwrap()
        .commit(state)
        .unwrap();
}

#[test]
fn process_output_mass_matches_balanced_precipitation_equation() {
    let reaction = crate::content::reactions::REACTIONS[9];
    assert!(reaction.is_balanced());
    assert_eq!(reaction.mass_milligrams(true), 5 * (GALENA_MG + IRON_MG));
    assert_eq!(reaction.mass_milligrams(false), 5 * (LEAD_MG + MATTE_MG));
}

struct PrecipitationFixture {
    registries: Registries,
    state: AppState,
    source: StockpileId,
    lead_destination: StockpileId,
    matte_destination: StockpileId,
    galena: crate::inventory::MaterialLotId,
    iron: crate::inventory::MaterialLotId,
    furnace: EquipmentId,
    electricity: EnergyStoreId,
}

fn charge_fixture() -> PrecipitationFixture {
    let registries = build_registries();
    let mut state = AppState::new();
    let source =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000)).unwrap();
    let lead_destination = add_stockpile(
        &mut state,
        Mass::from_milligrams(1_000_000),
        StockpileStorageProfile::new(false, true, Temperature::from_millikelvin(1_500_000))
            .unwrap(),
    )
    .unwrap();
    let matte_destination =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000)).unwrap();
    let temperature = Temperature::from_millikelvin(293_150);
    let galena_commodity = CommodityKey::new(MATERIAL_GALENA, FORM_CRUSHED);
    let galena_spec = MaterialLotSpec::with_composition_and_particle_size(
        galena_commodity,
        Mass::from_milligrams(GALENA_MG),
        temperature,
        MaterialComposition::pure(MATERIAL_GALENA),
        ParticleSizeRange::new(
            Length::from_micrometers(500),
            Length::from_micrometers(2_000),
        )
        .unwrap(),
    )
    .unwrap();
    let galena = deposit_lot_spec_for_test(&registries, &mut state, source, galena_spec).unwrap();
    let iron = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_IRON, FORM_SCRAP),
        Mass::from_milligrams(IRON_MG),
        temperature,
    )
    .unwrap();
    let furnace = add_equipment(
        &registries,
        &mut state,
        EQUIPMENT_ELECTRIC_FURNACE,
        Condition::PRISTINE,
    )
    .unwrap();
    mount_machine_fixture(&registries, &mut state, furnace, 0);
    let electricity = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        ENERGY_ELECTRICAL_BUFFER,
        Energy::from_nanojoules(2_000_000_000_000_000),
    )
    .unwrap();
    PrecipitationFixture {
        registries,
        state,
        source,
        lead_destination,
        matte_destination,
        galena,
        iron,
        furnace,
        electricity,
    }
}

#[test]
fn precipitation_rejects_missing_and_imbalanced_reactants_without_spending_resources() {
    let fixture = charge_fixture();
    let galena = MaterialLotSelection::new(fixture.galena, Mass::from_milligrams(GALENA_MG));
    let iron = MaterialLotSelection::new(fixture.iron, Mass::from_milligrams(IRON_MG));
    let before = fixture.state.clone();
    let missing = [galena];
    assert_eq!(
        resolve_precipitation_process(
            &fixture.registries,
            &fixture.state,
            PrecipitationRequest::new(
                PROCESS_PRECIPITATE_LEAD_FROM_GALENA,
                fixture.source,
                &missing,
                fixture.furnace,
                fixture.electricity,
            )
        )
        .err(),
        Some(PrecipitationResolutionError::Batch(
            PrecipitationBatchError::MissingReactant
        ))
    );
    for (galena_mass, iron_mass) in [(GALENA_MG - 1, IRON_MG), (GALENA_MG, IRON_MG - 1)] {
        let selections = [
            MaterialLotSelection::new(fixture.galena, Mass::from_milligrams(galena_mass)),
            MaterialLotSelection::new(fixture.iron, Mass::from_milligrams(iron_mass)),
        ];
        assert_eq!(
            resolve_precipitation_process(
                &fixture.registries,
                &fixture.state,
                PrecipitationRequest::new(
                    PROCESS_PRECIPITATE_LEAD_FROM_GALENA,
                    fixture.source,
                    &selections,
                    fixture.furnace,
                    fixture.electricity,
                )
            )
            .err(),
            Some(PrecipitationResolutionError::Batch(
                PrecipitationBatchError::InvalidMolarCharge {
                    galena: Mass::from_milligrams(galena_mass),
                    iron: Mass::from_milligrams(iron_mass),
                }
            ))
        );
    }
    let selections = [galena, iron];
    let wrong_process = crate::content::PROCESS_MELT_PURE_COPPER;
    assert!(matches!(
        resolve_precipitation_process(&fixture.registries, &fixture.state,
            PrecipitationRequest::new(
                wrong_process, fixture.source, &selections,
                fixture.furnace, fixture.electricity,
            )),
        Err(PrecipitationResolutionError::UnknownProcess { process }) if process == wrong_process
    ));
    assert_eq!(fixture.state, before);
}

#[test]
fn physically_selected_galena_and_iron_complete_into_separately_routed_lead_and_matte() {
    let PrecipitationFixture {
        registries,
        mut state,
        source,
        lead_destination,
        matte_destination,
        galena,
        iron,
        furnace,
        electricity,
    } = charge_fixture();
    let selections = [
        MaterialLotSelection::new(galena, Mass::from_milligrams(GALENA_MG)),
        MaterialLotSelection::new(iron, Mass::from_milligrams(IRON_MG)),
    ];
    let matter_before = calculate_matter_accounting(&state).unwrap().total();
    let resolved = resolve_precipitation_process(
        &registries,
        &state,
        PrecipitationRequest::new(
            PROCESS_PRECIPITATE_LEAD_FROM_GALENA,
            source,
            &selections,
            furnace,
            electricity,
        ),
    )
    .expect("real furnace must admit the balanced two-reactant charge");
    let streams = resolved.process_resolution().output_streams();
    assert_eq!(streams.len(), 2);
    assert_eq!(
        streams[0].outputs()[0].commodity(),
        CommodityKey::new(MATERIAL_LEAD, FORM_MOLTEN)
    );
    assert_eq!(
        streams[0].outputs()[0].mass(),
        Mass::from_milligrams(LEAD_MG)
    );
    assert_eq!(
        streams[1].outputs()[0].commodity(),
        CommodityKey::new(MATERIAL_IRON_SULFIDE, FORM_MATTE)
    );
    assert_eq!(
        streams[1].outputs()[0].mass(),
        Mass::from_milligrams(MATTE_MG)
    );
    assert!(resolved.required_energy() > Energy::ZERO);
    let duration = resolved.process_resolution().duration();
    let job = validate_start_process_routed(
        &registries,
        &state,
        resolved.process_resolution(),
        source,
        &[
            ProcessOutputRoute::new(ProcessOutputStreamId::PRIMARY, lead_destination),
            ProcessOutputRoute::new(ProcessOutputStreamId::RESIDUE, matte_destination),
        ],
    )
    .expect("reaction must atomically reserve both output streams")
    .commit(&mut state)
    .unwrap();
    assert!(state.production().get_job(job).is_some());
    validate_loaded_state(&registries, &state).expect("in-flight chemistry must replay");
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .expect("in-flight reaction must persist");
    assert_eq!(loaded, state);
    let mut tampered = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    tampered["state"]["systems"]["production"]["jobs"][job.value().to_string()]["output_streams"]
        [0]["outputs"][0]["temperature"] = serde_json::json!(1_325_001_u64);
    let forged = serde_json::from_value::<LoadedSaveEnvelope>(tampered).unwrap();
    assert_eq!(
        forged.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::PrecipitationJob(
                PrecipitationJobValidationError::OutputMismatch { job },
            )
        )),
        "a physically plausible but unauthored molten output temperature must fail replay"
    );
    for _ in 0..duration.value() {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(
        state
            .inventory()
            .get_stockpile(lead_destination)
            .unwrap()
            .get_mass(CommodityKey::new(MATERIAL_LEAD, FORM_MOLTEN)),
        Mass::from_milligrams(LEAD_MG)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(matte_destination)
            .unwrap()
            .get_mass(CommodityKey::new(MATERIAL_IRON_SULFIDE, FORM_MATTE)),
        Mass::from_milligrams(MATTE_MG)
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        matter_before
    );
    validate_loaded_state(&registries, &state).unwrap();
    // The hot first-stage product is actual custody, not an abstract unlock.
    // A separately supported cooled mold must transfer its latent and sensible
    // heat to a finite thermal sink before an ingot exists.
    let lead_lot = state.inventory().lot_ids(lead_destination).next().unwrap();
    let ingot_destination =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000)).unwrap();
    let mold = add_equipment(
        &registries,
        &mut state,
        EQUIPMENT_CASTING_MOLD,
        Condition::PRISTINE,
    )
    .unwrap();
    mount_machine_fixture(&registries, &mut state, mold, 2);
    let thermal_sink = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        ENERGY_THERMAL_SINK,
        Energy::ZERO,
    )
    .unwrap();
    let matter_before_casting = calculate_matter_accounting(&state).unwrap().total();
    let cast_selection = [MaterialLotSelection::new(
        lead_lot,
        Mass::from_milligrams(LEAD_MG),
    )];
    let casting = resolve_casting_process(
        &registries,
        &state,
        CastingRequest::new(
            PROCESS_CAST_LEAD_INGOT,
            lead_destination,
            &cast_selection,
            mold,
            thermal_sink,
        ),
    )
    .expect("the liquid lead must be castable with a real cooled mold");
    assert!(casting.released_energy() > Energy::ZERO);
    let casting_duration = casting.process_resolution().duration();
    let _casting_job = validate_start_process(
        &registries,
        &state,
        casting.process_resolution(),
        lead_destination,
        ingot_destination,
    )
    .expect("lead ingot output must reserve a real solids destination")
    .commit(&mut state)
    .unwrap();
    validate_loaded_state(&registries, &state).unwrap();
    for _ in 0..casting_duration.value() {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(
        state
            .inventory()
            .get_stockpile(lead_destination)
            .unwrap()
            .get_mass(CommodityKey::new(MATERIAL_LEAD, FORM_MOLTEN)),
        Mass::ZERO,
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(ingot_destination)
            .unwrap()
            .get_mass(CommodityKey::new(MATERIAL_LEAD, FORM_INGOT)),
        Mass::from_milligrams(LEAD_MG),
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        matter_before_casting
    );
    validate_loaded_state(&registries, &state).unwrap();

    // The ingot is not a terminal recipe trophy: melt the actually cast
    // product together with a separately recovered pure lead scrap lot.
    // All work is electrical and the molten destination is still heat-rated.
    let ingot = state.inventory().lot_ids(ingot_destination).next().unwrap();
    let reclaimed = deposit_lot_for_test(
        &registries,
        &mut state,
        ingot_destination,
        CommodityKey::new(MATERIAL_LEAD, crate::content::FORM_SCRAP),
        Mass::from_milligrams(5_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let input = [
        MaterialLotSelection::new(ingot, Mass::from_milligrams(LEAD_MG)),
        MaterialLotSelection::new(reclaimed, Mass::from_milligrams(5_000)),
    ];
    let matter_before_remelt = calculate_matter_accounting(&state).unwrap().total();
    let remelt = resolve_melting_process(
        &registries,
        &state,
        MeltingRequest::new(
            PROCESS_MELT_RECYCLED_LEAD,
            ingot_destination,
            &input,
            furnace,
            electricity,
        ),
    )
    .expect("recovered cast lead and pure lead scrap must admit a balanced remelt");
    assert!(remelt.required_energy() > Energy::ZERO);
    assert_eq!(
        remelt.process_resolution().outputs()[0].commodity(),
        CommodityKey::new(MATERIAL_LEAD, FORM_MOLTEN),
    );
    assert_eq!(
        remelt.process_resolution().outputs()[0].mass().milligrams(),
        LEAD_MG + 5_000,
    );
    let duration = remelt.process_resolution().duration().value();
    validate_start_process(
        &registries,
        &state,
        remelt.process_resolution(),
        ingot_destination,
        lead_destination,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let persisted = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut continued = serde_json::from_slice::<LoadedSaveEnvelope>(&persisted)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for _ in 0..duration {
        let _ = advance_tick(&registries, &mut state).unwrap();
        let _ = advance_tick(&registries, &mut continued).unwrap();
    }
    assert_eq!(state, continued);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(lead_destination)
            .unwrap()
            .get_mass(CommodityKey::new(MATERIAL_LEAD, FORM_MOLTEN))
            .milligrams(),
        LEAD_MG + 5_000,
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        matter_before_remelt
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    // Complete molten -> ingot -> molten -> ingot recycling, releasing heat
    // into the same finite thermal sink under a fresh mold reservation.
    let liquid = state.inventory().lot_ids(lead_destination).next().unwrap();
    let recast = resolve_casting_process(
        &registries,
        &state,
        CastingRequest::new(
            PROCESS_CAST_LEAD_INGOT,
            lead_destination,
            &[MaterialLotSelection::new(
                liquid,
                Mass::from_milligrams(LEAD_MG + 5_000),
            )],
            mold,
            thermal_sink,
        ),
    )
    .unwrap();
    assert!(recast.released_energy() > Energy::ZERO);
    let cast_duration = recast.process_resolution().duration().value();
    validate_start_process(
        &registries,
        &state,
        recast.process_resolution(),
        lead_destination,
        ingot_destination,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let recast_saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut recast_loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&recast_saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for _ in 0..cast_duration {
        let _ = advance_tick(&registries, &mut state).unwrap();
        let _ = advance_tick(&registries, &mut recast_loaded).unwrap();
    }
    assert_eq!(state, recast_loaded);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(ingot_destination)
            .unwrap()
            .get_mass(CommodityKey::new(MATERIAL_LEAD, FORM_INGOT))
            .milligrams(),
        LEAD_MG + 5_000
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        matter_before_remelt
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
