//! Tests for the sibling melting execution module; isolated so test-only edits do not invalidate production builds.

use super::*;
use crate::capability::{
    CapabilityComparison, CapabilityDefinition, CapabilityProfile, CapabilityRequirement,
    CapabilityValue, CapabilityValueKind,
};
use crate::content::{
    FORM_CONCENTRATE, FORM_INGOT, FORM_MOLTEN, MATERIAL_COPPER, MATERIAL_SLAG,
    make_test_registries_with_melting,
};
use crate::core::quantity::Length;
use crate::core::state::{StateValidationError, validate_loaded_state};
use crate::core::time::WorldSeed;
use crate::energy::{
    EnergyStoreDefinition, EnergyStoreDefinitionId, add_energy_store_with_initial_for_fixture,
    calculate_explicit_energy_accounting,
};
use crate::equipment::{EquipmentDefinition, EquipmentDefinitionId, add_equipment};
use crate::inventory::{
    StockpileStorageError, StockpileStorageProfile, add_solid_stockpile_for_test, add_stockpile,
    deposit_composed_lot_for_test, deposit_lot_for_test, deposit_lot_spec_for_test,
};
use crate::maintenance::MaintenanceThresholds;
use crate::material::{
    CompositionComponent, MaterialComposition, MaterialLotSpec, ParticleSizeRange,
};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::production::{ProcessDefinition, StartProcessError, validate_start_process};
use crate::simulation::advance_tick;
use crate::thermal::ThermalJobValidationError;

const HEATING_POWER: CapabilityId = CapabilityId::new(950_001);
const MAX_TEMPERATURE: CapabilityId = CapabilityId::new(950_002);
const MAX_BATCH_MASS: CapabilityId = CapabilityId::new(950_003);
const FURNACE: EquipmentDefinitionId = EquipmentDefinitionId::new(950_001);
const ENERGY_STORE: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(950_001);
const PROCESS: ProcessId = ProcessId::new(950_001);
const COPPER_MELTING_POINT: Temperature = Temperature::from_millikelvin(1_357_770);
const INPUT_TEMPERATURE: Temperature = Temperature::from_millikelvin(300_000);

#[derive(Clone, Copy)]
struct FixtureIds {
    source: StockpileId,
    destination: StockpileId,
    equipment: EquipmentId,
    energy_store: EnergyStoreId,
    source_lot: crate::inventory::MaterialLotId,
}

struct MeltingFixture {
    registries: Registries,
    state: AppState,
    ids: FixtureIds,
}

fn condition(parts_per_million: u32) -> Condition {
    match Condition::new(parts_per_million) {
        Ok(condition) => condition,
        Err(error) => panic!("melting condition fixture failed: {error}"),
    }
}

fn make_registries(maximum_temperature: Temperature, carrier: EnergyCarrier) -> Registries {
    let profile = match CapabilityProfile::new([
        (
            HEATING_POWER,
            CapabilityValue::Power(Power::from_microwatts(20_000_000)),
        ),
        (
            MAX_TEMPERATURE,
            CapabilityValue::Temperature(maximum_temperature),
        ),
        (
            MAX_BATCH_MASS,
            CapabilityValue::Mass(Mass::from_milligrams(20)),
        ),
    ]) {
        Ok(profile) => profile,
        Err(error) => panic!("melting capability profile failed: {error}"),
    };
    let thresholds = match MaintenanceThresholds::new(condition(600_000), condition(250_000)) {
        Ok(thresholds) => thresholds,
        Err(error) => panic!("melting maintenance fixture failed: {error}"),
    };
    let equipment = EquipmentDefinition::new(
        FURNACE,
        "test induction furnace",
        Mass::from_milligrams(2_000_000),
        profile,
        thresholds,
    );
    let energy = EnergyStoreDefinition::new_with_transfer_limits(
        ENERGY_STORE,
        "test melting electrical buffer",
        carrier,
        Energy::from_nanojoules(2_000_000_000_000),
        Power::ZERO,
        Power::from_microwatts(10_000_000),
    );
    let process = ProcessDefinition::new_selected_batch(
        PROCESS,
        "pure material melting",
        vec![
            CapabilityRequirement::new(
                HEATING_POWER,
                CapabilityComparison::AtLeast,
                CapabilityValue::Power(Power::from_microwatts(1_000_000)),
            ),
            CapabilityRequirement::new(
                MAX_TEMPERATURE,
                CapabilityComparison::AtLeast,
                CapabilityValue::Temperature(Temperature::from_millikelvin(1_200_000)),
            ),
            CapabilityRequirement::new(
                MAX_BATCH_MASS,
                CapabilityComparison::AtLeast,
                CapabilityValue::Mass(Mass::from_milligrams(1)),
            ),
        ],
    );
    make_test_registries_with_melting(
        vec![
            CapabilityDefinition::new(
                HEATING_POWER,
                "melting heating power",
                CapabilityValueKind::Power,
            ),
            CapabilityDefinition::new(
                MAX_TEMPERATURE,
                "melting maximum temperature",
                CapabilityValueKind::Temperature,
            ),
            CapabilityDefinition::new(
                MAX_BATCH_MASS,
                "melting maximum batch mass",
                CapabilityValueKind::Mass,
            ),
        ],
        equipment,
        energy,
        process,
        MeltingProcessDefinition::new(
            PROCESS,
            HEATING_POWER,
            MAX_TEMPERATURE,
            MAX_BATCH_MASS,
            EnergyCarrier::Electrical,
            PhaseChangeForms::new(FORM_INGOT, FORM_MOLTEN),
            10,
        ),
    )
}

