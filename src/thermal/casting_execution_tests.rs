//! Tests for the sibling casting execution module; isolated so test-only edits do not invalidate production builds.

use super::*;
use crate::capability::{
    CapabilityComparison, CapabilityDefinition, CapabilityProfile, CapabilityRequirement,
    CapabilityValue, CapabilityValueKind,
};
use crate::content::{FORM_INGOT, FORM_MOLTEN, MATERIAL_COPPER, make_test_registries_with_casting};
use crate::core::state::{StateValidationError, validate_loaded_state};
use crate::core::time::WorldSeed;
use crate::energy::{
    EnergyStoreDefinition, EnergyStoreDefinitionId, EnergyStoreRecord,
    ExplicitEnergyAccountingError, add_energy_store, calculate_explicit_energy_accounting,
    validate_energy_sink,
};
use crate::equipment::{EquipmentDefinition, EquipmentDefinitionId, add_equipment};
use crate::inventory::{
    MaterialLotId, StockpileStorageProfile, add_solid_stockpile_for_test, add_stockpile,
    deposit_lot_for_test,
};
use crate::maintenance::MaintenanceThresholds;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::production::{
    CompletionCommitError, ProcessDefinition, apply_completion_plan, decide_due_completions,
    validate_start_process,
};
use crate::simulation::advance_tick;
use crate::thermal::ThermalJobValidationError;

const COOLING_POWER: CapabilityId = CapabilityId::new(960_001);
const MAX_TEMPERATURE: CapabilityId = CapabilityId::new(960_002);
const MAX_BATCH_MASS: CapabilityId = CapabilityId::new(960_003);
const MOLD: EquipmentDefinitionId = EquipmentDefinitionId::new(960_001);
const HEAT_SINK: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(960_001);
const PROCESS: ProcessId = ProcessId::new(960_001);
const MELTING_POINT: Temperature = Temperature::from_millikelvin(1_357_770);

#[derive(Clone, Copy)]
struct FixtureIds {
    source: StockpileId,
    destination: StockpileId,
    source_lot: MaterialLotId,
    equipment: EquipmentId,
    heat_sink: EnergyStoreId,
}

struct CastingFixture {
    registries: Registries,
    state: AppState,
    ids: FixtureIds,
}

fn condition(parts_per_million: u32) -> Condition {
    match Condition::new(parts_per_million) {
        Ok(condition) => condition,
        Err(error) => panic!("casting condition fixture failed: {error}"),
    }
}

fn make_registries(
    sink_carrier: EnergyCarrier,
    sink_capacity: Energy,
    sink_input_power: Power,
) -> Registries {
    let profile = match CapabilityProfile::new([
        (
            COOLING_POWER,
            CapabilityValue::Power(Power::from_microwatts(10_000_000)),
        ),
        (
            MAX_TEMPERATURE,
            CapabilityValue::Temperature(Temperature::from_millikelvin(1_600_000)),
        ),
        (
            MAX_BATCH_MASS,
            CapabilityValue::Mass(Mass::from_milligrams(20)),
        ),
    ]) {
        Ok(profile) => profile,
        Err(error) => panic!("casting capability profile failed: {error}"),
    };
    let thresholds = match MaintenanceThresholds::new(condition(600_000), condition(250_000)) {
        Ok(thresholds) => thresholds,
        Err(error) => panic!("casting maintenance fixture failed: {error}"),
    };
    let equipment = EquipmentDefinition::new(
        MOLD,
        "test cooled casting mold",
        Mass::from_milligrams(500_000),
        profile,
        thresholds,
    );
    let sink = EnergyStoreDefinition::new_with_transfer_limits(
        HEAT_SINK,
        "test finite thermal sink",
        sink_carrier,
        sink_capacity,
        sink_input_power,
        Power::ZERO,
    );
    let process = ProcessDefinition::new_selected_batch(
        PROCESS,
        "pure material casting",
        vec![
            CapabilityRequirement::new(
                COOLING_POWER,
                CapabilityComparison::AtLeast,
                CapabilityValue::Power(Power::from_microwatts(1_000_000)),
            ),
            CapabilityRequirement::new(
                MAX_TEMPERATURE,
                CapabilityComparison::AtLeast,
                CapabilityValue::Temperature(Temperature::from_millikelvin(1_400_000)),
            ),
            CapabilityRequirement::new(
                MAX_BATCH_MASS,
                CapabilityComparison::AtLeast,
                CapabilityValue::Mass(Mass::from_milligrams(1)),
            ),
        ],
    );
    make_test_registries_with_casting(
        vec![
            CapabilityDefinition::new(
                COOLING_POWER,
                "casting cooling power",
                CapabilityValueKind::Power,
            ),
            CapabilityDefinition::new(
                MAX_TEMPERATURE,
                "casting maximum input temperature",
                CapabilityValueKind::Temperature,
            ),
            CapabilityDefinition::new(
                MAX_BATCH_MASS,
                "casting maximum batch mass",
                CapabilityValueKind::Mass,
            ),
        ],
        equipment,
        sink,
        process,
        CastingProcessDefinition::new(
            PROCESS,
            COOLING_POWER,
            MAX_TEMPERATURE,
            MAX_BATCH_MASS,
            EnergyCarrier::Thermal,
            PhaseChangeForms::new(FORM_MOLTEN, FORM_INGOT),
            10,
        ),
    )
}

