//! Tests for the sibling comminution execution module; isolated so test-only edits do not invalidate production builds.

use super::*;
use crate::capability::{
    CapabilityComparison, CapabilityDefinition, CapabilityId, CapabilityProfile,
    CapabilityRequirement, CapabilityValueKind,
};
use crate::content::{
    FORM_CONCENTRATE, FORM_CRUSHED, FORM_INGOT, FORM_ORE, MATERIAL_COPPER, MATERIAL_SLAG,
    make_test_registries_with_comminution,
};
use crate::core::quantity::{AggregateMass, Length, MassSpecificEnergy};
use crate::core::state::{StateValidationError, validate_loaded_state};
use crate::core::time::{TickSpan, WorldSeed};
use crate::energy::{
    EnergyStoreDefinition, EnergyStoreDefinitionId, add_energy_store_with_initial_for_test,
};
use crate::equipment::{
    CapabilityConditionCurve, CapabilityConditionPoint, EquipmentDefinition, EquipmentDefinitionId,
    add_equipment,
};
use crate::inventory::{
    add_solid_stockpile_for_test, deposit_composed_lot_for_test, deposit_lot_spec_for_test,
};
use crate::maintenance::MaintenanceThresholds;
use crate::material::CompositionComponent;
use crate::matter::calculate_matter_accounting;
use crate::ore_processing::{ComminutionProcessDefinition, PoweredOreProcessProfile};
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::production::{ProcessDefinition, validate_start_process};
use crate::simulation::advance_tick;

const MASS_FLOW_CAPABILITY: CapabilityId = CapabilityId::new(970_001);
const MAX_BATCH_MASS_CAPABILITY: CapabilityId = CapabilityId::new(970_002);
const CRUSHER: EquipmentDefinitionId = EquipmentDefinitionId::new(970_001);
const ENERGY_STORE_DEFINITION: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(970_001);
const PROCESS: ProcessId = ProcessId::new(970_001);
const INPUT_TEMPERATURE: Temperature = Temperature::from_millikelvin(300_000);
const SPECIFIC_WORK: MassSpecificEnergy = MassSpecificEnergy::from_nanojoules_per_milligram(100);

fn crushed_particle_size() -> ParticleSizeRange {
    match ParticleSizeRange::new(
        Length::from_micrometers(1),
        Length::from_micrometers(20_000),
    ) {
        Ok(range) => range,
        Err(error) => panic!("comminution particle-size fixture failed: {error}"),
    }
}

fn ground_particle_size() -> ParticleSizeRange {
    match ParticleSizeRange::new(Length::from_micrometers(1), Length::from_micrometers(5_000)) {
        Ok(range) => range,
        Err(error) => panic!("grinding particle-size fixture failed: {error}"),
    }
}

fn selective_feed_particle_size() -> ParticleSizeRange {
    match ParticleSizeRange::new(
        Length::from_micrometers(5_001),
        Length::from_micrometers(20_000),
    ) {
        Ok(range) => range,
        Err(error) => panic!("selective grinding feed-size fixture failed: {error}"),
    }
}

fn condition(parts_per_million: u32) -> Condition {
    match Condition::new(parts_per_million) {
        Ok(condition) => condition,
        Err(error) => panic!("comminution condition fixture failed: {error}"),
    }
}

fn mixed_ore_composition() -> MaterialComposition {
    match MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, 400_000),
        CompositionComponent::new(MATERIAL_SLAG, 600_000),
    ]) {
        Ok(composition) => composition,
        Err(error) => panic!("comminution composition fixture failed: {error}"),
    }
}

fn make_registries_with_energy(carrier: EnergyCarrier, max_output_power: Power) -> Registries {
    make_registries_with_definition(
        carrier,
        max_output_power,
        ComminutionProcessDefinition::new(
            PROCESS,
            FORM_ORE,
            FORM_CRUSHED,
            crushed_particle_size(),
            PoweredOreProcessProfile::new(
                MASS_FLOW_CAPABILITY,
                MAX_BATCH_MASS_CAPABILITY,
                EnergyCarrier::Mechanical,
                SPECIFIC_WORK,
                1_000,
            ),
        ),
    )
}

