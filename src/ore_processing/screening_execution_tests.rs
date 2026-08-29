//! Contract tests for dry-screening execution and replay.

use super::*;
use crate::capability::{
    CapabilityComparison, CapabilityDefinition, CapabilityId, CapabilityProfile,
    CapabilityRequirement, CapabilityValue, CapabilityValueKind,
};
use crate::content::{
    FORM_CRUSHED, MATERIAL_COPPER, MATERIAL_SLAG, make_test_registries_with_screening,
};
use crate::core::quantity::{Length, MassSpecificEnergy};
use crate::core::state::{StateValidationError, validate_loaded_state};
use crate::core::time::WorldSeed;
use crate::energy::{
    EnergyStoreDefinition, EnergyStoreDefinitionId, add_energy_store_with_initial_for_fixture,
};
use crate::equipment::{EquipmentDefinition, EquipmentDefinitionId, add_equipment};
use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_spec_for_test};
use crate::maintenance::MaintenanceThresholds;
use crate::material::{CompositionComponent, ParticleSizeClass};
use crate::matter::calculate_matter_accounting;
use crate::ore_processing::{PoweredOreProcessProfile, ScreeningProcessDefinition};
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::production::{ProcessDefinition, ProcessOutputRoute, validate_start_process_routed};
use crate::simulation::advance_tick;

const FLOW_CAPABILITY: CapabilityId = CapabilityId::new(971_001);
const BATCH_CAPABILITY: CapabilityId = CapabilityId::new(971_002);
const SCREEN: EquipmentDefinitionId = EquipmentDefinitionId::new(971_001);
const ENERGY_STORE: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(971_001);
const PROCESS: ProcessId = ProcessId::new(971_001);
const TEMPERATURE: Temperature = Temperature::from_millikelvin(300_000);

fn distribution() -> ParticleSizeDistribution {
    let class = |minimum, maximum, weight| {
        let range = ParticleSizeRange::new(
            Length::from_micrometers(minimum),
            Length::from_micrometers(maximum),
        )
        .unwrap_or_else(|error| panic!("screening range fixture failed: {error}"));
        ParticleSizeClass::new(range, weight)
            .unwrap_or_else(|error| panic!("screening class fixture failed: {error}"))
    };
    ParticleSizeDistribution::new(vec![
        class(500, 2_000, 4),
        class(2_001, 5_000, 4),
        class(5_001, 10_000, 2),
    ])
    .unwrap_or_else(|error| panic!("screening distribution fixture failed: {error}"))
}

fn composition() -> MaterialComposition {
    MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, 400_000),
        CompositionComponent::new(MATERIAL_SLAG, 600_000),
    ])
    .unwrap_or_else(|error| panic!("screening composition fixture failed: {error}"))
}

fn registries_with_power(aperture: Length, max_output_power: Power) -> Registries {
    let capabilities = CapabilityProfile::new([
        (
            FLOW_CAPABILITY,
            CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(200)),
        ),
        (
            BATCH_CAPABILITY,
            CapabilityValue::Mass(Mass::from_milligrams(100)),
        ),
    ])
    .unwrap_or_else(|error| panic!("screening capability fixture failed: {error}"));
    let warning = Condition::new(600_000)
        .unwrap_or_else(|error| panic!("screening warning fixture failed: {error}"));
    let critical = Condition::new(250_000)
        .unwrap_or_else(|error| panic!("screening critical fixture failed: {error}"));
    let thresholds = MaintenanceThresholds::new(warning, critical)
        .unwrap_or_else(|error| panic!("screening maintenance fixture failed: {error}"));
    let equipment = EquipmentDefinition::new(
        SCREEN,
        "test dry screen",
        Mass::from_milligrams(1_000_000),
        capabilities,
        thresholds,
    );
    let process = ProcessDefinition::new_selected_batch(
        PROCESS,
        "test dry screening",
        vec![
            CapabilityRequirement::new(
                FLOW_CAPABILITY,
                CapabilityComparison::AtLeast,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(1)),
            ),
            CapabilityRequirement::new(
                BATCH_CAPABILITY,
                CapabilityComparison::AtLeast,
                CapabilityValue::Mass(Mass::from_milligrams(1)),
            ),
        ],
    );
    make_test_registries_with_screening(
        vec![
            CapabilityDefinition::new(
                FLOW_CAPABILITY,
                "screen material throughput",
                CapabilityValueKind::MassFlow,
            ),
            CapabilityDefinition::new(
                BATCH_CAPABILITY,
                "screen maximum batch mass",
                CapabilityValueKind::Mass,
            ),
        ],
        equipment,
        EnergyStoreDefinition::new_with_transfer_limits(
            ENERGY_STORE,
            "test screen mechanical buffer",
            EnergyCarrier::Mechanical,
            Energy::from_nanojoules(1_000_000),
            Power::ZERO,
            max_output_power,
        ),
        process,
        ScreeningProcessDefinition::new(
            PROCESS,
            FORM_CRUSHED,
            FORM_CRUSHED,
            aperture,
            PoweredOreProcessProfile::new(
                FLOW_CAPABILITY,
                BATCH_CAPABILITY,
                EnergyCarrier::Mechanical,
                MassSpecificEnergy::from_nanojoules_per_milligram(100),
                1_000,
            ),
        ),
    )
}