fn make_fixture(input_mass: Mass, input_temperature: Temperature) -> CastingFixture {
    let registries = make_registries(
        EnergyCarrier::Thermal,
        Energy::from_nanojoules(100_000_000_000),
        Power::from_microwatts(10_000_000),
    );
    let mut state = AppState::new(WorldSeed::new(0x9600_0001));
    let source_profile =
        match StockpileStorageProfile::new(false, true, Temperature::from_millikelvin(1_600_000)) {
            Ok(profile) => profile,
            Err(error) => panic!("casting source profile failed: {error}"),
        };
    let source = match add_stockpile(&mut state, Mass::from_milligrams(1_000), source_profile) {
        Ok(source) => source,
        Err(error) => panic!("casting source fixture failed: {error}"),
    };
    let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000)) {
        Ok(destination) => destination,
        Err(error) => panic!("casting destination fixture failed: {error}"),
    };
    let source_lot = match deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN),
        input_mass,
        input_temperature,
    ) {
        Ok(lot) => lot,
        Err(error) => panic!("casting molten input fixture failed: {error}"),
    };
    let equipment = match add_equipment(&registries, &mut state, MOLD, Condition::PRISTINE) {
        Ok(equipment) => equipment,
        Err(error) => panic!("casting equipment fixture failed: {error}"),
    };
    let heat_sink = match add_energy_store(&registries, &mut state, HEAT_SINK) {
        Ok(store) => store,
        Err(error) => panic!("casting heat sink fixture failed: {error}"),
    };
    CastingFixture {
        registries,
        state,
        ids: FixtureIds {
            source,
            destination,
            source_lot,
            equipment,
            heat_sink,
        },
    }
}

fn resolve_selected(
    registries: &Registries,
    state: &AppState,
    ids: FixtureIds,
    mass: Mass,
) -> Result<ResolvedCasting, CastingResolutionError> {
    resolve_casting_process(
        registries,
        state,
        CastingRequest::new(
            PROCESS,
            ids.source,
            &[MaterialLotSelection::new(ids.source_lot, mass)],
            ids.equipment,
            ids.heat_sink,
        ),
    )
}

fn matter_total(state: &AppState) -> crate::core::quantity::AggregateMass {
    match calculate_matter_accounting(state) {
        Ok(accounting) => accounting.total(),
        Err(error) => panic!("casting matter accounting failed: {error}"),
    }
}

fn energy_total(registries: &Registries, state: &AppState) -> Energy {
    match calculate_explicit_energy_accounting(registries, state).and_then(|accounting| {
        accounting
            .total()
            .ok_or(ExplicitEnergyAccountingError::Overflow)
    }) {
        Ok(total) => total,
        Err(error) => panic!("casting energy accounting failed: {error}"),
    }
}

fn finish_job(registries: &Registries, state: &mut AppState, duration: TickSpan) {
    for _ in 0..duration.value() {
        if let Err(error) = advance_tick(registries, state) {
            panic!("casting completion tick failed: {error}");
        }
    }
}

#[test]
fn casting_at_fusion_boundary_releases_exact_latent_heat() {
    let fixture = make_fixture(Mass::from_milligrams(10), MELTING_POINT);
    let expected = match calculate_fusion_heat(
        fixture.registries.materials(),
        Mass::from_milligrams(10),
        MATERIAL_COPPER,
    ) {
        Ok(heat) => heat.energy(),
        Err(error) => panic!("casting latent heat fixture failed: {error}"),
    };

    let resolved = match resolve_selected(
        &fixture.registries,
        &fixture.state,
        fixture.ids,
        Mass::from_milligrams(10),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("casting resolution failed: {error}"),
    };

    assert_eq!(resolved.material(), MATERIAL_COPPER);
    assert_eq!(resolved.melting_point(), MELTING_POINT);
    assert_eq!(resolved.released_energy(), expected);
    assert_eq!(resolved.process_resolution().outputs().len(), 1);
    let output = &resolved.process_resolution().outputs()[0];
    assert_eq!(
        output.commodity(),
        CommodityKey::new(MATERIAL_COPPER, FORM_INGOT)
    );
    assert_eq!(output.mass(), Mass::from_milligrams(10));
    assert_eq!(output.temperature(), MELTING_POINT);
}