fn make_registries_with_definition(
    carrier: EnergyCarrier,
    max_output_power: Power,
    comminution_definition: ComminutionProcessDefinition,
) -> Registries {
    let capabilities = match CapabilityProfile::new([
        (
            MASS_FLOW_CAPABILITY,
            CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(200)),
        ),
        (
            MAX_BATCH_MASS_CAPABILITY,
            CapabilityValue::Mass(Mass::from_milligrams(100)),
        ),
    ]) {
        Ok(profile) => profile,
        Err(error) => panic!("comminution capability fixture failed: {error}"),
    };
    let thresholds = match MaintenanceThresholds::new(condition(600_000), condition(250_000)) {
        Ok(thresholds) => thresholds,
        Err(error) => panic!("comminution maintenance fixture failed: {error}"),
    };
    let throughput_curve = CapabilityConditionCurve::new(
        MASS_FLOW_CAPABILITY,
        vec![CapabilityConditionPoint::new(
            Condition::FAILED,
            CapabilityValue::MassFlow(MassFlow::ZERO),
        )],
    );
    let equipment = EquipmentDefinition::new_with_capability_condition_curves(
        CRUSHER,
        "test jaw crusher",
        Mass::from_milligrams(1_000_000),
        capabilities,
        thresholds,
        vec![throughput_curve],
    );
    let process = ProcessDefinition::new_selected_batch(
        PROCESS,
        "test ore crushing",
        vec![CapabilityRequirement::new(
            MASS_FLOW_CAPABILITY,
            CapabilityComparison::AtLeast,
            CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(1)),
        )],
    );
    make_test_registries_with_comminution(
        vec![
            CapabilityDefinition::new(
                MASS_FLOW_CAPABILITY,
                "material mass throughput",
                CapabilityValueKind::MassFlow,
            ),
            CapabilityDefinition::new(
                MAX_BATCH_MASS_CAPABILITY,
                "maximum comminution batch mass",
                CapabilityValueKind::Mass,
            ),
        ],
        equipment,
        EnergyStoreDefinition::new(
            ENERGY_STORE_DEFINITION,
            "test crusher work buffer",
            carrier,
            Energy::from_nanojoules(1_000_000),
            max_output_power,
        ),
        process,
        comminution_definition,
    )
}

fn make_registries() -> Registries {
    make_registries_with_energy(EnergyCarrier::Mechanical, Power::from_microwatts(100))
}

#[test]
fn comminution_can_reduce_particle_size_without_relabeling_the_material_form() {
    let registries = make_registries_with_definition(
        EnergyCarrier::Mechanical,
        Power::from_microwatts(100),
        ComminutionProcessDefinition::new_with_input_particle_size_range(
            PROCESS,
            FORM_CRUSHED,
            FORM_CRUSHED,
            crushed_particle_size(),
            ground_particle_size(),
            PoweredOreProcessProfile::new(
                MASS_FLOW_CAPABILITY,
                MAX_BATCH_MASS_CAPABILITY,
                EnergyCarrier::Mechanical,
                SPECIFIC_WORK,
                1_000,
            ),
        ),
    );
    let mut state = AppState::new(WorldSeed::new(0x9700_0006));
    assert_eq!(
        registries
            .ore_processing()
            .get_comminution(PROCESS)
            .and_then(ComminutionProcessDefinition::input_particle_size_range),
        Some(crushed_particle_size())
    );
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(source) => source,
        Err(error) => panic!("grinding source fixture failed: {error}"),
    };
    let input = match MaterialLotSpec::with_composition_and_particle_size(
        CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED),
        Mass::from_milligrams(20),
        INPUT_TEMPERATURE,
        mixed_ore_composition(),
        crushed_particle_size(),
    ) {
        Ok(input) => input,
        Err(error) => panic!("grinding input specification failed: {error}"),
    };
    let lot = match deposit_lot_spec_for_test(&registries, &mut state, source, input) {
        Ok(lot) => lot,
        Err(error) => panic!("grinding input fixture failed: {error}"),
    };
    let equipment = match add_equipment(&registries, &mut state, CRUSHER, Condition::PRISTINE) {
        Ok(equipment) => equipment,
        Err(error) => panic!("grinding equipment fixture failed: {error}"),
    };
    let energy_store = match add_energy_store_with_initial_for_test(
        &registries,
        &mut state,
        ENERGY_STORE_DEFINITION,
        Energy::from_nanojoules(1_000_000),
    ) {
        Ok(energy_store) => energy_store,
        Err(error) => panic!("grinding energy fixture failed: {error}"),
    };

    let resolved = match resolve_comminution_process(
        &registries,
        &state,
        ComminutionRequest::new(
            PROCESS,
            source,
            &[MaterialLotSelection::new(lot, Mass::from_milligrams(20))],
            equipment,
            energy_store,
        ),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("same-form grinding resolution failed: {error}"),
    };
    let outputs = resolved.process_resolution().outputs();
    assert_eq!(outputs.len(), 1);
    assert_eq!(
        outputs[0].commodity(),
        CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED)
    );
    assert_eq!(outputs[0].particle_size(), Some(ground_particle_size()));
}