#[cfg(feature = "test-soak")]
fn registries(aperture: Length) -> Registries {
    registries_with_power(aperture, Power::from_microwatts(100))
}

struct Fixture {
    registries: Registries,
    state: AppState,
    source: StockpileId,
    lot: crate::inventory::MaterialLotId,
    equipment: EquipmentId,
    energy: EnergyStoreId,
}

fn fixture_with_power(aperture: Length, max_output_power: Power) -> Fixture {
    let registries = registries_with_power(aperture, max_output_power);
    let mut state = AppState::new(WorldSeed::new(0x9710_0001));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("screening source fixture failed: {error}"));
    let input = MaterialLotSpec::with_composition_and_particle_size(
        CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED),
        Mass::from_milligrams(10),
        TEMPERATURE,
        composition(),
        distribution(),
    )
    .unwrap_or_else(|error| panic!("screening input fixture failed: {error}"));
    let lot = deposit_lot_spec_for_test(&registries, &mut state, source, input)
        .unwrap_or_else(|error| panic!("screening lot fixture failed: {error}"));
    let equipment = add_equipment(&registries, &mut state, SCREEN, Condition::PRISTINE)
        .unwrap_or_else(|error| panic!("screening equipment fixture failed: {error}"));
    let energy = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        ENERGY_STORE,
        Energy::from_nanojoules(1_000_000),
    )
    .unwrap_or_else(|error| panic!("screening energy fixture failed: {error}"));
    Fixture {
        registries,
        state,
        source,
        lot,
        equipment,
        energy,
    }
}

fn fixture(aperture: Length) -> Fixture {
    fixture_with_power(aperture, Power::from_microwatts(100))
}

fn resolve(fixture: &Fixture) -> Result<ResolvedScreening, ScreeningResolutionError> {
    resolve_screening_process(
        &fixture.registries,
        &fixture.state,
        ScreeningRequest::new(
            PROCESS,
            fixture.source,
            &[MaterialLotSelection::new(
                fixture.lot,
                Mass::from_milligrams(10),
            )],
            fixture.equipment,
            fixture.energy,
        ),
    )
}

#[test]
fn screening_partitions_resolved_size_classes_without_changing_material_identity() {
    let fixture = fixture(Length::from_micrometers(2_000));
    let resolved =
        resolve(&fixture).unwrap_or_else(|error| panic!("screening resolution failed: {error}"));
    assert_eq!(resolved.undersize_mass(), Mass::from_milligrams(4));
    assert_eq!(resolved.oversize_mass(), Mass::from_milligrams(6));
    let streams = resolved.process_resolution().output_streams();
    assert_eq!(streams.len(), 2);
    assert_eq!(
        streams[0].id(),
        ScreeningProcessDefinition::UNDERSIZE_STREAM
    );
    assert_eq!(streams[1].id(), ScreeningProcessDefinition::OVERSIZE_STREAM);
    let fines = &streams[0].outputs()[0];
    let coarse = &streams[1].outputs()[0];
    assert_eq!(
        fines.commodity(),
        CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED)
    );
    assert_eq!(coarse.commodity(), fines.commodity());
    assert_eq!(fines.composition(), &composition());
    assert_eq!(coarse.composition(), &composition());
    assert_eq!(fines.temperature(), TEMPERATURE);
    assert_eq!(coarse.temperature(), TEMPERATURE);
    let fines_distribution = fines
        .particle_size_distribution()
        .unwrap_or_else(|| panic!("screening fines output lost particle-size state"));
    let coarse_distribution = coarse
        .particle_size_distribution()
        .unwrap_or_else(|| panic!("screening coarse output lost particle-size state"));
    assert_eq!(fines_distribution.classes().len(), 1);
    assert_eq!(coarse_distribution.classes().len(), 2);
}

