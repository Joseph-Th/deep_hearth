//! Agreement tests for powered ore current-state mass planning.

use super::*;
use crate::capability::{
    CapabilityComparison, CapabilityDefinition, CapabilityId, CapabilityProfile,
    CapabilityRequirement, CapabilityValue, CapabilityValueKind,
};
use crate::content::{
    FORM_CRUSHED, FORM_INGOT, FORM_ORE, MATERIAL_COPPER, make_test_registries_with_comminution,
};
use crate::core::quantity::{Energy, Length, Mass, MassFlow, MassSpecificEnergy, Power};
use crate::core::state::AppState;
use crate::core::time::WorldSeed;
use crate::energy::{
    EnergyCarrier, EnergyStoreDefinition, EnergyStoreDefinitionId, EnergySupplyError,
    add_energy_store_with_initial_for_fixture,
};
use crate::equipment::{EquipmentDefinition, EquipmentDefinitionId, add_equipment};
use crate::inventory::{
    MaterialLotSelection, StockpileId, add_solid_stockpile_for_test, deposit_composed_lot_for_test,
};
use crate::maintenance::{Condition, MaintenanceThresholds};
use crate::material::{CommodityKey, MaterialComposition, ParticleSizeRange};
use crate::ore_processing::{
    ComminutionProcessDefinition, ComminutionRequest, ComminutionResolutionError,
    PoweredOreProcessProfile, resolve_comminution_process,
};
use crate::production::{ProcessDefinition, ProcessId, validate_start_process};
use crate::registry::Registries;

const FLOW: CapabilityId = CapabilityId::new(976_001);
const MAX_BATCH: CapabilityId = CapabilityId::new(976_002);
const EQUIPMENT: EquipmentDefinitionId = EquipmentDefinitionId::new(976_001);
const STORE: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(976_001);
const PROCESS: ProcessId = ProcessId::new(976_001);

#[derive(Clone, Copy)]
struct PlanningConfig {
    flow_mg_per_second: u64,
    max_batch_mg: u64,
    specific_nj_per_mg: u64,
    wear_ppm_per_tick: u32,
    condition_ppm: u32,
    stored_nj: u128,
    output_power: Power,
    store_carrier: EnergyCarrier,
}

impl Default for PlanningConfig {
    fn default() -> Self {
        Self {
            flow_mg_per_second: 1_000,
            max_batch_mg: 1_000,
            specific_nj_per_mg: 100,
            wear_ppm_per_tick: 1,
            condition_ppm: 1_000_000,
            stored_nj: 1_000_000,
            output_power: Power::from_microwatts(1_000_000),
            store_carrier: EnergyCarrier::Mechanical,
        }
    }
}

struct PlanningFixture {
    registries: Registries,
    state: AppState,
    source: StockpileId,
    destination: StockpileId,
    lot: crate::inventory::MaterialLotId,
    equipment: crate::equipment::EquipmentId,
    store: crate::energy::EnergyStoreId,
}

fn condition(ppm: u32) -> Condition {
    Condition::new(ppm).unwrap_or_else(|error| panic!("planning condition fixture failed: {error}"))
}

fn particle_size() -> ParticleSizeRange {
    ParticleSizeRange::new(
        Length::from_micrometers(1),
        Length::from_micrometers(10_000),
    )
    .unwrap_or_else(|error| panic!("planning particle-size fixture failed: {error}"))
}

