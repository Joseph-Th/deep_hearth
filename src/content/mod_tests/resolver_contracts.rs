//! Registry assembly and physical-resolver ownership rejection tests.

use super::*;

fn process_registry_domains(
    capabilities: CapabilityRegistry,
    ore_processing: OreProcessingRegistry,
    thermal: ThermalRegistry,
    production: ProductionRegistry,
) -> RegistryDomains {
    RegistryDomains {
        energy: empty_energy_registry(),
        fluid: fluid::build_fluid_registry(),
        capabilities,
        crafting: crate::crafting::CraftingRegistry::new(std::iter::empty()),
        labor: labor::empty_labor_registry(),
        equipment: empty_equipment_registry(),
        storage: crate::inventory::StorageRegistry::new(std::iter::empty()),
        structural: structural::build_structural_registry(),
        materials: materials::build_material_registry(),
        mining: crate::mining::MiningRegistry::new(std::iter::empty()),
        ore_processing,
        thermal,
        production,
        survival: survival::build_survival_registry(),
        presentation: RegistryPresentation {
            textures: empty_texture_registry(),
            shaders: empty_shader_registry(),
        },
    }
}

#[test]
fn missing_process_capability_reference_is_rejected_during_registry_assembly() {
    let process = ProcessDefinition::new(
        TEST_PROCESS,
        "test capability process",
        vec![CapabilityRequirement::new(
            TEST_CAPABILITY,
            CapabilityComparison::AtLeast,
            CapabilityValue::Temperature(Temperature::from_millikelvin(500_000)),
        )],
    );
    let mut production = ProductionRegistry::new();
    production.register_process(process);

    let result = std::panic::catch_unwind(|| {
        Registries::new(
            REGISTRY_SCHEMA_VERSION,
            build_core_definitions(),
            process_registry_domains(
                CapabilityRegistry::new(),
                OreProcessingRegistry::new(std::iter::empty()),
                empty_thermal_registry(),
                production,
            ),
        )
    });

    assert!(result.is_err());
}

#[test]
fn process_without_physical_resolver_semantics_is_rejected_during_registry_assembly() {
    let mut production = ProductionRegistry::new();
    production.register_process(ProcessDefinition::new(
        TEST_PROCESS,
        "orphan physical process fixture",
        Vec::new(),
    ));

    let result = std::panic::catch_unwind(|| {
        Registries::new(
            REGISTRY_SCHEMA_VERSION,
            build_core_definitions(),
            process_registry_domains(
                CapabilityRegistry::new(),
                OreProcessingRegistry::new(std::iter::empty()),
                empty_thermal_registry(),
                production,
            ),
        )
    });

    assert!(result.is_err());
}

#[test]
fn process_cannot_own_multiple_physical_resolver_semantics() {
    let mut capabilities = CapabilityRegistry::new();
    for (id, name, kind) in [
        (
            TEST_MASS_FLOW,
            "test mass flow",
            CapabilityValueKind::MassFlow,
        ),
        (
            TEST_MAX_BATCH_MASS,
            "test maximum batch mass",
            CapabilityValueKind::Mass,
        ),
        (
            TEST_HEATING_POWER,
            "test heating power",
            CapabilityValueKind::Power,
        ),
        (
            TEST_MAX_TEMPERATURE,
            "test maximum temperature",
            CapabilityValueKind::Temperature,
        ),
    ] {
        capabilities.register_capability(CapabilityDefinition::new(id, name, kind));
    }
    let process = ProcessDefinition::new(
        TEST_PROCESS,
        "ambiguous physical resolver fixture",
        vec![
            CapabilityRequirement::new(
                TEST_MASS_FLOW,
                CapabilityComparison::AtLeast,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(1)),
            ),
            CapabilityRequirement::new(
                TEST_MAX_BATCH_MASS,
                CapabilityComparison::AtLeast,
                CapabilityValue::Mass(Mass::from_milligrams(1)),
            ),
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
        ],
    );
    let mut production = ProductionRegistry::new();
    production.register_process(process);
    let ore_processing = OreProcessingRegistry::new([ComminutionProcessDefinition::new(
        TEST_PROCESS,
        FORM_ORE,
        FORM_CRUSHED,
        match ParticleSizeRange::new(
            Length::from_micrometers(1),
            Length::from_micrometers(20_000),
        ) {
            Ok(range) => range,
            Err(error) => panic!("comminution particle-size fixture failed: {error}"),
        },
        PoweredOreProcessProfile::new(
            TEST_MASS_FLOW,
            TEST_MAX_BATCH_MASS,
            EnergyCarrier::Mechanical,
            MassSpecificEnergy::from_nanojoules_per_milligram(1),
            1,
        ),
    )]);
    let thermal = ThermalRegistry::new(
        [SensibleHeatingProcessDefinition::new(
            TEST_PROCESS,
            TEST_HEATING_POWER,
            TEST_MAX_TEMPERATURE,
            TEST_MAX_BATCH_MASS,
            EnergyCarrier::Electrical,
            1,
        )],
        std::iter::empty(),
        std::iter::empty(),
    );

    let result = std::panic::catch_unwind(|| {
        Registries::new(
            REGISTRY_SCHEMA_VERSION,
            build_core_definitions(),
            process_registry_domains(capabilities, ore_processing, thermal, production),
        )
    });

    assert!(result.is_err());
}