#[test]
fn melting_rejects_pure_concentrate_without_a_reduction_step() {
    let mut fixture = make_fixture(
        Temperature::from_millikelvin(1_500_000),
        EnergyCarrier::Electrical,
        Mass::from_milligrams(10),
    );
    let concentrate_particle_size = ParticleSizeRange::new(
        Length::from_micrometers(500),
        Length::from_micrometers(2_000),
    )
    .unwrap_or_else(|error| panic!("concentrate particle-size fixture failed: {error}"));
    let concentrate = deposit_lot_spec_for_test(
        &fixture.registries,
        &mut fixture.state,
        fixture.ids.source,
        MaterialLotSpec::with_composition_and_particle_size(
            CommodityKey::new(MATERIAL_COPPER, FORM_CONCENTRATE),
            Mass::from_milligrams(5),
            INPUT_TEMPERATURE,
            MaterialComposition::pure(MATERIAL_COPPER),
            concentrate_particle_size,
        )
        .unwrap_or_else(|error| panic!("pure concentrate lot specification failed: {error}")),
    )
    .unwrap_or_else(|error| panic!("pure concentrate melting fixture failed: {error}"));
    let before = fixture.state.clone();

    assert_eq!(
        resolve_melting_process(
            &fixture.registries,
            &fixture.state,
            MeltingRequest::new(
                PROCESS,
                fixture.ids.source,
                &[MaterialLotSelection::new(
                    concentrate,
                    Mass::from_milligrams(5),
                )],
                fixture.ids.equipment,
                fixture.ids.energy_store,
            ),
        ),
        Err(MeltingResolutionError::Batch(
            MeltingBatchError::InputFormMismatch {
                expected: FORM_INGOT,
                found: FORM_CONCENTRATE,
            }
        ))
    );
    assert_eq!(fixture.state, before);
}

fn make_fixture(
    maximum_temperature: Temperature,
    carrier: EnergyCarrier,
    input_mass: Mass,
) -> MeltingFixture {
    let registries = make_registries(maximum_temperature, carrier);
    let mut state = AppState::new(WorldSeed::new(0x9500_0001));
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000)) {
        Ok(source) => source,
        Err(error) => panic!("melting source fixture failed: {error}"),
    };
    let vessel_profile =
        match StockpileStorageProfile::new(false, true, Temperature::from_millikelvin(1_500_000)) {
            Ok(profile) => profile,
            Err(error) => panic!("melting vessel profile failed: {error}"),
        };
    let destination = match add_stockpile(&mut state, Mass::from_milligrams(1_000), vessel_profile)
    {
        Ok(destination) => destination,
        Err(error) => panic!("melting vessel fixture failed: {error}"),
    };
    let source_lot = match deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_COPPER, FORM_INGOT),
        input_mass,
        INPUT_TEMPERATURE,
    ) {
        Ok(lot) => lot,
        Err(error) => panic!("melting copper fixture failed: {error}"),
    };
    let equipment = match add_equipment(&registries, &mut state, FURNACE, Condition::PRISTINE) {
        Ok(equipment) => equipment,
        Err(error) => panic!("melting equipment fixture failed: {error}"),
    };
    let energy_store = match add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        ENERGY_STORE,
        Energy::from_nanojoules(1_000_000_000_000),
    ) {
        Ok(store) => store,
        Err(error) => panic!("melting energy fixture failed: {error}"),
    };
    MeltingFixture {
        registries,
        state,
        ids: FixtureIds {
            source,
            destination,
            equipment,
            energy_store,
            source_lot,
        },
    }
}