fn make_registries(config: PlanningConfig) -> Registries {
    let capabilities = CapabilityProfile::new([
        (
            FLOW,
            CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(
                config.flow_mg_per_second,
            )),
        ),
        (
            MAX_BATCH,
            CapabilityValue::Mass(Mass::from_milligrams(config.max_batch_mg)),
        ),
    ])
    .unwrap_or_else(|error| panic!("planning capability profile failed: {error}"));
    let thresholds = MaintenanceThresholds::new(condition(600_000), condition(250_000))
        .unwrap_or_else(|error| panic!("planning maintenance thresholds failed: {error}"));
    let equipment = EquipmentDefinition::new(
        EQUIPMENT,
        "planning crusher",
        Mass::from_milligrams(1_000_000),
        capabilities,
        thresholds,
    );
    let process = ProcessDefinition::new_selected_batch(
        PROCESS,
        "planning comminution",
        vec![
            CapabilityRequirement::new(
                FLOW,
                CapabilityComparison::AtLeast,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(1)),
            ),
            CapabilityRequirement::new(
                MAX_BATCH,
                CapabilityComparison::AtLeast,
                CapabilityValue::Mass(Mass::from_milligrams(1)),
            ),
        ],
    );
    let ore = ComminutionProcessDefinition::new(
        PROCESS,
        FORM_ORE,
        FORM_CRUSHED,
        particle_size(),
        PoweredOreProcessProfile::new(
            FLOW,
            MAX_BATCH,
            EnergyCarrier::Mechanical,
            MassSpecificEnergy::from_nanojoules_per_milligram(config.specific_nj_per_mg),
            config.wear_ppm_per_tick,
        ),
    );
    make_test_registries_with_comminution(
        vec![
            CapabilityDefinition::new(FLOW, "planning mass flow", CapabilityValueKind::MassFlow),
            CapabilityDefinition::new(MAX_BATCH, "planning max batch", CapabilityValueKind::Mass),
        ],
        equipment,
        EnergyStoreDefinition::new_with_transfer_limits(
            STORE,
            "planning work store",
            config.store_carrier,
            Energy::from_nanojoules(config.stored_nj.max(10_000_000)),
            Power::ZERO,
            config.output_power,
        ),
        process,
        ore,
    )
}

fn make_fixture(config: PlanningConfig) -> PlanningFixture {
    let registries = make_registries(config);
    let mut state = AppState::new(WorldSeed::new(0x9760_0001));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_000))
        .unwrap_or_else(|error| panic!("planning source fixture failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_000))
        .unwrap_or_else(|error| panic!("planning destination fixture failed: {error}"));
    let lot = deposit_composed_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        Mass::from_milligrams(1_000),
        crate::core::quantity::Temperature::from_millikelvin(300_000),
        MaterialComposition::pure(MATERIAL_COPPER),
    )
    .unwrap_or_else(|error| panic!("planning ore fixture failed: {error}"));
    let equipment = add_equipment(
        &registries,
        &mut state,
        EQUIPMENT,
        condition(config.condition_ppm),
    )
    .unwrap_or_else(|error| panic!("planning equipment fixture failed: {error}"));
    let store = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        STORE,
        Energy::from_nanojoules(config.stored_nj),
    )
    .unwrap_or_else(|error| panic!("planning energy fixture failed: {error}"));
    PlanningFixture {
        registries,
        state,
        source,
        destination,
        lot,
        equipment,
        store,
    }
}

fn envelope(fixture: &PlanningFixture) -> PoweredOreMassEnvelope {
    assess_powered_ore_mass_envelope(
        &fixture.registries,
        &fixture.state,
        PROCESS,
        fixture.equipment,
        fixture.store,
    )
    .unwrap_or_else(|error| panic!("planning envelope failed: {error}"))
}

fn resolve(
    fixture: &PlanningFixture,
    state: &AppState,
    mass: Mass,
) -> Result<crate::ore_processing::ResolvedComminution, ComminutionResolutionError> {
    resolve_comminution_process(
        &fixture.registries,
        state,
        ComminutionRequest::new(
            PROCESS,
            fixture.source,
            &[MaterialLotSelection::new(fixture.lot, mass)],
            fixture.equipment,
            fixture.store,
        ),
    )
}

#[test]
fn equipment_capacity_projection_matches_canonical_batch_boundary() {
    let fixture = make_fixture(PlanningConfig {
        max_batch_mg: 10,
        ..PlanningConfig::default()
    });
    let envelope = envelope(&fixture);
    assert_eq!(envelope.equipment_capacity(), Mass::from_milligrams(10));
    assert_eq!(envelope.maximum_mass(), Mass::from_milligrams(10));
    assert_eq!(
        envelope.constraint_for(Mass::from_milligrams(11)),
        Some(PoweredOreMassConstraint::EquipmentCapacity)
    );
    assert!(resolve(&fixture, &fixture.state, Mass::from_milligrams(10)).is_ok());
    assert!(matches!(
        resolve(&fixture, &fixture.state, Mass::from_milligrams(11)),
        Err(ComminutionResolutionError::BatchMassExceeded { selected, maximum })
            if selected == Mass::from_milligrams(11) && maximum == Mass::from_milligrams(10)
    ));
}