#[test]
fn constrained_comminution_rejects_out_of_range_feed_without_mutation() {
    let required = selective_feed_particle_size();
    let found = crushed_particle_size();
    let registries = make_registries_with_definition(
        EnergyCarrier::Mechanical,
        Power::from_microwatts(100),
        ComminutionProcessDefinition::new_with_input_particle_size_range(
            PROCESS,
            FORM_CRUSHED,
            FORM_CRUSHED,
            required,
            ground_particle_size(),
            PoweredOreProcessProfile::new(
                MASS_FLOW_CAPABILITY,
                MAX_BATCH_MASS_CAPABILITY,
                EnergyCarrier::Mechanical,
                SPECIFIC_WORK,
                1_000,
            ),
        ),
    );
    let mut state = AppState::new(WorldSeed::new(0x9700_0007));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("constrained grinding source failed: {error}"));
    let input = MaterialLotSpec::with_composition_and_particle_size(
        CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED),
        Mass::from_milligrams(20),
        INPUT_TEMPERATURE,
        mixed_ore_composition(),
        found,
    )
    .unwrap_or_else(|error| panic!("constrained grinding input failed: {error}"));
    let lot = deposit_lot_spec_for_test(&registries, &mut state, source, input)
        .unwrap_or_else(|error| panic!("constrained grinding lot seed failed: {error}"));
    let equipment = add_equipment(&registries, &mut state, CRUSHER, Condition::PRISTINE)
        .unwrap_or_else(|error| panic!("constrained grinding equipment failed: {error}"));
    let energy_store = add_energy_store_with_initial_for_test(
        &registries,
        &mut state,
        ENERGY_STORE_DEFINITION,
        Energy::from_nanojoules(1_000_000),
    )
    .unwrap_or_else(|error| panic!("constrained grinding energy failed: {error}"));
    let before = state.clone();

    match resolve_comminution_process(
        &registries,
        &state,
        ComminutionRequest::new(
            PROCESS,
            source,
            &[MaterialLotSelection::new(lot, Mass::from_milligrams(20))],
            equipment,
            energy_store,
        ),
    ) {
        Err(ComminutionResolutionError::Batch(
            ComminutionBatchError::InputParticleSizeOutsideOperatingRange {
                required: actual_required,
                found: actual_found,
            },
        )) => {
            assert_eq!(actual_required, required);
            assert_eq!(actual_found, found);
        }
        other => panic!("out-of-range constrained grinding returned {other:?}"),
    }
    assert_eq!(state, before);
}