fn resolve_selected(
    registries: &Registries,
    state: &AppState,
    ids: FixtureIds,
    mass: Mass,
) -> Result<ResolvedMelting, MeltingResolutionError> {
    resolve_melting_process(
        registries,
        state,
        MeltingRequest::new(
            PROCESS,
            ids.source,
            &[MaterialLotSelection::new(ids.source_lot, mass)],
            ids.equipment,
            ids.energy_store,
        ),
    )
}

fn explicit_energy_total(registries: &Registries, state: &AppState) -> Energy {
    match calculate_explicit_energy_accounting(registries, state).and_then(|accounting| {
        accounting
            .total()
            .ok_or(crate::energy::ExplicitEnergyAccountingError::Overflow)
    }) {
        Ok(total) => total,
        Err(error) => panic!("explicit energy accounting failed: {error}"),
    }
}

fn matter_total(state: &AppState) -> crate::core::quantity::AggregateMass {
    match calculate_matter_accounting(state) {
        Ok(accounting) => accounting.total(),
        Err(error) => panic!("matter accounting failed: {error}"),
    }
}

#[cfg(feature = "test-soak")]
fn commit_one_melt(
    registries: &Registries,
    state: &mut AppState,
    ids: FixtureIds,
    mass: Mass,
) -> TickSpan {
    let resolved = match resolve_selected(registries, state, ids, mass) {
        Ok(resolved) => resolved,
        Err(error) => panic!("melting resolution failed: {error}"),
    };
    let duration = resolved.process_resolution().duration();
    let token = match validate_start_process(
        registries,
        state,
        resolved.process_resolution(),
        ids.source,
        ids.destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("melting start validation failed: {error}"),
    };
    if let Err(error) = token.commit(state) {
        panic!("melting start commit failed: {error}");
    }
    for _ in 0..duration.value() {
        if let Err(error) = advance_tick(registries, state) {
            panic!("melting completion tick failed: {error}");
        }
    }
    duration
}

#[test]
fn melting_resolves_exact_sensible_plus_latent_energy_and_molten_output() {
    let fixture = make_fixture(
        Temperature::from_millikelvin(1_500_000),
        EnergyCarrier::Electrical,
        Mass::from_milligrams(10),
    );
    let sensible = match calculate_sensible_heat(
        fixture.registries.materials(),
        Mass::from_milligrams(10),
        &MaterialComposition::pure(MATERIAL_COPPER),
        INPUT_TEMPERATURE,
        COPPER_MELTING_POINT,
    ) {
        Ok(heat) => heat.energy(),
        Err(error) => panic!("melting sensible fixture failed: {error}"),
    };
    let latent = match calculate_fusion_heat(
        fixture.registries.materials(),
        Mass::from_milligrams(10),
        MATERIAL_COPPER,
    ) {
        Ok(heat) => heat.energy(),
        Err(error) => panic!("melting latent fixture failed: {error}"),
    };
    let expected_energy = match sensible.checked_add(latent) {
        Some(energy) => energy,
        None => panic!("melting expected energy overflowed"),
    };

    let resolved = match resolve_selected(
        &fixture.registries,
        &fixture.state,
        fixture.ids,
        Mass::from_milligrams(10),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("melting resolution failed: {error}"),
    };

    assert_eq!(resolved.material(), MATERIAL_COPPER);
    assert_eq!(resolved.melting_point(), COPPER_MELTING_POINT);
    assert_eq!(resolved.required_energy(), expected_energy);
    assert_eq!(
        resolved.transfer_power(),
        Power::from_microwatts(10_000_000)
    );
    assert_eq!(resolved.process_resolution().outputs().len(), 1);
    let output = &resolved.process_resolution().outputs()[0];
    assert_eq!(
        output.commodity(),
        CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN)
    );
    assert_eq!(output.mass(), Mass::from_milligrams(10));
    assert_eq!(output.temperature(), COPPER_MELTING_POINT);
    assert_eq!(
        output.composition(),
        &MaterialComposition::pure(MATERIAL_COPPER)
    );
}

