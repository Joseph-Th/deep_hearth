//! Built-in registry assembly, reference integrity, and resolver-ownership tests.

use std::collections::BTreeSet;

use super::*;
use crate::capability::{
    CapabilityComparison, CapabilityDefinition, CapabilityId, CapabilityRegistry,
    CapabilityRequirement, CapabilityValue, CapabilityValueKind,
};
use crate::core::quantity::{
    Energy, Length, Mass, MassFlow, MassSpecificEnergy, Power, Temperature, Volume,
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
fn built_in_direct_drinking_uses_a_meaningful_serving_floor() {
    let registries = build_registries();

    assert_eq!(
        registries
            .survival()
            .physiology()
            .direct_consumption()
            .minimum_drink_volume(),
        Volume::from_microliters(250_000),
        "ordinary drinking should use a cup-sized serving floor rather than threshold-sipping"
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

fn primitive_commodity_has_root_route(
    registries: &Registries,
    commodity: CommodityKey,
    roots: &BTreeSet<CommodityKey>,
    visiting: &mut BTreeSet<CommodityKey>,
) -> bool {
    if roots.contains(&commodity) {
        return true;
    }
    if !visiting.insert(commodity) {
        return false;
    }
    let reachable = registries
        .crafting()
        .definitions()
        .filter(|definition| {
            definition
                .outputs()
                .iter()
                .any(|output| output.commodity() == commodity)
        })
        .any(|producer| {
            primitive_commodity_has_root_route(registries, producer.input(), roots, visiting)
        });
    assert!(visiting.remove(&commodity));
    reachable
}

fn assert_primitive_commodity_reachable(
    registries: &Registries,
    commodity: CommodityKey,
    roots: &BTreeSet<CommodityKey>,
    visiting: &mut BTreeSet<CommodityKey>,
) {
    assert!(
        primitive_commodity_has_root_route(registries, commodity, roots, visiting),
        "primitive component commodity {} must have at least one acyclic ordinary manual route from an authored primitive root",
        commodity.value()
    );
}

#[test]
fn every_declared_primitive_infrastructure_component_has_a_transitive_runtime_route() {
    let registries = build_registries();
    let roots = BTreeSet::from([
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
    ]);
    let mut required = BTreeSet::new();

    for definition in registries.equipment().definitions() {
        if !definition.has_authored_acquisition_edge() {
            continue;
        }
        if let Some(assembly) = definition.assembly_profile() {
            required.extend(assembly.inputs().iter().map(MaterialInputSpec::commodity));
        }
        if let Some(upgrade) = definition.upgrade_profile() {
            let base = registries
                .equipment()
                .get_equipment(upgrade.from())
                .unwrap_or_else(|| unreachable!("registry validation resolves upgrade bases"));
            assert!(
                base.has_authored_acquisition_edge(),
                "equipment upgrade {} starts from base {} with no direct authored acquisition edge",
                definition.id().value(),
                base.id().value()
            );
            required.extend(
                upgrade
                    .additions()
                    .inputs()
                    .iter()
                    .map(MaterialInputSpec::commodity),
            );
        }
    }
    for definition in registries.energy().definitions() {
        if let Some(assembly) = definition.assembly_profile() {
            required.extend(assembly.inputs().iter().map(MaterialInputSpec::commodity));
        }
    }
    for definition in registries.storage().definitions() {
        required.extend(
            definition
                .assembly_profile()
                .inputs()
                .iter()
                .map(MaterialInputSpec::commodity),
        );
    }

    for commodity in required {
        assert_primitive_commodity_reachable(&registries, commodity, &roots, &mut BTreeSet::new());
    }
}

#[test]
fn built_in_missing_acquisition_edges_are_exactly_capability_only_infrastructure() {
    let registries = build_registries();
    let equipment_without_acquisition = registries
        .equipment()
        .definitions()
        .filter(|definition| !definition.has_authored_acquisition_edge())
        .map(|definition| definition.id())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        equipment_without_acquisition,
        BTreeSet::from([
            EQUIPMENT_JAW_CRUSHER,
            EQUIPMENT_ELECTRIC_FURNACE,
            EQUIPMENT_CASTING_MOLD,
            EQUIPMENT_DRY_SCREEN,
            EQUIPMENT_GRINDING_MILL,
            EQUIPMENT_GRAVITY_SEPARATOR,
        ]),
        "only controlled industrial workshop equipment may lack an ordinary acquisition edge"
    );

    let energy_without_assembly = registries
        .energy()
        .definitions()
        .filter(|definition| !definition.has_authored_assembly_edge())
        .map(|definition| definition.id())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        energy_without_assembly,
        BTreeSet::from([
            ENERGY_MECHANICAL_SMALL_DRIVE,
            ENERGY_MECHANICAL_LARGE_DRIVE,
            ENERGY_ELECTRICAL_BUFFER,
            ENERGY_THERMAL_SINK,
        ]),
        "only controlled workshop energy infrastructure may lack an ordinary assembly edge"
    );
}

#[test]
fn primitive_flywheel_loses_stored_rotation_without_erasing_short_work_windows() {
    let registries = build_registries();
    for (store, expected_loss) in [
        (
            ENERGY_TIMBER_FLYWHEEL_DRIVE,
            Power::from_microwatts(500_000),
        ),
        (
            ENERGY_STONE_FLYWHEEL_DRIVE,
            Power::from_microwatts(1_000_000),
        ),
        (
            ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE,
            Power::from_microwatts(1_000_000),
        ),
        (
            ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE,
            Power::from_microwatts(2_000_000),
        ),
        (
            ENERGY_TIMBER_FRAME_FLYWHEEL_BANK,
            Power::from_microwatts(10_000_000),
        ),
    ] {
        let flywheel = registries
            .energy()
            .get_store(store)
            .unwrap_or_else(|| panic!("built-in primitive flywheel definition disappeared"));
        let loss = flywheel.passive_dissipation_power();
        assert_eq!(loss, expected_loss);
        assert!(
            loss < flywheel.max_input_power() && loss < flywheel.max_output_power(),
            "passive flywheel drag must remain below active transfer power"
        );
        let loss_per_tick = crate::energy::integrate_power(
            loss,
            TickSpan::new(1),
            registries.core().physical_tick_duration(),
            crate::energy::PowerRemainder::ZERO,
        )
        .unwrap_or_else(|error| panic!("primitive flywheel loss integration failed: {error}"));
        assert_eq!(
            loss_per_tick.remainder(),
            crate::energy::PowerRemainder::ZERO
        );
        let passive_ticks = flywheel.capacity().nanojoules() / loss_per_tick.energy().nanojoules();
        assert!(
            (120..=210).contains(&passive_ticks),
            "primitive flywheel full-charge coast time must remain a multi-minute work buffer, not long-term storage"
        );
    }
}

#[test]
fn built_in_workshop_energy_buffers_have_coherent_transfer_and_recovery_rates() {
    let registries = build_registries();
    let electrical = registries
        .energy()
        .get_store(ENERGY_ELECTRICAL_BUFFER)
        .unwrap_or_else(|| panic!("built-in electrical buffer disappeared"));
    let thermal = registries
        .energy()
        .get_store(ENERGY_THERMAL_SINK)
        .unwrap_or_else(|| panic!("built-in thermal sink disappeared"));

    assert_eq!(
        electrical.max_input_power(),
        Power::from_microwatts(1_000_000_000_000),
        "an electrical buffer must accept recharge as well as provide stored power"
    );
    assert_eq!(electrical.max_output_power(), electrical.max_input_power());
    assert_eq!(
        thermal.max_input_power(),
        Power::from_microwatts(1_000_000_000_000)
    );
    assert_eq!(
        thermal.passive_dissipation_power(),
        Power::from_microwatts(100_000_000_000)
    );
    assert!(
        thermal.passive_dissipation_power() < thermal.max_input_power(),
        "passive heat rejection must recover the finite sink more slowly than active casting can fill it"
    );
}

#[test]
fn built_in_protein_options_trade_immediate_density_for_storage_resilience() {
    let registries = build_registries();
    let meat = *registries
        .survival()
        .get_food(CommodityKey::new(MATERIAL_MEAT, FORM_FOOD))
        .unwrap_or_else(|| panic!("built-in meat food definition disappeared"));
    let legumes = *registries
        .survival()
        .get_food(CommodityKey::new(MATERIAL_LEGUMES, FORM_FOOD))
        .unwrap_or_else(|| panic!("built-in legume food definition disappeared"));

    assert_eq!(meat.category(), FoodCategory::Protein);
    assert_eq!(legumes.category(), FoodCategory::Protein);
    assert!(meat.dietary_energy() > legumes.dietary_energy());
    assert!(meat.hydration_multiplier_ppm() > 0);
    assert!(meat.hydration_multiplier_ppm() < 1_000_000);
    assert_eq!(legumes.hydration_multiplier_ppm(), 0);
    assert!(legumes.shelf_life() > meat.shelf_life());
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

#[test]
fn hand_mining_exertion_remains_a_sustained_human_workload() {
    let registries = build_registries();
    let method = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("built-in hand-mining method disappeared"));
    let total_energy_per_tick = registries
        .survival()
        .physiology()
        .basal_energy_cost_per_tick()
        .checked_add(method.exertion().energy_cost_per_tick())
        .unwrap_or_else(|| panic!("hand-mining metabolic cost overflowed"));

    // 2.16 kJ over the authoritative 3.6-second tick is 600 W total metabolic demand.
    assert!(
        total_energy_per_tick <= Energy::from_nanojoules(2_160_000_000_000),
        "sustained hand mining must not require implausible kilowatt-scale human metabolism"
    );
}

#[path = "mod_tests/gameplay_authoring.rs"]
mod gameplay_authoring;
#[path = "mod_tests/presentation.rs"]
mod presentation;
#[path = "mod_tests/resolver_contracts.rs"]
mod resolver_contracts;