#[test]
fn superheated_casting_releases_sensible_cooling_plus_latent_heat() {
    let input_temperature = Temperature::from_millikelvin(1_400_000);
    let fixture = make_fixture(Mass::from_milligrams(10), input_temperature);
    let sensible = match calculate_sensible_heat(
        fixture.registries.materials(),
        Mass::from_milligrams(10),
        &MaterialComposition::pure(MATERIAL_COPPER),
        input_temperature,
        MELTING_POINT,
    ) {
        Ok(heat) => heat.energy(),
        Err(error) => panic!("casting sensible-cooling fixture failed: {error}"),
    };
    let latent = match calculate_fusion_heat(
        fixture.registries.materials(),
        Mass::from_milligrams(10),
        MATERIAL_COPPER,
    ) {
        Ok(heat) => heat.energy(),
        Err(error) => panic!("casting latent fixture failed: {error}"),
    };
    let expected = match sensible.checked_add(latent) {
        Some(energy) => energy,
        None => panic!("casting expected released energy overflowed"),
    };

    let resolved = match resolve_selected(
        &fixture.registries,
        &fixture.state,
        fixture.ids,
        Mass::from_milligrams(10),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("superheated casting resolution failed: {error}"),
    };

    assert_eq!(resolved.released_energy(), expected);
    assert!(resolved.released_energy() > latent);
}

#[test]
fn casting_rejects_wrong_energy_sink_carrier_without_mutation() {
    let registries = make_registries(
        EnergyCarrier::Electrical,
        Energy::from_nanojoules(10_000_000_000),
        Power::from_microwatts(10_000_000),
    );
    let mut state = AppState::new(WorldSeed::new(0x9600_0002));
    let source_profile =
        match StockpileStorageProfile::new(false, true, Temperature::from_millikelvin(1_500_000)) {
            Ok(profile) => profile,
            Err(error) => panic!("wrong-carrier source profile failed: {error}"),
        };
    let source = match add_stockpile(&mut state, Mass::from_milligrams(100), source_profile) {
        Ok(source) => source,
        Err(error) => panic!("wrong-carrier source failed: {error}"),
    };
    let lot = match deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN),
        Mass::from_milligrams(10),
        MELTING_POINT,
    ) {
        Ok(lot) => lot,
        Err(error) => panic!("wrong-carrier molten input failed: {error}"),
    };
    let equipment = match add_equipment(&registries, &mut state, MOLD, Condition::PRISTINE) {
        Ok(equipment) => equipment,
        Err(error) => panic!("wrong-carrier equipment failed: {error}"),
    };
    let sink = match add_energy_store(&registries, &mut state, HEAT_SINK) {
        Ok(sink) => sink,
        Err(error) => panic!("wrong-carrier sink failed: {error}"),
    };
    let before = state.clone();

    assert!(matches!(
        resolve_casting_process(
            &registries,
            &state,
            CastingRequest::new(
                PROCESS,
                source,
                &[MaterialLotSelection::new(lot, Mass::from_milligrams(10))],
                equipment,
                sink,
            ),
        ),
        Err(CastingResolutionError::WrongEnergyCarrier {
            required: EnergyCarrier::Thermal,
            provided: EnergyCarrier::Electrical,
        })
    ));
    assert_eq!(state, before);
}