#[test]
fn melting_requires_liquid_capable_destination_storage() {
    let fixture = make_fixture(
        Temperature::from_millikelvin(1_500_000),
        EnergyCarrier::Electrical,
        Mass::from_milligrams(10),
    );
    let mut state = fixture.state;
    let bad_destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
    {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("solid destination fixture failed: {error}"),
    };
    let resolved = match resolve_selected(
        &fixture.registries,
        &state,
        fixture.ids,
        Mass::from_milligrams(10),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("melting resolution failed: {error}"),
    };
    let before = state.clone();

    assert_eq!(
        validate_start_process(
            &fixture.registries,
            &state,
            resolved.process_resolution(),
            fixture.ids.source,
            bad_destination,
        ),
        Err(StartProcessError::DestinationStorage(
            StockpileStorageError::PhaseNotAccepted {
                stockpile: bad_destination,
                phase: MaterialPhase::Liquid,
            }
        ))
    );
    assert_eq!(state, before);
}

#[test]
fn melting_preserves_matter_and_modeled_energy_through_save_resume_and_completion() {
    let mut fixture = make_fixture(
        Temperature::from_millikelvin(1_500_000),
        EnergyCarrier::Electrical,
        Mass::from_milligrams(10),
    );
    let initial_matter = matter_total(&fixture.state);
    let initial_energy = explicit_energy_total(&fixture.registries, &fixture.state);
    let resolved = match resolve_selected(
        &fixture.registries,
        &fixture.state,
        fixture.ids,
        Mass::from_milligrams(10),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("melting resolution failed: {error}"),
    };
    let duration = resolved.process_resolution().duration();
    let token = match validate_start_process(
        &fixture.registries,
        &fixture.state,
        resolved.process_resolution(),
        fixture.ids.source,
        fixture.ids.destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("melting start validation failed: {error}"),
    };
    let job = match token.commit(&mut fixture.state) {
        Ok(job) => job,
        Err(error) => panic!("melting start commit failed: {error}"),
    };
    assert_eq!(matter_total(&fixture.state), initial_matter);
    assert_eq!(
        explicit_energy_total(&fixture.registries, &fixture.state),
        initial_energy
    );
    assert_eq!(
        validate_loaded_state(&fixture.registries, &fixture.state),
        Ok(())
    );

    let encoded = match serde_json::to_vec(&SaveEnvelope::new(&fixture.registries, &fixture.state))
    {
        Ok(encoded) => encoded,
        Err(error) => panic!("melting save serialization failed: {error}"),
    };
    let decoded: LoadedSaveEnvelope = match serde_json::from_slice(&encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("melting save deserialization failed: {error}"),
    };
    let mut resumed = match decoded.into_state(&fixture.registries) {
        Ok(state) => state,
        Err(error) => panic!("melting save validation failed: {error}"),
    };
    let mut uninterrupted = fixture.state;
    assert_eq!(resumed, uninterrupted);

    for _ in 0..duration.value() {
        let first = match advance_tick(&fixture.registries, &mut uninterrupted) {
            Ok(outcome) => outcome,
            Err(error) => panic!("uninterrupted melting continuation failed: {error}"),
        };
        let second = match advance_tick(&fixture.registries, &mut resumed) {
            Ok(outcome) => outcome,
            Err(error) => panic!("resumed melting continuation failed: {error}"),
        };
        assert_eq!(first, second);
    }
    assert_eq!(resumed, uninterrupted);
    assert!(resumed.production().get_job(job).is_none());
    assert_eq!(matter_total(&resumed), initial_matter);
    assert_eq!(
        explicit_energy_total(&fixture.registries, &resumed),
        initial_energy
    );
    let output = match resumed
        .inventory()
        .lots()
        .find(|lot| lot.stockpile() == fixture.ids.destination)
    {
        Some(output) => output,
        None => panic!("molten output lot missing after completion"),
    };
    assert_eq!(
        output.commodity(),
        CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN)
    );
    assert_eq!(output.mass(), Mass::from_milligrams(10));
    assert_eq!(output.temperature(), COPPER_MELTING_POINT);
}