#[test]
fn stored_energy_projection_matches_canonical_supply_boundary() {
    let fixture = make_fixture(PlanningConfig {
        stored_nj: 500,
        ..PlanningConfig::default()
    });
    let envelope = envelope(&fixture);
    assert_eq!(envelope.stored_energy_capacity(), Mass::from_milligrams(5));
    assert_eq!(envelope.maximum_mass(), Mass::from_milligrams(5));
    assert!(envelope.maximum_mass_with_replenished_energy() > envelope.maximum_mass());
    assert_eq!(
        envelope.additional_energy_required_for(Mass::from_milligrams(6)),
        Some(Energy::from_nanojoules(100))
    );
    assert_eq!(
        envelope.constraint_for(Mass::from_milligrams(6)),
        Some(PoweredOreMassConstraint::StoredEnergy)
    );
    assert!(resolve(&fixture, &fixture.state, Mass::from_milligrams(5)).is_ok());
    assert!(matches!(
        resolve(&fixture, &fixture.state, Mass::from_milligrams(6)),
        Err(ComminutionResolutionError::Energy(EnergySupplyError::InsufficientEnergy {
            available,
            requested,
            ..
        })) if available == Energy::from_nanojoules(500)
            && requested == Energy::from_nanojoules(600)
    ));
}

#[test]
fn replenishment_projection_refuses_mass_beyond_non_energy_constraints() {
    let fixture = make_fixture(PlanningConfig {
        max_batch_mg: 10,
        stored_nj: 100,
        ..PlanningConfig::default()
    });
    let envelope = envelope(&fixture);

    assert_eq!(
        envelope.maximum_mass_with_replenished_energy(),
        Mass::from_milligrams(10)
    );
    assert_eq!(
        envelope.additional_energy_required_for(Mass::from_milligrams(10)),
        Some(Energy::from_nanojoules(900))
    );
    assert_eq!(
        envelope.additional_energy_required_for(Mass::from_milligrams(11)),
        None
    );
}

#[test]
fn condition_lifetime_projection_matches_first_unusable_duration() {
    let config = PlanningConfig {
        flow_mg_per_second: 1,
        wear_ppm_per_tick: 60,
        condition_ppm: 100,
        ..PlanningConfig::default()
    };
    let fixture = make_fixture(config);
    let envelope = envelope(&fixture);
    let maximum_ticks = maximum_usable_active_ticks(config.wear_ppm_per_tick, condition(100));
    let expected = calculate_mass_flow_capacity(
        MassFlow::from_milligrams_per_second(config.flow_mg_per_second),
        maximum_ticks,
        fixture.registries.core().physical_tick_duration(),
    );
    assert_eq!(envelope.condition_lifetime_capacity(), expected);
    assert_eq!(envelope.maximum_mass(), expected);
    assert!(!expected.is_zero());
    assert!(resolve(&fixture, &fixture.state, expected).is_ok());
    let rejected = Mass::from_milligrams(expected.milligrams() + 1);
    assert_eq!(
        envelope.constraint_for(rejected),
        Some(PoweredOreMassConstraint::ConditionLifetime)
    );
    assert!(matches!(
        resolve(&fixture, &fixture.state, rejected),
        Err(ComminutionResolutionError::ConditionDuration(_))
    ));
}

#[test]
fn caller_selected_condition_floor_has_an_exact_noncritical_mass_bound() {
    let config = PlanningConfig {
        flow_mg_per_second: 1,
        wear_ppm_per_tick: 60_000,
        condition_ppm: 311_000,
        ..PlanningConfig::default()
    };
    let fixture = make_fixture(config);
    let critical_floor = condition(250_000);
    let envelope = envelope(&fixture);
    let safe = envelope.maximum_mass_preserving_condition_above(critical_floor);
    assert!(!safe.is_zero());
    let safe_resolution = resolve(&fixture, &fixture.state, safe).unwrap_or_else(|error| {
        panic!("safe planning boundary failed canonical resolution: {error}")
    });
    assert!(safe_resolution.condition_after() > critical_floor);

    let next = Mass::from_milligrams(safe.milligrams() + 1);
    let next_resolution = resolve(&fixture, &fixture.state, next)
        .unwrap_or_else(|error| panic!("next usable planning mass should still resolve: {error}"));
    assert!(next_resolution.condition_after() <= critical_floor);
}