#[test]
fn constrained_comminution_persistence_rejects_forged_feed_size_trace() {
    let required = selective_feed_particle_size();
    let registries = make_registries_with_definition(
        EnergyCarrier::Mechanical,
        Power::from_microwatts(100),
        ComminutionProcessDefinition::new_with_input_particle_size_range(
            PROCESS,
            FORM_CRUSHED,
            FORM_CRUSHED,
            required,
            ground_particle_size(),
            PoweredOreProcessProfile::new(
                MASS_FLOW_CAPABILITY,
                MAX_BATCH_MASS_CAPABILITY,
                EnergyCarrier::Mechanical,
                SPECIFIC_WORK,
                1_000,
            ),
        ),
    );
    let mut state = AppState::new(WorldSeed::new(0x9700_0008));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("constrained persistence source failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("constrained persistence destination failed: {error}"));
    let input = MaterialLotSpec::with_composition_and_particle_size(
        CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED),
        Mass::from_milligrams(20),
        INPUT_TEMPERATURE,
        mixed_ore_composition(),
        required,
    )
    .unwrap_or_else(|error| panic!("constrained persistence input failed: {error}"));
    let lot = deposit_lot_spec_for_test(&registries, &mut state, source, input)
        .unwrap_or_else(|error| panic!("constrained persistence lot seed failed: {error}"));
    let equipment = add_equipment(&registries, &mut state, CRUSHER, Condition::PRISTINE)
        .unwrap_or_else(|error| panic!("constrained persistence equipment failed: {error}"));
    let energy_store = add_energy_store_with_initial_for_test(
        &registries,
        &mut state,
        ENERGY_STORE_DEFINITION,
        Energy::from_nanojoules(1_000_000),
    )
    .unwrap_or_else(|error| panic!("constrained persistence energy failed: {error}"));
    let resolved = resolve_comminution_process(
        &registries,
        &state,
        ComminutionRequest::new(
            PROCESS,
            source,
            &[MaterialLotSelection::new(lot, Mass::from_milligrams(20))],
            equipment,
            energy_store,
        ),
    )
    .unwrap_or_else(|error| panic!("constrained persistence resolution failed: {error}"));
    let job = validate_start_process(
        &registries,
        &state,
        resolved.process_resolution(),
        source,
        destination,
    )
    .unwrap_or_else(|error| panic!("constrained persistence start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("constrained persistence commit failed: {error}"));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let mut tampered = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("constrained persistence serialization failed: {error}"));
    tampered["state"]["systems"]["production"]["jobs"][job.value().to_string()]["resources"]["consumed_inputs"]
        [0]["profile"]["particle_size"]["classes"][0]["range"]["minimum_diameter"] =
        serde_json::json!(1_u64);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(tampered)
        .unwrap_or_else(|error| panic!("constrained persistence decode failed: {error}"));
    let forged = ParticleSizeRange::new(
        Length::from_micrometers(1),
        Length::from_micrometers(20_000),
    )
    .unwrap_or_else(|error| panic!("forged feed-size fixture failed: {error}"));
    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::ComminutionJob(ComminutionJobValidationError::Batch {
                job,
                error: ComminutionBatchError::InputParticleSizeOutsideOperatingRange {
                    required,
                    found: forged,
                },
            })
        ))
    );
}

struct Fixture {
    registries: Registries,
    state: AppState,
    source: StockpileId,
    destination: StockpileId,
    lot: crate::inventory::MaterialLotId,
    equipment: EquipmentId,
    energy_store: EnergyStoreId,
}

fn make_fixture_with_registries(
    registries: Registries,
    seed: WorldSeed,
    input_mass: Mass,
    equipment_condition: Condition,
) -> Fixture {
    let mut state = AppState::new(seed);
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000)) {
        Ok(source) => source,
        Err(error) => panic!("comminution source fixture failed: {error}"),
    };
    let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000)) {
        Ok(destination) => destination,
        Err(error) => panic!("comminution destination fixture failed: {error}"),
    };
    let lot = match deposit_composed_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        input_mass,
        INPUT_TEMPERATURE,
        mixed_ore_composition(),
    ) {
        Ok(lot) => lot,
        Err(error) => panic!("comminution input fixture failed: {error}"),
    };
    let equipment = match add_equipment(&registries, &mut state, CRUSHER, equipment_condition) {
        Ok(equipment) => equipment,
        Err(error) => panic!("comminution equipment fixture failed: {error}"),
    };
    let energy_store = match add_energy_store_with_initial_for_test(
        &registries,
        &mut state,
        ENERGY_STORE_DEFINITION,
        Energy::from_nanojoules(1_000_000),
    ) {
        Ok(energy_store) => energy_store,
        Err(error) => panic!("comminution energy fixture failed: {error}"),
    };
    Fixture {
        registries,
        state,
        source,
        destination,
        lot,
        equipment,
        energy_store,
    }
}