#[test]
fn casting_moves_released_heat_only_when_completion_becomes_authoritative() {
    let mut fixture = make_fixture(Mass::from_milligrams(10), MELTING_POINT);
    let initial_matter = matter_total(&fixture.state);
    let initial_energy = energy_total(&fixture.registries, &fixture.state);
    let resolved = match resolve_selected(
        &fixture.registries,
        &fixture.state,
        fixture.ids,
        Mass::from_milligrams(10),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("casting resolution failed: {error}"),
    };
    let released = resolved.released_energy();
    let duration = resolved.process_resolution().duration();
    let token = match validate_start_process(
        &fixture.registries,
        &fixture.state,
        resolved.process_resolution(),
        fixture.ids.source,
        fixture.ids.destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("casting start validation failed: {error}"),
    };
    if let Err(error) = token.commit(&mut fixture.state) {
        panic!("casting start commit failed: {error}");
    }

    assert_eq!(
        fixture
            .state
            .energy()
            .get_store(fixture.ids.heat_sink)
            .map(EnergyStoreRecord::stored),
        Some(Energy::ZERO)
    );
    assert_eq!(matter_total(&fixture.state), initial_matter);
    assert_eq!(
        energy_total(&fixture.registries, &fixture.state),
        initial_energy
    );
    assert_eq!(
        validate_loaded_state(&fixture.registries, &fixture.state),
        Ok(())
    );

    finish_job(&fixture.registries, &mut fixture.state, duration);
    assert_eq!(
        fixture
            .state
            .energy()
            .get_store(fixture.ids.heat_sink)
            .map(EnergyStoreRecord::stored),
        Some(released)
    );
    assert_eq!(matter_total(&fixture.state), initial_matter);
    assert_eq!(
        energy_total(&fixture.registries, &fixture.state),
        initial_energy
    );
    assert_eq!(
        fixture
            .state
            .inventory()
            .get_stockpile(fixture.ids.destination)
            .map(|stockpile| {
                stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_INGOT))
            }),
        Some(Mass::from_milligrams(10))
    );
}

#[test]
fn active_casting_job_reserves_thermal_sink_exclusively() {
    let mut fixture = make_fixture(Mass::from_milligrams(10), MELTING_POINT);
    let resolved = match resolve_selected(
        &fixture.registries,
        &fixture.state,
        fixture.ids,
        Mass::from_milligrams(10),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("casting resolution failed: {error}"),
    };
    let released = resolved.released_energy();
    let token = match validate_start_process(
        &fixture.registries,
        &fixture.state,
        resolved.process_resolution(),
        fixture.ids.source,
        fixture.ids.destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("casting start validation failed: {error}"),
    };
    let job = match token.commit(&mut fixture.state) {
        Ok(job) => job,
        Err(error) => panic!("casting start commit failed: {error}"),
    };

    assert_eq!(
        validate_energy_sink(
            &fixture.registries,
            &fixture.state,
            fixture.ids.heat_sink,
            released,
        ),
        Err(EnergySinkError::StoreBusy {
            store: fixture.ids.heat_sink,
            job,
            release: fixture
                .state
                .production()
                .get_job(job)
                .map(ProductionJobRecord::occupancy_release)
                .unwrap_or_else(|| panic!("casting job disappeared")),
        })
    );
}

#[test]
fn due_casting_completion_rejects_stale_energy_revision_atomically() {
    let mut fixture = make_fixture(Mass::from_milligrams(10), MELTING_POINT);
    let resolved = match resolve_selected(
        &fixture.registries,
        &fixture.state,
        fixture.ids,
        Mass::from_milligrams(10),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("casting completion-race resolution failed: {error}"),
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
        Err(error) => panic!("casting completion-race start failed: {error}"),
    };
    let job = match token.commit(&mut fixture.state) {
        Ok(job) => job,
        Err(error) => panic!("casting completion-race commit failed: {error}"),
    };
    for _ in 1..duration.value() {
        if let Err(error) = advance_tick(&fixture.registries, &mut fixture.state) {
            panic!("casting completion-race pre-due tick failed: {error}");
        }
    }
    let due = match fixture.state.production().get_job(job) {
        Some(record) => record.completes_at(),
        None => panic!("casting completion-race job disappeared before planning"),
    };
    let plan = match decide_due_completions(&fixture.registries, &fixture.state, due) {
        Ok(plan) => plan,
        Err(error) => panic!("casting completion-race planning failed: {error:?}"),
    };
    let expected = fixture.state.energy().revision();
    if let Err(error) = add_energy_store(&fixture.registries, &mut fixture.state, HEAT_SINK) {
        panic!("casting independent energy mutation failed: {error}");
    }
    let before = fixture.state.clone();

    assert_eq!(
        apply_completion_plan(&mut fixture.state, plan),
        Err(CompletionCommitError::EnergyRevisionConflict {
            expected,
            actual: expected + 1,
        })
    );
    assert_eq!(fixture.state, before);
    assert!(fixture.state.production().get_job(job).is_some());
    assert_eq!(
        fixture
            .state
            .energy()
            .get_store(fixture.ids.heat_sink)
            .map(EnergyStoreRecord::stored),
        Some(Energy::ZERO)
    );
    assert_eq!(
        fixture
            .state
            .inventory()
            .get_stockpile(fixture.ids.destination)
            .map(|stockpile| {
                stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_INGOT))
            }),
        Some(Mass::ZERO)
    );
}