#[test]
fn mass_envelope_does_not_overclaim_process_specific_feed_legality() {
    let mut fixture = make_fixture(PlanningConfig::default());
    let wrong_lot = deposit_composed_lot_for_test(
        &fixture.registries,
        &mut fixture.state,
        fixture.source,
        CommodityKey::new(MATERIAL_COPPER, FORM_INGOT),
        Mass::from_milligrams(10),
        crate::core::quantity::Temperature::from_millikelvin(300_000),
        MaterialComposition::pure(MATERIAL_COPPER),
    )
    .unwrap_or_else(|error| panic!("wrong-feed planning fixture failed: {error}"));
    let planned = envelope(&fixture);
    assert!(planned.maximum_mass() >= Mass::from_milligrams(10));
    assert!(matches!(
        resolve_comminution_process(
            &fixture.registries,
            &fixture.state,
            ComminutionRequest::new(
                PROCESS,
                fixture.source,
                &[MaterialLotSelection::new(
                    wrong_lot,
                    Mass::from_milligrams(10)
                )],
                fixture.equipment,
                fixture.store,
            ),
        ),
        Err(ComminutionResolutionError::Batch(_))
    ));
}

#[test]
fn retained_envelope_never_authorizes_work_after_owner_state_changes() {
    let mut fixture = make_fixture(PlanningConfig::default());
    let retained = envelope(&fixture);
    let resolved = resolve(&fixture, &fixture.state, Mass::from_milligrams(10))
        .unwrap_or_else(|error| panic!("stale-envelope setup resolution failed: {error}"));
    let job = validate_start_process(
        &fixture.registries,
        &fixture.state,
        resolved.process_resolution(),
        fixture.source,
        fixture.destination,
    )
    .unwrap_or_else(|error| panic!("stale-envelope process start failed: {error}"))
    .commit(&mut fixture.state)
    .unwrap_or_else(|error| panic!("stale-envelope process commit failed: {error}"));

    assert!(!retained.maximum_mass().is_zero());
    assert!(matches!(
        assess_powered_ore_mass_envelope(
            &fixture.registries,
            &fixture.state,
            PROCESS,
            fixture.equipment,
            fixture.store,
        ),
        Err(PoweredOreMassEnvelopeError::Equipment(
            EquipmentProviderError::ProductionInProgress {
                equipment: occupied_equipment,
                job: occupied_by,
                ..
            }
        )) if occupied_equipment == fixture.equipment && occupied_by == job
    ));
}

#[test]
fn current_envelope_rejects_busy_equipment_even_with_idle_energy_store() {
    let mut fixture = make_fixture(PlanningConfig::default());
    let resolved = resolve(&fixture, &fixture.state, Mass::from_milligrams(10))
        .unwrap_or_else(|error| panic!("busy-equipment setup resolution failed: {error}"));
    let job = validate_start_process(
        &fixture.registries,
        &fixture.state,
        resolved.process_resolution(),
        fixture.source,
        fixture.destination,
    )
    .unwrap_or_else(|error| panic!("busy-equipment process start failed: {error}"))
    .commit(&mut fixture.state)
    .unwrap_or_else(|error| panic!("busy-equipment process commit failed: {error}"));
    let idle_store = add_energy_store_with_initial_for_fixture(
        &fixture.registries,
        &mut fixture.state,
        STORE,
        Energy::from_nanojoules(1_000_000),
    )
    .unwrap_or_else(|error| panic!("idle planning energy fixture failed: {error}"));

    assert!(matches!(
        assess_powered_ore_mass_envelope(
            &fixture.registries,
            &fixture.state,
            PROCESS,
            fixture.equipment,
            idle_store,
        ),
        Err(PoweredOreMassEnvelopeError::Equipment(
            EquipmentProviderError::ProductionInProgress {
                equipment: occupied_equipment,
                job: occupied_by,
                ..
            }
        )) if occupied_equipment == fixture.equipment && occupied_by == job
    ));
}

#[test]
fn envelope_rejects_wrong_energy_carrier_before_reporting_a_mass() {
    let fixture = make_fixture(PlanningConfig {
        store_carrier: EnergyCarrier::Electrical,
        ..PlanningConfig::default()
    });
    assert_eq!(
        assess_powered_ore_mass_envelope(
            &fixture.registries,
            &fixture.state,
            PROCESS,
            fixture.equipment,
            fixture.store,
        ),
        Err(PoweredOreMassEnvelopeError::WrongEnergyCarrier {
            required: EnergyCarrier::Mechanical,
            provided: EnergyCarrier::Electrical,
        })
    );
}