fn make_fixture(seed: WorldSeed, input_mass: Mass, equipment_condition: Condition) -> Fixture {
    make_fixture_with_registries(make_registries(), seed, input_mass, equipment_condition)
}

fn matter_total(state: &AppState) -> AggregateMass {
    match calculate_matter_accounting(state) {
        Ok(accounting) => accounting.total(),
        Err(error) => panic!("comminution matter accounting failed: {error}"),
    }
}

fn resolve_mass(
    fixture: &Fixture,
    state: &AppState,
    mass: Mass,
) -> Result<ResolvedComminution, ComminutionResolutionError> {
    resolve_comminution_process(
        &fixture.registries,
        state,
        ComminutionRequest::new(
            PROCESS,
            fixture.source,
            &[MaterialLotSelection::new(fixture.lot, mass)],
            fixture.equipment,
            fixture.energy_store,
        ),
    )
}

fn finish_job(registries: &Registries, state: &mut AppState, duration: TickSpan) {
    for _ in 0..duration.value() {
        if let Err(error) = advance_tick(registries, state) {
            panic!("comminution completion tick failed: {error}");
        }
    }
}

#[test]
fn comminution_preserves_exact_mixed_profile_and_derates_throughput_with_wear() {
    let mut fixture = make_fixture(
        WorldSeed::new(0x9700_0001),
        Mass::from_milligrams(20),
        condition(500_000),
    );
    let initial_matter = matter_total(&fixture.state);
    let resolved = match resolve_mass(&fixture, &fixture.state, Mass::from_milligrams(20)) {
        Ok(resolved) => resolved,
        Err(error) => panic!("comminution resolution failed: {error}"),
    };
    assert_eq!(
        resolved.processing_rate(),
        MassFlow::from_milligrams_per_second(100)
    );
    assert_eq!(resolved.required_energy(), Energy::from_nanojoules(2_000));
    assert_eq!(resolved.available_power(), Power::from_microwatts(100));
    assert_eq!(resolved.condition_before(), condition(500_000));
    assert_eq!(resolved.condition_after(), condition(499_000));
    assert_eq!(resolved.process_resolution().duration(), TickSpan::new(1));
    assert_eq!(resolved.process_resolution().outputs().len(), 1);
    let output = &resolved.process_resolution().outputs()[0];
    assert_eq!(
        output.commodity(),
        CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED)
    );
    assert_eq!(output.mass(), Mass::from_milligrams(20));
    assert_eq!(output.temperature(), INPUT_TEMPERATURE);
    assert_eq!(output.composition(), &mixed_ore_composition());
    assert_eq!(output.particle_size(), Some(crushed_particle_size()));
    assert_eq!(
        resolved.process_resolution().equipment_condition_after(),
        Some(condition(499_000))
    );

    let duration = resolved.process_resolution().duration();
    let token = match validate_start_process(
        &fixture.registries,
        &fixture.state,
        resolved.process_resolution(),
        fixture.source,
        fixture.destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("comminution start validation failed: {error}"),
    };
    if let Err(error) = token.commit(&mut fixture.state) {
        panic!("comminution start commit failed: {error}");
    }
    assert_eq!(
        validate_loaded_state(&fixture.registries, &fixture.state),
        Ok(())
    );
    assert_eq!(matter_total(&fixture.state), initial_matter);
    finish_job(&fixture.registries, &mut fixture.state, duration);

    let output = match fixture
        .state
        .inventory()
        .lots()
        .find(|lot| lot.stockpile() == fixture.destination)
    {
        Some(output) => output,
        None => panic!("completed comminution output disappeared"),
    };
    assert_eq!(
        output.commodity(),
        CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED)
    );
    assert_eq!(output.composition(), &mixed_ore_composition());
    assert_eq!(output.particle_size(), Some(crushed_particle_size()));
    assert_eq!(matter_total(&fixture.state), initial_matter);
    assert_eq!(
        fixture
            .state
            .equipment()
            .get_equipment(fixture.equipment)
            .map(|record| record.condition()),
        Some(condition(499_000))
    );
}