#[test]
fn weak_screen_power_extends_active_time_and_equipment_wear() {
    let fixture = fixture_with_power(
        Length::from_micrometers(2_000),
        Power::from_picowatts(100_000),
    );
    let resolved = resolve(&fixture)
        .unwrap_or_else(|error| panic!("power-limited screening resolution failed: {error}"));

    assert_eq!(resolved.throughput_duration(), TickSpan::new(1));
    assert_eq!(resolved.energy_duration(), TickSpan::new(3));
    assert_eq!(resolved.process_resolution().duration(), TickSpan::new(3));
    assert_eq!(resolved.condition_before(), Condition::PRISTINE);
    assert_eq!(
        resolved.condition_after(),
        Condition::new(997_000)
            .unwrap_or_else(|error| panic!("screening wear fixture failed: {error}"))
    );
}

#[test]
fn screening_refuses_to_guess_yield_when_aperture_intersects_an_unresolved_class() {
    let fixture = fixture(Length::from_micrometers(3_000));
    assert!(matches!(
        resolve(&fixture),
        Err(ScreeningResolutionError::Batch(
            ScreeningBatchError::UnresolvedParticleClass {
                aperture: _aperture,
                class: _class,
            }
        ))
    ));
    assert_eq!(
        fixture
            .state
            .inventory()
            .get_stockpile(fixture.source)
            .unwrap_or_else(|| panic!("screening source stockpile disappeared"))
            .stored_mass(),
        Mass::from_milligrams(10)
    );
}

#[test]
fn screening_refuses_fractional_class_mass_at_current_mass_resolution() {
    let fixture = fixture(Length::from_micrometers(2_000));
    let source = fixture
        .state
        .inventory()
        .get_stockpile(fixture.source)
        .unwrap_or_else(|| panic!("screening source stockpile disappeared"));
    assert_eq!(source.stored_mass(), Mass::from_milligrams(10));

    let selections = [MaterialLotSelection::new(
        fixture.lot,
        Mass::from_milligrams(9),
    )];
    let request = ScreeningRequest::new(
        PROCESS,
        fixture.source,
        &selections,
        fixture.equipment,
        fixture.energy,
    );
    assert!(matches!(
        resolve_screening_process(&fixture.registries, &fixture.state, request),
        Err(ScreeningResolutionError::Batch(
            ScreeningBatchError::UnrepresentableClassMass {
                mass,
                undersize_weight: 2,
                total_weight: 5,
            }
        )) if mass == Mass::from_milligrams(9)
    ));
    assert_eq!(
        fixture
            .state
            .inventory()
            .get_stockpile(fixture.source)
            .unwrap_or_else(|| panic!("screening source stockpile disappeared"))
            .stored_mass(),
        Mass::from_milligrams(10)
    );
}