#[test]
fn casting_save_resume_preserves_exact_completion_and_rejects_tampered_heat() {
    let mut fixture = make_fixture(Mass::from_milligrams(10), MELTING_POINT);
    let resolved = match resolve_selected(
        &fixture.registries,
        &fixture.state,
        fixture.ids,
        Mass::from_milligrams(10),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("casting resolution failed: {error}"),
    };
    let required = resolved.released_energy();
    let duration = resolved.process_resolution().duration();
    let token = match validate_start_process(
        &fixture.registries,
        &fixture.state,
        resolved.process_resolution(),
        fixture.ids.source,
        fixture.ids.destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("casting start validation failed: {error}"),
    };
    let job = match token.commit(&mut fixture.state) {
        Ok(job) => job,
        Err(error) => panic!("casting start commit failed: {error}"),
    };
    let encoded = match serde_json::to_vec(&SaveEnvelope::new(&fixture.registries, &fixture.state))
    {
        Ok(encoded) => encoded,
        Err(error) => panic!("casting save serialization failed: {error}"),
    };
    let decoded: LoadedSaveEnvelope = match serde_json::from_slice(&encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("casting save deserialization failed: {error}"),
    };
    let mut resumed = match decoded.into_state(&fixture.registries) {
        Ok(state) => state,
        Err(error) => panic!("casting save validation failed: {error}"),
    };
    let mut uninterrupted = fixture.state.clone();
    finish_job(&fixture.registries, &mut resumed, duration);
    finish_job(&fixture.registries, &mut uninterrupted, duration);
    assert_eq!(resumed, uninterrupted);

    let mut tampered =
        match serde_json::to_value(SaveEnvelope::new(&fixture.registries, &fixture.state)) {
            Ok(encoded) => encoded,
            Err(error) => panic!("casting tamper serialization failed: {error}"),
        };
    tampered["state"]["systems"]["production"]["jobs"][job.value().to_string()]["resources"]["released_energy"]
        ["energy"] = serde_json::json!(1_u64);
    let tampered: LoadedSaveEnvelope = match serde_json::from_value(tampered) {
        Ok(decoded) => decoded,
        Err(error) => panic!("casting tampered save failed decode: {error}"),
    };
    assert_eq!(
        tampered.into_state(&fixture.registries),
        Err(LoadError::InvalidState(StateValidationError::ThermalJob(
            ThermalJobValidationError::Casting(CastingJobValidationError::ReleasedEnergyMismatch {
                job,
                traced: Energy::from_nanojoules(1),
                required,
            })
        )))
    );
}

#[cfg(feature = "test-soak")]
#[test]
#[ignore = "long-horizon soak"]
fn casting_soak_preserves_conservation_and_replay() {
    let fixture = make_fixture(Mass::from_milligrams(300), MELTING_POINT);
    let initial_matter = matter_total(&fixture.state);
    let initial_energy = energy_total(&fixture.registries, &fixture.state);
    let mut first = fixture.state.clone();
    let mut second = fixture.state;

    for step in 0..300_u64 {
        for state in [&mut first, &mut second] {
            let resolved = match resolve_selected(
                &fixture.registries,
                state,
                fixture.ids,
                Mass::from_milligrams(1),
            ) {
                Ok(resolved) => resolved,
                Err(error) => panic!("casting soak resolution failed: {error}"),
            };
            let duration = resolved.process_resolution().duration();
            let token = match validate_start_process(
                &fixture.registries,
                state,
                resolved.process_resolution(),
                fixture.ids.source,
                fixture.ids.destination,
            ) {
                Ok(token) => token,
                Err(error) => panic!("casting soak start failed: {error}"),
            };
            if let Err(error) = token.commit(state) {
                panic!("casting soak commit failed: {error}");
            }
            finish_job(&fixture.registries, state, duration);
        }
        if step % 47 == 0 {
            assert_eq!(validate_loaded_state(&fixture.registries, &first), Ok(()));
            assert_eq!(matter_total(&first), initial_matter);
            assert_eq!(energy_total(&fixture.registries, &first), initial_energy);
        }
    }

    assert_eq!(first, second);
    assert_eq!(matter_total(&first), initial_matter);
    assert_eq!(energy_total(&fixture.registries, &first), initial_energy);
    assert_eq!(
        first
            .inventory()
            .get_stockpile(fixture.ids.destination)
            .map(|stockpile| {
                stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_INGOT))
            }),
        Some(Mass::from_milligrams(300))
    );
}