#[test]
fn melting_rejects_impure_input_and_insufficient_furnace_temperature() {
    let mut fixture = make_fixture(
        Temperature::from_millikelvin(1_500_000),
        EnergyCarrier::Electrical,
        Mass::from_milligrams(10),
    );
    let mixed = match MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, 500_000),
        CompositionComponent::new(MATERIAL_SLAG, 500_000),
    ]) {
        Ok(composition) => composition,
        Err(error) => panic!("mixed melting fixture failed: {error}"),
    };
    let mixed_lot = match deposit_composed_lot_for_test(
        &fixture.registries,
        &mut fixture.state,
        fixture.ids.source,
        CommodityKey::new(MATERIAL_COPPER, FORM_INGOT),
        Mass::from_milligrams(5),
        INPUT_TEMPERATURE,
        mixed,
    ) {
        Ok(lot) => lot,
        Err(error) => panic!("mixed melting lot fixture failed: {error}"),
    };
    assert!(matches!(
        resolve_melting_process(
            &fixture.registries,
            &fixture.state,
            MeltingRequest::new(
                PROCESS,
                fixture.ids.source,
                &[MaterialLotSelection::new(
                    mixed_lot,
                    Mass::from_milligrams(5)
                )],
                fixture.ids.equipment,
                fixture.ids.energy_store,
            ),
        ),
        Err(MeltingResolutionError::Batch(
            MeltingBatchError::ImpureInput {
                commodity: _commodity,
            }
        ))
    ));

    let cool_fixture = make_fixture(
        Temperature::from_millikelvin(1_300_000),
        EnergyCarrier::Electrical,
        Mass::from_milligrams(10),
    );
    assert_eq!(
        resolve_selected(
            &cool_fixture.registries,
            &cool_fixture.state,
            cool_fixture.ids,
            Mass::from_milligrams(10),
        ),
        Err(
            MeltingResolutionError::MeltingPointExceedsEquipmentMaximum {
                melting_point: COPPER_MELTING_POINT,
                maximum: Temperature::from_millikelvin(1_300_000),
            }
        )
    );
}