#[test]
fn weak_energy_delivery_extends_active_time_and_equipment_wear() {
    let fixture = make_fixture_with_registries(
        make_registries_with_energy(EnergyCarrier::Mechanical, Power::from_picowatts(100_000)),
        WorldSeed::new(0x9700_0004),
        Mass::from_milligrams(20),
        Condition::PRISTINE,
    );
    let resolved = match resolve_mass(&fixture, &fixture.state, Mass::from_milligrams(20)) {
        Ok(resolved) => resolved,
        Err(error) => panic!("power-limited comminution resolution failed: {error}"),
    };

    assert_eq!(
        resolved.processing_rate(),
        MassFlow::from_milligrams_per_second(200)
    );
    assert_eq!(resolved.required_energy(), Energy::from_nanojoules(2_000));
    assert_eq!(resolved.available_power(), Power::from_picowatts(100_000));
    assert_eq!(resolved.condition_before(), Condition::PRISTINE);
    assert_eq!(resolved.throughput_duration(), TickSpan::new(1));
    assert_eq!(resolved.energy_duration(), TickSpan::new(6));
    assert_eq!(resolved.condition_after(), condition(994_000));
    assert_eq!(resolved.process_resolution().duration(), TickSpan::new(6));
    assert_eq!(
        resolved.process_resolution().equipment_condition_after(),
        Some(condition(994_000))
    );
}

#[test]
fn comminution_rejects_wrong_energy_carrier_without_mutation() {
    let fixture = make_fixture_with_registries(
        make_registries_with_energy(EnergyCarrier::Electrical, Power::from_microwatts(100)),
        WorldSeed::new(0x9700_0005),
        Mass::from_milligrams(20),
        Condition::PRISTINE,
    );
    let before = fixture.state.clone();

    assert!(matches!(
        resolve_mass(&fixture, &fixture.state, Mass::from_milligrams(20)),
        Err(ComminutionResolutionError::WrongEnergyCarrier {
            required: EnergyCarrier::Mechanical,
            provided: EnergyCarrier::Electrical,
        })
    ));
    assert_eq!(fixture.state, before);
}

#[test]
fn comminution_rejects_wrong_form_and_oversized_batch_without_mutation() {
    let registries = make_registries();
    let mut state = AppState::new(WorldSeed::new(0x9700_0002));
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(500)) {
        Ok(source) => source,
        Err(error) => panic!("comminution rejection source failed: {error}"),
    };
    let wrong_form_lot = match deposit_composed_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_COPPER, FORM_INGOT),
        Mass::from_milligrams(10),
        INPUT_TEMPERATURE,
        mixed_ore_composition(),
    ) {
        Ok(lot) => lot,
        Err(error) => panic!("wrong-form comminution fixture failed: {error}"),
    };
    let oversized_lot = match deposit_composed_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        Mass::from_milligrams(101),
        INPUT_TEMPERATURE,
        mixed_ore_composition(),
    ) {
        Ok(lot) => lot,
        Err(error) => panic!("oversized comminution fixture failed: {error}"),
    };
    let equipment = match add_equipment(&registries, &mut state, CRUSHER, Condition::PRISTINE) {
        Ok(equipment) => equipment,
        Err(error) => panic!("comminution rejection equipment failed: {error}"),
    };
    let energy_store = match add_energy_store_with_initial_for_test(
        &registries,
        &mut state,
        ENERGY_STORE_DEFINITION,
        Energy::from_nanojoules(1_000_000),
    ) {
        Ok(energy_store) => energy_store,
        Err(error) => panic!("comminution rejection energy fixture failed: {error}"),
    };
    let before = state.clone();

    assert!(matches!(
        resolve_comminution_process(
            &registries,
            &state,
            ComminutionRequest::new(
                PROCESS,
                source,
                &[MaterialLotSelection::new(
                    wrong_form_lot,
                    Mass::from_milligrams(10)
                )],
                equipment,
                energy_store,
            ),
        ),
        Err(ComminutionResolutionError::Batch(
            ComminutionBatchError::InputFormMismatch {
                expected: FORM_ORE,
                found: FORM_INGOT,
            }
        ))
    ));
    assert!(matches!(
        resolve_comminution_process(
            &registries,
            &state,
            ComminutionRequest::new(
                PROCESS,
                source,
                &[MaterialLotSelection::new(
                    oversized_lot,
                    Mass::from_milligrams(101),
                )],
                equipment,
                energy_store,
            ),
        ),
        Err(ComminutionResolutionError::BatchMassExceeded { selected, maximum })
            if selected == Mass::from_milligrams(101)
                && maximum == Mass::from_milligrams(100)
    ));
    assert_eq!(state, before);
}