#[test]
fn routed_screening_completion_conserves_matter_and_validates_while_in_flight() {
    let mut fixture = fixture(Length::from_micrometers(2_000));
    let undersize = add_solid_stockpile_for_test(&mut fixture.state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("undersize destination fixture failed: {error}"));
    let oversize = add_solid_stockpile_for_test(&mut fixture.state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("oversize destination fixture failed: {error}"));
    let initial_matter = calculate_matter_accounting(&fixture.state)
        .unwrap_or_else(|error| panic!("screening matter accounting failed: {error}"))
        .total();
    let resolved =
        resolve(&fixture).unwrap_or_else(|error| panic!("screening resolution failed: {error}"));
    let duration = resolved.process_resolution().duration();
    let start = validate_start_process_routed(
        &fixture.registries,
        &fixture.state,
        resolved.process_resolution(),
        fixture.source,
        &[
            ProcessOutputRoute::new(ScreeningProcessDefinition::UNDERSIZE_STREAM, undersize),
            ProcessOutputRoute::new(ScreeningProcessDefinition::OVERSIZE_STREAM, oversize),
        ],
    )
    .unwrap_or_else(|error| panic!("screening start validation failed: {error}"));
    start
        .commit(&mut fixture.state)
        .unwrap_or_else(|error| panic!("screening start commit failed: {error}"));
    validate_loaded_state(&fixture.registries, &fixture.state)
        .unwrap_or_else(|error| panic!("in-flight screening state failed audit: {error}"));
    for _ in 0..duration.value() {
        let _ = advance_tick(&fixture.registries, &mut fixture.state)
            .unwrap_or_else(|error| panic!("screening completion tick failed: {error}"));
    }
    assert_eq!(
        fixture
            .state
            .inventory()
            .get_stockpile(undersize)
            .unwrap_or_else(|| panic!("screening undersize stockpile disappeared"))
            .stored_mass(),
        Mass::from_milligrams(4)
    );
    assert_eq!(
        fixture
            .state
            .inventory()
            .get_stockpile(oversize)
            .unwrap_or_else(|| panic!("screening oversize stockpile disappeared"))
            .stored_mass(),
        Mass::from_milligrams(6)
    );
    assert_eq!(
        calculate_matter_accounting(&fixture.state)
            .unwrap_or_else(|error| panic!("completed screening matter accounting failed: {error}"))
            .total(),
        initial_matter
    );
}