#[test]
fn melting_job_tampering_is_rejected_by_physics_and_destination_audits() {
    let mut fixture = make_fixture(
        Temperature::from_millikelvin(1_500_000),
        EnergyCarrier::Electrical,
        Mass::from_milligrams(10),
    );
    let resolved = match resolve_selected(
        &fixture.registries,
        &fixture.state,
        fixture.ids,
        Mass::from_milligrams(10),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("melting resolution failed: {error}"),
    };
    let required_energy = resolved.required_energy();
    let token = match validate_start_process(
        &fixture.registries,
        &fixture.state,
        resolved.process_resolution(),
        fixture.ids.source,
        fixture.ids.destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("melting start validation failed: {error}"),
    };
    let job = match token.commit(&mut fixture.state) {
        Ok(job) => job,
        Err(error) => panic!("melting start commit failed: {error}"),
    };

    let mut tampered_energy =
        match serde_json::to_value(SaveEnvelope::new(&fixture.registries, &fixture.state)) {
            Ok(encoded) => encoded,
            Err(error) => panic!("melting energy tamper serialization failed: {error}"),
        };
    tampered_energy["state"]["systems"]["production"]["jobs"][job.value().to_string()]["resources"]
        ["consumed_energy"]["energy"] = serde_json::json!(1_u64);
    let tampered_energy: LoadedSaveEnvelope = match serde_json::from_value(tampered_energy) {
        Ok(decoded) => decoded,
        Err(error) => panic!("melting energy tamper failed decode: {error}"),
    };
    assert_eq!(
        tampered_energy.into_state(&fixture.registries),
        Err(LoadError::InvalidState(StateValidationError::ThermalJob(
            ThermalJobValidationError::Melting(MeltingJobValidationError::EnergyMismatch {
                job,
                traced: Energy::from_nanojoules(1),
                required: required_energy,
            })
        )))
    );

    let mut tampered_input =
        match serde_json::to_value(SaveEnvelope::new(&fixture.registries, &fixture.state)) {
            Ok(encoded) => encoded,
            Err(error) => panic!("melting input-form tamper serialization failed: {error}"),
        };
    tampered_input["state"]["systems"]["production"]["jobs"][job.value().to_string()]["resources"]
        ["consumed_inputs"][0]["profile"]["commodity"] =
        serde_json::json!(CommodityKey::new(MATERIAL_COPPER, FORM_CONCENTRATE).value());
    let tampered_input: LoadedSaveEnvelope = match serde_json::from_value(tampered_input) {
        Ok(decoded) => decoded,
        Err(error) => panic!("melting input-form tamper decode failed: {error}"),
    };
    assert_eq!(
        tampered_input.into_state(&fixture.registries),
        Err(LoadError::InvalidState(StateValidationError::ThermalJob(
            ThermalJobValidationError::Melting(MeltingJobValidationError::Batch {
                job,
                error: MeltingBatchError::InputFormMismatch {
                    expected: FORM_INGOT,
                    found: FORM_CONCENTRATE,
                },
            })
        )))
    );

    let mut invalid_destination =
        match serde_json::to_value(SaveEnvelope::new(&fixture.registries, &fixture.state)) {
            Ok(encoded) => encoded,
            Err(error) => panic!("melting destination tamper serialization failed: {error}"),
        };
    let destination = fixture.ids.destination.value().to_string();
    invalid_destination["state"]["systems"]["inventory"]["stockpiles"][destination.clone()]["storage_profile"]
        ["can_store_liquid"] = serde_json::json!(false);
    invalid_destination["state"]["systems"]["inventory"]["stockpiles"][destination]["storage_profile"]
        ["can_store_solid"] = serde_json::json!(true);
    let invalid_destination: LoadedSaveEnvelope = match serde_json::from_value(invalid_destination)
    {
        Ok(decoded) => decoded,
        Err(error) => panic!("melting destination tamper failed decode: {error}"),
    };
    assert_eq!(
        invalid_destination.into_state(&fixture.registries),
        Err(LoadError::InvalidState(
            StateValidationError::JobOutputStorage {
                job,
                error: StockpileStorageError::PhaseNotAccepted {
                    stockpile: fixture.ids.destination,
                    phase: MaterialPhase::Liquid,
                },
            }
        ))
    );
}

#[cfg(feature = "test-soak")]
#[test]
#[ignore = "long-horizon soak"]
fn small_melt_soak_preserves_conservation_and_deterministic_replay() {
    let fixture = make_fixture(
        Temperature::from_millikelvin(1_500_000),
        EnergyCarrier::Electrical,
        Mass::from_milligrams(500),
    );
    let initial_matter = matter_total(&fixture.state);
    let initial_energy = explicit_energy_total(&fixture.registries, &fixture.state);
    let mut first = fixture.state.clone();
    let mut second = fixture.state;

    for step in 0..500_u64 {
        let first_duration = commit_one_melt(
            &fixture.registries,
            &mut first,
            fixture.ids,
            Mass::from_milligrams(1),
        );
        let second_duration = commit_one_melt(
            &fixture.registries,
            &mut second,
            fixture.ids,
            Mass::from_milligrams(1),
        );
        assert_eq!(first_duration, second_duration);
        if step % 73 == 0 {
            assert_eq!(validate_loaded_state(&fixture.registries, &first), Ok(()));
            assert_eq!(matter_total(&first), initial_matter);
            assert_eq!(
                explicit_energy_total(&fixture.registries, &first),
                initial_energy
            );
        }
    }

    assert_eq!(first, second);
    assert_eq!(matter_total(&first), initial_matter);
    assert_eq!(
        explicit_energy_total(&fixture.registries, &first),
        initial_energy
    );
    let molten_mass = first
        .inventory()
        .get_stockpile(fixture.ids.destination)
        .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN)));
    assert_eq!(molten_mass, Some(Mass::from_milligrams(500)));
}