#[test]
fn comminution_job_round_trip_revalidates_exact_outputs_and_continues() {
    let mut fixture = make_fixture(
        WorldSeed::new(0x9700_0003),
        Mass::from_milligrams(20),
        Condition::PRISTINE,
    );
    let resolved = match resolve_mass(&fixture, &fixture.state, Mass::from_milligrams(20)) {
        Ok(resolved) => resolved,
        Err(error) => panic!("round-trip comminution resolution failed: {error}"),
    };
    let required_energy = resolved.required_energy();
    let duration = resolved.process_resolution().duration();
    let token = match validate_start_process(
        &fixture.registries,
        &fixture.state,
        resolved.process_resolution(),
        fixture.source,
        fixture.destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("round-trip comminution start validation failed: {error}"),
    };
    let job = match token.commit(&mut fixture.state) {
        Ok(job) => job,
        Err(error) => panic!("round-trip comminution start failed: {error}"),
    };
    assert_eq!(
        validate_loaded_state(&fixture.registries, &fixture.state),
        Ok(())
    );

    let encoded = match serde_json::to_vec(&SaveEnvelope::new(&fixture.registries, &fixture.state))
    {
        Ok(encoded) => encoded,
        Err(error) => panic!("comminution save serialization failed: {error}"),
    };
    let decoded: LoadedSaveEnvelope = match serde_json::from_slice(&encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("comminution save decode failed: {error}"),
    };
    let mut loaded = match decoded.into_state(&fixture.registries) {
        Ok(loaded) => loaded,
        Err(error) => panic!("comminution save validation failed: {error}"),
    };
    assert_eq!(loaded, fixture.state);

    let mut tampered =
        match serde_json::to_value(SaveEnvelope::new(&fixture.registries, &fixture.state)) {
            Ok(encoded) => encoded,
            Err(error) => panic!("comminution tamper serialization failed: {error}"),
        };
    tampered["state"]["systems"]["production"]["jobs"][job.value().to_string()]["output_streams"]
        [0]["outputs"][0]["commodity"] =
        serde_json::json!(CommodityKey::new(MATERIAL_COPPER, FORM_CONCENTRATE).value());
    let tampered: LoadedSaveEnvelope = match serde_json::from_value(tampered) {
        Ok(decoded) => decoded,
        Err(error) => panic!("comminution tampered save decode failed: {error}"),
    };
    assert_eq!(
        tampered.into_state(&fixture.registries),
        Err(LoadError::InvalidState(
            StateValidationError::ComminutionJob(ComminutionJobValidationError::OutputMismatch {
                job
            })
        ))
    );

    let mut tampered_particle_size =
        match serde_json::to_value(SaveEnvelope::new(&fixture.registries, &fixture.state)) {
            Ok(encoded) => encoded,
            Err(error) => {
                panic!("comminution particle-size tamper serialization failed: {error}")
            }
        };
    tampered_particle_size["state"]["systems"]["production"]["jobs"][job.value().to_string()]["output_streams"]
        [0]["outputs"][0]["particle_size"]["classes"][0]["range"]["maximum_diameter"] =
        serde_json::json!(5_000_u64);
    let tampered_particle_size: LoadedSaveEnvelope =
        match serde_json::from_value(tampered_particle_size) {
            Ok(decoded) => decoded,
            Err(error) => panic!("comminution particle-size tamper failed decode: {error}"),
        };
    assert_eq!(
        tampered_particle_size.into_state(&fixture.registries),
        Err(LoadError::InvalidState(
            StateValidationError::ComminutionJob(ComminutionJobValidationError::OutputMismatch {
                job
            })
        ))
    );

    let mut tampered_energy =
        match serde_json::to_value(SaveEnvelope::new(&fixture.registries, &fixture.state)) {
            Ok(encoded) => encoded,
            Err(error) => panic!("comminution energy tamper serialization failed: {error}"),
        };
    tampered_energy["state"]["systems"]["production"]["jobs"][job.value().to_string()]["resources"]
        ["consumed_energy"]["energy"] = serde_json::json!(1_u64);
    let tampered_energy: LoadedSaveEnvelope = match serde_json::from_value(tampered_energy) {
        Ok(decoded) => decoded,
        Err(error) => panic!("comminution energy tamper failed decode: {error}"),
    };
    assert_eq!(
        tampered_energy.into_state(&fixture.registries),
        Err(LoadError::InvalidState(
            StateValidationError::ComminutionJob(ComminutionJobValidationError::EnergyMismatch {
                job,
                traced: Energy::from_nanojoules(1),
                required: required_energy,
            })
        ))
    );

    finish_job(&fixture.registries, &mut fixture.state, duration);
    finish_job(&fixture.registries, &mut loaded, duration);
    assert_eq!(loaded, fixture.state);
}

