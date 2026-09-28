//! Built-in registry assembly, reference integrity, and resolver-ownership tests.

use super::*;
use crate::capability::{
    CapabilityComparison, CapabilityDefinition, CapabilityId, CapabilityRegistry,
    CapabilityRequirement, CapabilityValue, CapabilityValueKind,
};
use crate::core::quantity::{
    Energy, Length, Mass, MassFlow, MassSpecificEnergy, Power, Temperature,
};
use crate::core::time::TickSpan;
use crate::energy::{EnergyCarrier, EnergyStoreDefinition, EnergyStoreDefinitionId};
use crate::material::{
    CommodityKey, MaterialAssemblyProfile, MaterialInputSpec, ParticleSizeRange,
};
use crate::ore_processing::{
    ComminutionProcessDefinition, OreProcessingRegistry, PoweredOreProcessProfile,
};
use crate::production::{ProcessDefinition, ProcessId, ProductionRegistry};
use crate::survival::{
    ConsumptionTemperatureRange, FoodCategory, FoodDefinition, SurvivalRegistry,
};
use crate::thermal::{
    CastingPhaseChange, CastingProcessDefinition, MeltingProcessDefinition, PhaseChangeForms,
    PhaseChangeProcessProfile, SensibleHeatingProcessDefinition, ThermalRegistry,
};

const TEST_CAPABILITY: CapabilityId = CapabilityId::new(700_001);
const TEST_PROCESS: ProcessId = ProcessId::new(700_001);
const TEST_MASS_FLOW: CapabilityId = CapabilityId::new(700_002);
const TEST_MAX_BATCH_MASS: CapabilityId = CapabilityId::new(700_003);
const TEST_HEATING_POWER: CapabilityId = CapabilityId::new(700_004);
const TEST_MAX_TEMPERATURE: CapabilityId = CapabilityId::new(700_005);

#[test]
fn built_in_water_has_an_authoritative_liquid_phase_boundary() {
    let registries = build_registries();
    let water = registries
        .materials()
        .get_material(MATERIAL_WATER)
        .unwrap_or_else(|| panic!("built-in water material disappeared"));
    let fusion = water
        .properties()
        .thermal()
        .fusion()
        .unwrap_or_else(|| panic!("built-in liquid water must define fusion physics"));

    assert_eq!(fusion.melting_point(), materials::WATER_MELTING_POINT);
    assert_eq!(
        fusion.latent_heat_j_per_kg(),
        materials::WATER_LATENT_HEAT_OF_FUSION_J_PER_KG
    );
    assert_eq!(
        registries
            .fluid()
            .get_fluid(FLUID_WATER)
            .and_then(|definition| {
                definition.minimum_modeled_temperature(registries.materials())
            }),
        Some(materials::WATER_MELTING_POINT)
    );
}

#[test]
fn infrastructure_assembly_cannot_hide_perishable_food_from_storage_age() {
    let food = CommodityKey::new(MATERIAL_WOOD, FORM_BOARD);
    let infrastructure = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let base_survival = survival::build_test_survival_registry();
    let survival = SurvivalRegistry::new(
        base_survival.physiology(),
        [FoodDefinition::new(
            food,
            FoodCategory::Fruit,
            MassSpecificEnergy::from_nanojoules_per_milligram(1),
            0,
            TickSpan::new(10),
            ConsumptionTemperatureRange::new(
                Temperature::from_millikelvin(273_150),
                Temperature::from_millikelvin(333_150),
            ),
        )],
        std::iter::empty(),
    );
    let store = EnergyStoreDefinition::new_with_transfer_limits(
        EnergyStoreDefinitionId::new(990_001),
        "perishable embodiment fixture",
        EnergyCarrier::Mechanical,
        Energy::from_nanojoules(10),
        Power::from_picowatts(1_000),
        Power::from_picowatts(1_000),
    )
    .with_assembly_profile(MaterialAssemblyProfile::new(vec![MaterialInputSpec::pure(
        infrastructure,
        Mass::from_milligrams(1),
    )]));

    let result = std::panic::catch_unwind(|| {
        make_test_registries_with_energy_store_and_survival(store, survival)
    });

    assert!(result.is_err());
}

fn assert_thermal_reference_validation_rejects(thermal: ThermalRegistry) {
    let mut capabilities = CapabilityRegistry::new();
    for (id, name, kind) in [
        (
            TEST_HEATING_POWER,
            "test thermal power",
            CapabilityValueKind::Power,
        ),
        (
            TEST_MAX_TEMPERATURE,
            "test maximum temperature",
            CapabilityValueKind::Temperature,
        ),
        (
            TEST_MAX_BATCH_MASS,
            "test maximum batch mass",
            CapabilityValueKind::Mass,
        ),
    ] {
        capabilities.register_capability(CapabilityDefinition::new(id, name, kind));
    }
    let mut production = ProductionRegistry::new();
    production.register_process(ProcessDefinition::new(
        TEST_PROCESS,
        "invalid phase-change fixture",
        vec![
            CapabilityRequirement::new(
                TEST_HEATING_POWER,
                CapabilityComparison::AtLeast,
                CapabilityValue::Power(Power::from_picowatts(1)),
            ),
            CapabilityRequirement::new(
                TEST_MAX_TEMPERATURE,
                CapabilityComparison::AtLeast,
                CapabilityValue::Temperature(Temperature::from_millikelvin(1)),
            ),
            CapabilityRequirement::new(
                TEST_MAX_BATCH_MASS,
                CapabilityComparison::AtLeast,
                CapabilityValue::Mass(Mass::from_milligrams(1)),
            ),
        ],
    ));
    let material_registry = materials::build_material_registry();

    let result = std::panic::catch_unwind(|| {
        thermal.validate_references(&production, &capabilities, &material_registry)
    });

    assert!(result.is_err());
}

#[test]
fn built_in_world_time_scale_and_gravity_are_stable() {
    let registries = build_registries();

    assert_eq!(
        registries.core().physical_tick_duration().microseconds(),
        3_600_000
    );
    assert_eq!(
        registries.core().gravity().micrometers_per_second_squared(),
        DEFAULT_GRAVITY_MICROMETERS_PER_SECOND_SQUARED
    );
    assert_eq!(
        registries.core().calendar().ticks_per_day(),
        DEFAULT_TICKS_PER_DAY
    );
    assert_eq!(
        registries.core().calendar().physical_seconds_per_day(),
        DEFAULT_PHYSICAL_SECONDS_PER_DAY
    );
}

#[path = "mod_tests/gameplay_authoring.rs"]
mod gameplay_authoring;
#[path = "mod_tests/presentation.rs"]
mod presentation;
#[path = "mod_tests/public_contracts.rs"]
mod public_contracts;
#[path = "mod_tests/resolver_contracts.rs"]
mod resolver_contracts;