#[test]
fn screening_job_round_trip_rejects_tampered_output_distribution() {
    let mut fixture = fixture(Length::from_micrometers(2_000));
    let undersize = add_solid_stockpile_for_test(&mut fixture.state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("undersize destination fixture failed: {error}"));
    let oversize = add_solid_stockpile_for_test(&mut fixture.state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("oversize destination fixture failed: {error}"));
    let resolved =
        resolve(&fixture).unwrap_or_else(|error| panic!("screening resolution failed: {error}"));
    let start = validate_start_process_routed(
        &fixture.registries,
        &fixture.state,
        resolved.process_resolution(),
        fixture.source,
        &[
            ProcessOutputRoute::new(ScreeningProcessDefinition::UNDERSIZE_STREAM, undersize),
            ProcessOutputRoute::new(ScreeningProcessDefinition::OVERSIZE_STREAM, oversize),
        ],
    )
    .unwrap_or_else(|error| panic!("screening start validation failed: {error}"));
    let job = start
        .commit(&mut fixture.state)
        .unwrap_or_else(|error| panic!("screening start commit failed: {error}"));

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&fixture.registries, &fixture.state))
        .unwrap_or_else(|error| panic!("screening save serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("screening save decode failed: {error}"));
    let loaded = decoded
        .into_state(&fixture.registries)
        .unwrap_or_else(|error| panic!("screening save validation failed: {error}"));
    assert_eq!(loaded, fixture.state);

    let mut tampered = serde_json::to_value(SaveEnvelope::new(&fixture.registries, &fixture.state))
        .unwrap_or_else(|error| panic!("screening tamper serialization failed: {error}"));
    tampered["state"]["systems"]["production"]["jobs"][job.value().to_string()]["output_streams"]
        [0]["outputs"][0]["particle_size"]["classes"][0]["range"]["maximum_diameter"] =
        serde_json::json!(1_999_u64);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(tampered)
        .unwrap_or_else(|error| panic!("screening tampered save decode failed: {error}"));
    assert_eq!(
        tampered.into_state(&fixture.registries),
        Err(LoadError::InvalidState(StateValidationError::ScreeningJob(
            ScreeningJobValidationError::OutputMismatch { job }
        )))
    );
}

#[cfg(feature = "test-soak")]
fn run_screening_soak(seed: WorldSeed) -> AppState {
    const OPERATIONS: u64 = 300;
    const BATCH_MILLIGRAMS: u64 = 10;
    let registries = registries(Length::from_micrometers(2_000));
    let mut state = AppState::new(seed);
    let total_mass = Mass::from_milligrams(OPERATIONS * BATCH_MILLIGRAMS);
    let source = add_solid_stockpile_for_test(&mut state, total_mass)
        .unwrap_or_else(|error| panic!("screening soak source failed: {error}"));
    let undersize = add_solid_stockpile_for_test(&mut state, total_mass)
        .unwrap_or_else(|error| panic!("screening soak undersize storage failed: {error}"));
    let oversize = add_solid_stockpile_for_test(&mut state, total_mass)
        .unwrap_or_else(|error| panic!("screening soak oversize storage failed: {error}"));
    let input = MaterialLotSpec::with_composition_and_particle_size(
        CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED),
        total_mass,
        TEMPERATURE,
        composition(),
        distribution(),
    )
    .unwrap_or_else(|error| panic!("screening soak input failed: {error}"));
    let lot = deposit_lot_spec_for_test(&registries, &mut state, source, input)
        .unwrap_or_else(|error| panic!("screening soak lot seed failed: {error}"));
    let equipment = add_equipment(&registries, &mut state, SCREEN, Condition::PRISTINE)
        .unwrap_or_else(|error| panic!("screening soak equipment failed: {error}"));
    let energy = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        ENERGY_STORE,
        Energy::from_nanojoules(1_000_000),
    )
    .unwrap_or_else(|error| panic!("screening soak energy failed: {error}"));
    let initial_matter = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("screening soak matter accounting failed: {error}"))
        .total();

    for operation in 0..OPERATIONS {
        let selection = [MaterialLotSelection::new(
            lot,
            Mass::from_milligrams(BATCH_MILLIGRAMS),
        )];
        let resolved = resolve_screening_process(
            &registries,
            &state,
            ScreeningRequest::new(PROCESS, source, &selection, equipment, energy),
        )
        .unwrap_or_else(|error| panic!("screening soak resolution failed: {error}"));
        assert_eq!(resolved.undersize_mass(), Mass::from_milligrams(4));
        assert_eq!(resolved.oversize_mass(), Mass::from_milligrams(6));
        assert_eq!(resolved.process_resolution().duration(), TickSpan::new(1));
        let start = validate_start_process_routed(
            &registries,
            &state,
            resolved.process_resolution(),
            source,
            &[
                ProcessOutputRoute::new(ScreeningProcessDefinition::UNDERSIZE_STREAM, undersize),
                ProcessOutputRoute::new(ScreeningProcessDefinition::OVERSIZE_STREAM, oversize),
            ],
        )
        .unwrap_or_else(|error| panic!("screening soak start failed: {error}"));
        start
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("screening soak commit failed: {error}"));

        if operation == OPERATIONS / 2 {
            let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
                .unwrap_or_else(|error| panic!("screening soak serialization failed: {error}"));
            let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
                .unwrap_or_else(|error| panic!("screening soak decode failed: {error}"));
            state = decoded
                .into_state(&registries)
                .unwrap_or_else(|error| panic!("screening soak resume failed: {error}"));
        }

        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("screening soak completion failed: {error}"));
        if operation % 25 == 0 {
            validate_loaded_state(&registries, &state)
                .unwrap_or_else(|error| panic!("screening soak audit failed: {error}"));
        }
    }

    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("screening soak final audit failed: {error}"));
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("screening soak final matter failed: {error}"))
            .total(),
        initial_matter
    );
    assert_eq!(
        state
            .energy()
            .get_store(energy)
            .unwrap_or_else(|| panic!("screening soak energy store disappeared"))
            .stored(),
        Energy::from_nanojoules(700_000)
    );
    assert_eq!(
        state
            .equipment()
            .get_equipment(equipment)
            .unwrap_or_else(|| panic!("screening soak equipment disappeared"))
            .condition(),
        Condition::new(700_000)
            .unwrap_or_else(|error| panic!("screening soak final condition failed: {error}"))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(undersize)
            .unwrap_or_else(|| panic!("screening soak undersize storage disappeared"))
            .stored_mass(),
        Mass::from_milligrams(1_200)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(oversize)
            .unwrap_or_else(|| panic!("screening soak oversize storage disappeared"))
            .stored_mass(),
        Mass::from_milligrams(1_800)
    );
    assert_eq!(state.inventory().lot_ids(undersize).count(), 1);
    assert_eq!(state.inventory().lot_ids(oversize).count(), 1);
    state
}

#[cfg(feature = "test-soak")]
#[test]
#[ignore = "long-horizon soak"]
fn screening_soak_preserves_conservation_persistence_and_replay() {
    let seed = WorldSeed::new(0x9710_50A5);
    let first = run_screening_soak(seed);
    let second = run_screening_soak(seed);
    assert_eq!(first, second);
}