#[cfg(feature = "test-soak")]
fn run_comminution_soak(seed: WorldSeed) -> AppState {
    let fixture = make_fixture(seed, Mass::from_milligrams(300), Condition::PRISTINE);
    let initial_matter = matter_total(&fixture.state);
    let mut state = fixture.state.clone();
    for step in 0..300_u64 {
        let resolved = match resolve_mass(&fixture, &state, Mass::from_milligrams(1)) {
            Ok(resolved) => resolved,
            Err(error) => panic!("comminution soak resolution failed at step {step}: {error}"),
        };
        let duration = resolved.process_resolution().duration();
        let token = match validate_start_process(
            &fixture.registries,
            &state,
            resolved.process_resolution(),
            fixture.source,
            fixture.destination,
        ) {
            Ok(token) => token,
            Err(error) => panic!("comminution soak start failed at step {step}: {error}"),
        };
        if let Err(error) = token.commit(&mut state) {
            panic!("comminution soak commit failed at step {step}: {error}");
        }
        finish_job(&fixture.registries, &mut state, duration);
        if step.is_multiple_of(47) {
            assert_eq!(validate_loaded_state(&fixture.registries, &state), Ok(()));
            assert_eq!(matter_total(&state), initial_matter);
        }
    }
    assert_eq!(matter_total(&state), initial_matter);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(fixture.destination)
            .map(|stockpile| {
                stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED))
            }),
        Some(Mass::from_milligrams(300))
    );
    assert_eq!(
        state
            .energy()
            .get_store(fixture.energy_store)
            .map(|store| store.stored()),
        Some(Energy::from_nanojoules(970_000))
    );
    state
}

#[cfg(feature = "test-soak")]
#[test]
#[ignore = "long-horizon soak"]
fn comminution_soak_preserves_matter_and_deterministic_replay() {
    let seed = WorldSeed::new(0x9700_5000);
    let first = run_comminution_soak(seed);
    let second = run_comminution_soak(seed);
    assert_eq!(first, second);
}
