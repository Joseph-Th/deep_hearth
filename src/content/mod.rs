//! Built-in immutable game definitions; sibling files own authored definitions for each registry domain.

mod capabilities;
mod energy;
mod equipment;
mod fluid;
#[cfg(test)]
mod gameplay_harness;
mod materials;
mod ore_processing;
mod processes;
mod structural;
mod thermal;

use crate::core::quantity::Acceleration;
use crate::registry::{CoreDefinitions, Registries, RegistryDomains, RegistrySchemaVersion};

#[cfg(test)]
use crate::capability::{CapabilityDefinition, CapabilityRegistry};
#[cfg(test)]
use crate::energy::{EnergyRegistry, EnergyStoreDefinition};
#[cfg(test)]
use crate::equipment::{EquipmentDefinition, EquipmentRegistry};
#[cfg(test)]
use crate::fluid::{FluidDefinition, FluidRegistry};
#[cfg(test)]
use crate::ore_processing::{
    ComminutionProcessDefinition, OreProcessingRegistry, ScreeningProcessDefinition,
};
#[cfg(test)]
use crate::production::{ProcessDefinition, ProductionRegistry};
#[cfg(test)]
use crate::thermal::{
    CastingProcessDefinition, MeltingProcessDefinition, SensibleHeatingProcessDefinition,
    ThermalRegistry,
};

#[cfg(test)]
fn empty_energy_registry() -> EnergyRegistry {
    EnergyRegistry::new(std::iter::empty())
}

#[cfg(test)]
pub(crate) fn make_test_registries_with_screening(
    capability_definitions: Vec<CapabilityDefinition>,
    equipment_definition: EquipmentDefinition,
    energy_definition: EnergyStoreDefinition,
    process: ProcessDefinition,
    screening_definition: ScreeningProcessDefinition,
) -> Registries {
    let mut capabilities = CapabilityRegistry::new();
    for capability in capability_definitions {
        capabilities.register_capability(capability);
    }
    let mut production = ProductionRegistry::new();
    production.register_process_for_test(process);
    Registries::new(
        REGISTRY_SCHEMA_VERSION,
        build_core_definitions(),
        RegistryDomains {
            energy: EnergyRegistry::new([energy_definition]),
            fluid: fluid::build_fluid_registry(),
            capabilities,
            equipment: EquipmentRegistry::new([equipment_definition]),
            structural: structural::build_structural_registry(),
            materials: materials::build_material_registry(),
            ore_processing: OreProcessingRegistry::new_with_screening(
                std::iter::empty(),
                [screening_definition],
            ),
            thermal: empty_thermal_registry(),
            production,
        },
    )
}

#[cfg(test)]
fn empty_equipment_registry() -> EquipmentRegistry {
    EquipmentRegistry::new(std::iter::empty())
}

#[cfg(test)]
fn empty_thermal_registry() -> ThermalRegistry {
    ThermalRegistry::new(std::iter::empty(), std::iter::empty(), std::iter::empty())
}

pub use energy::{
    ENERGY_ELECTRICAL_BUFFER, ENERGY_MECHANICAL_LARGE_DRIVE, ENERGY_MECHANICAL_SMALL_DRIVE,
    ENERGY_THERMAL_SINK,
};
pub use equipment::{EQUIPMENT_CASTING_MOLD, EQUIPMENT_ELECTRIC_FURNACE, EQUIPMENT_JAW_CRUSHER};
pub use materials::{
    FORM_CONCENTRATE, FORM_CRUSHED, FORM_INGOT, FORM_LOG, FORM_LUMP, FORM_MOLTEN, FORM_ORE,
    MATERIAL_CHARCOAL, MATERIAL_COPPER, MATERIAL_SLAG, MATERIAL_WOOD,
};
pub use processes::{PROCESS_CAST_PURE_COPPER, PROCESS_CRUSH_ORE, PROCESS_MELT_PURE_COPPER};
pub use structural::{STRUCTURAL_PROFILE_AXIAL_COMPRESSION, STRUCTURAL_PROFILE_AXIAL_TENSION};

const DEFAULT_TICKS_PER_SECOND: u16 = 20;
const DEFAULT_GRAVITY_MICROMETERS_PER_SECOND_SQUARED: u64 = 9_806_650;
const REGISTRY_SCHEMA_VERSION: RegistrySchemaVersion = RegistrySchemaVersion::new(14);

fn build_core_definitions() -> CoreDefinitions {
    CoreDefinitions::new(
        DEFAULT_TICKS_PER_SECOND,
        Acceleration::from_micrometers_per_second_squared(
            DEFAULT_GRAVITY_MICROMETERS_PER_SECOND_SQUARED,
        ),
    )
}

/// Builds the immutable built-in registry set used by a new application instance.
#[must_use]
pub fn build_registries() -> Registries {
    Registries::new(
        REGISTRY_SCHEMA_VERSION,
        build_core_definitions(),
        RegistryDomains {
            energy: energy::build_energy_registry(),
            fluid: fluid::build_fluid_registry(),
            capabilities: capabilities::build_capability_registry(),
            equipment: equipment::build_equipment_registry(),
            structural: structural::build_structural_registry(),
            materials: materials::build_material_registry(),
            ore_processing: ore_processing::build_ore_processing_registry(),
            thermal: thermal::build_thermal_registry(),
            production: processes::build_production_registry(),
        },
    )
}

#[cfg(test)]
pub(crate) fn make_test_registries_with_equipment(
    capability: CapabilityDefinition,
    equipment_definition: EquipmentDefinition,
) -> Registries {
    let mut capabilities = CapabilityRegistry::new();
    capabilities.register_capability(capability);
    Registries::new(
        REGISTRY_SCHEMA_VERSION,
        build_core_definitions(),
        RegistryDomains {
            energy: empty_energy_registry(),
            fluid: fluid::build_fluid_registry(),
            capabilities,
            equipment: EquipmentRegistry::new([equipment_definition]),
            structural: structural::build_structural_registry(),
            materials: materials::build_material_registry(),
            ore_processing: OreProcessingRegistry::new(std::iter::empty()),
            thermal: empty_thermal_registry(),
            production: ProductionRegistry::new(),
        },
    )
}

#[cfg(test)]
pub(crate) fn make_test_registries_with_process(process: ProcessDefinition) -> Registries {
    let mut production = ProductionRegistry::new();
    production.register_process_for_test(process);
    Registries::new(
        REGISTRY_SCHEMA_VERSION,
        build_core_definitions(),
        RegistryDomains {
            energy: empty_energy_registry(),
            fluid: fluid::build_fluid_registry(),
            capabilities: CapabilityRegistry::new(),
            equipment: empty_equipment_registry(),
            structural: structural::build_structural_registry(),
            materials: materials::build_material_registry(),
            ore_processing: OreProcessingRegistry::new(std::iter::empty()),
            thermal: empty_thermal_registry(),
            production,
        },
    )
}

#[cfg(test)]
pub(crate) fn make_test_registries_with_energy_store(
    definition: EnergyStoreDefinition,
) -> Registries {
    make_test_registries_with_energy_stores(vec![definition])
}

#[cfg(test)]
pub(crate) fn make_test_registries_with_energy_stores(
    definitions: Vec<EnergyStoreDefinition>,
) -> Registries {
    Registries::new(
        REGISTRY_SCHEMA_VERSION,
        build_core_definitions(),
        RegistryDomains {
            energy: EnergyRegistry::new(definitions),
            fluid: fluid::build_fluid_registry(),
            capabilities: CapabilityRegistry::new(),
            equipment: empty_equipment_registry(),
            structural: structural::build_structural_registry(),
            materials: materials::build_material_registry(),
            ore_processing: OreProcessingRegistry::new(std::iter::empty()),
            thermal: empty_thermal_registry(),
            production: ProductionRegistry::new(),
        },
    )
}

#[cfg(test)]
pub(crate) fn make_test_registries_with_energy_stores_and_process(
    definitions: Vec<EnergyStoreDefinition>,
    process: ProcessDefinition,
) -> Registries {
    let mut production = ProductionRegistry::new();
    production.register_process_for_test(process);
    Registries::new(
        REGISTRY_SCHEMA_VERSION,
        build_core_definitions(),
        RegistryDomains {
            energy: EnergyRegistry::new(definitions),
            fluid: fluid::build_fluid_registry(),
            capabilities: CapabilityRegistry::new(),
            equipment: empty_equipment_registry(),
            structural: structural::build_structural_registry(),
            materials: materials::build_material_registry(),
            ore_processing: OreProcessingRegistry::new(std::iter::empty()),
            thermal: empty_thermal_registry(),
            production,
        },
    )
}

#[cfg(test)]
pub(crate) fn make_test_registries_with_sensible_heating(
    capability_definitions: Vec<CapabilityDefinition>,
    equipment_definition: EquipmentDefinition,
    energy_definition: EnergyStoreDefinition,
    process: ProcessDefinition,
    thermal_definition: SensibleHeatingProcessDefinition,
) -> Registries {
    let mut capabilities = CapabilityRegistry::new();
    for capability in capability_definitions {
        capabilities.register_capability(capability);
    }
    let mut production = ProductionRegistry::new();
    production.register_process_for_test(process);
    Registries::new(
        REGISTRY_SCHEMA_VERSION,
        build_core_definitions(),
        RegistryDomains {
            energy: EnergyRegistry::new([energy_definition]),
            fluid: fluid::build_fluid_registry(),
            capabilities,
            equipment: EquipmentRegistry::new([equipment_definition]),
            structural: structural::build_structural_registry(),
            materials: materials::build_material_registry(),
            ore_processing: OreProcessingRegistry::new(std::iter::empty()),
            thermal: ThermalRegistry::new(
                [thermal_definition],
                std::iter::empty(),
                std::iter::empty(),
            ),
            production,
        },
    )
}

#[cfg(test)]
pub(crate) fn make_test_registries_with_melting(
    capability_definitions: Vec<CapabilityDefinition>,
    equipment_definition: EquipmentDefinition,
    energy_definition: EnergyStoreDefinition,
    process: ProcessDefinition,
    thermal_definition: MeltingProcessDefinition,
) -> Registries {
    let mut capabilities = CapabilityRegistry::new();
    for capability in capability_definitions {
        capabilities.register_capability(capability);
    }
    let mut production = ProductionRegistry::new();
    production.register_process_for_test(process);
    Registries::new(
        REGISTRY_SCHEMA_VERSION,
        build_core_definitions(),
        RegistryDomains {
            energy: EnergyRegistry::new([energy_definition]),
            fluid: fluid::build_fluid_registry(),
            capabilities,
            equipment: EquipmentRegistry::new([equipment_definition]),
            structural: structural::build_structural_registry(),
            materials: materials::build_material_registry(),
            ore_processing: OreProcessingRegistry::new(std::iter::empty()),
            thermal: ThermalRegistry::new(
                std::iter::empty(),
                [thermal_definition],
                std::iter::empty(),
            ),
            production,
        },
    )
}

#[cfg(test)]
pub(crate) fn make_test_registries_with_casting(
    capability_definitions: Vec<CapabilityDefinition>,
    equipment_definition: EquipmentDefinition,
    energy_definition: EnergyStoreDefinition,
    process: ProcessDefinition,
    thermal_definition: CastingProcessDefinition,
) -> Registries {
    let mut capabilities = CapabilityRegistry::new();
    for capability in capability_definitions {
        capabilities.register_capability(capability);
    }
    let mut production = ProductionRegistry::new();
    production.register_process_for_test(process);
    Registries::new(
        REGISTRY_SCHEMA_VERSION,
        build_core_definitions(),
        RegistryDomains {
            energy: EnergyRegistry::new([energy_definition]),
            fluid: fluid::build_fluid_registry(),
            capabilities,
            equipment: EquipmentRegistry::new([equipment_definition]),
            structural: structural::build_structural_registry(),
            materials: materials::build_material_registry(),
            ore_processing: OreProcessingRegistry::new(std::iter::empty()),
            thermal: ThermalRegistry::new(
                std::iter::empty(),
                std::iter::empty(),
                [thermal_definition],
            ),
            production,
        },
    )
}

#[cfg(test)]
pub(crate) fn make_test_registries_with_fluids(definitions: Vec<FluidDefinition>) -> Registries {
    Registries::new(
        REGISTRY_SCHEMA_VERSION,
        build_core_definitions(),
        RegistryDomains {
            energy: empty_energy_registry(),
            fluid: FluidRegistry::new(definitions),
            capabilities: CapabilityRegistry::new(),
            equipment: empty_equipment_registry(),
            structural: structural::build_structural_registry(),
            materials: materials::build_material_registry(),
            ore_processing: OreProcessingRegistry::new(std::iter::empty()),
            thermal: empty_thermal_registry(),
            production: ProductionRegistry::new(),
        },
    )
}

#[cfg(test)]
pub(crate) fn make_test_registries_with_comminution(
    capability_definitions: Vec<CapabilityDefinition>,
    equipment_definition: EquipmentDefinition,
    energy_definition: EnergyStoreDefinition,
    process: ProcessDefinition,
    comminution_definition: ComminutionProcessDefinition,
) -> Registries {
    let mut capabilities = CapabilityRegistry::new();
    for capability in capability_definitions {
        capabilities.register_capability(capability);
    }
    let mut production = ProductionRegistry::new();
    production.register_process_for_test(process);
    Registries::new(
        REGISTRY_SCHEMA_VERSION,
        build_core_definitions(),
        RegistryDomains {
            energy: EnergyRegistry::new([energy_definition]),
            fluid: fluid::build_fluid_registry(),
            capabilities,
            equipment: EquipmentRegistry::new([equipment_definition]),
            structural: structural::build_structural_registry(),
            materials: materials::build_material_registry(),
            ore_processing: OreProcessingRegistry::new([comminution_definition]),
            thermal: empty_thermal_registry(),
            production,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{
        CapabilityComparison, CapabilityDefinition, CapabilityId, CapabilityRegistry,
        CapabilityRequirement, CapabilityValue, CapabilityValueKind,
    };
    use crate::core::quantity::{Length, Mass, MassSpecificEnergy, Temperature};
    use crate::energy::EnergyCarrier;
    use crate::material::{CommodityKey, MaterialInputSpec, ParticleSizeRange};
    use crate::ore_processing::{ComminutionOperatingProfile, ComminutionProcessDefinition};
    use crate::production::{ProcessDefinition, ProcessId, ProductionRegistry};
    use crate::thermal::{SensibleHeatingProcessDefinition, ThermalRegistry};

    const TEST_CAPABILITY: CapabilityId = CapabilityId::new(700_001);
    const TEST_PROCESS: ProcessId = ProcessId::new(700_001);
    const TEST_MASS_FLOW: CapabilityId = CapabilityId::new(700_002);
    const TEST_MAX_BATCH_MASS: CapabilityId = CapabilityId::new(700_003);
    const TEST_HEATING_POWER: CapabilityId = CapabilityId::new(700_004);
    const TEST_MAX_TEMPERATURE: CapabilityId = CapabilityId::new(700_005);

    #[test]
    fn built_in_tick_rate_is_nonzero_and_stable() {
        let registries = build_registries();

        assert_eq!(registries.core().ticks_per_second().get(), 20);
        assert_eq!(
            registries.core().gravity().micrometers_per_second_squared(),
            DEFAULT_GRAVITY_MICROMETERS_PER_SECOND_SQUARED
        );
    }

    #[test]
    fn built_in_workshop_ids_resolve_canonical_gameplay_content() {
        let registries = build_registries();

        for equipment in [
            EQUIPMENT_JAW_CRUSHER,
            EQUIPMENT_ELECTRIC_FURNACE,
            EQUIPMENT_CASTING_MOLD,
        ] {
            assert!(registries.equipment().get_equipment(equipment).is_some());
        }
        for energy in [
            ENERGY_MECHANICAL_SMALL_DRIVE,
            ENERGY_MECHANICAL_LARGE_DRIVE,
            ENERGY_ELECTRICAL_BUFFER,
            ENERGY_THERMAL_SINK,
        ] {
            assert!(registries.energy().get_store(energy).is_some());
        }
        for process in [
            PROCESS_CRUSH_ORE,
            PROCESS_MELT_PURE_COPPER,
            PROCESS_CAST_PURE_COPPER,
        ] {
            assert!(registries.production().get_process(process).is_some());
        }
        assert!(
            registries
                .ore_processing()
                .get_comminution(PROCESS_CRUSH_ORE)
                .is_some()
        );
        assert!(
            registries
                .thermal()
                .get_melting(PROCESS_MELT_PURE_COPPER)
                .is_some()
        );
        assert!(
            registries
                .thermal()
                .get_casting(PROCESS_CAST_PURE_COPPER)
                .is_some()
        );
    }

    #[test]
    fn process_capability_references_are_validated_during_registry_assembly() {
        let mut capabilities = CapabilityRegistry::new();
        capabilities.register_capability(CapabilityDefinition::new(
            TEST_CAPABILITY,
            "test chamber temperature",
            CapabilityValueKind::Temperature,
        ));
        let process = ProcessDefinition::new(
            TEST_PROCESS,
            "test capability process",
            vec![MaterialInputSpec::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
                Mass::from_milligrams(1),
            )],
            vec![CapabilityRequirement::new(
                TEST_CAPABILITY,
                CapabilityComparison::AtLeast,
                CapabilityValue::Temperature(Temperature::from_millikelvin(500_000)),
            )],
        );
        let mut production = ProductionRegistry::new();
        production.register_process_for_test(process);

        let registries = Registries::new(
            REGISTRY_SCHEMA_VERSION,
            build_core_definitions(),
            RegistryDomains {
                energy: empty_energy_registry(),
                fluid: fluid::build_fluid_registry(),
                capabilities,
                equipment: empty_equipment_registry(),
                structural: structural::build_structural_registry(),
                materials: materials::build_material_registry(),
                ore_processing: OreProcessingRegistry::new(std::iter::empty()),
                thermal: empty_thermal_registry(),
                production,
            },
        );

        assert!(
            registries
                .capabilities()
                .get_capability(TEST_CAPABILITY)
                .is_some()
        );
        assert!(registries.production().get_process(TEST_PROCESS).is_some());
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
        let process = ProcessDefinition::new_selected_batch(
            TEST_PROCESS,
            "ambiguous physical resolver fixture",
            Vec::new(),
        );
        let mut production = ProductionRegistry::new();
        production.register_process_for_test(process);
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
            ComminutionOperatingProfile::new(
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
                RegistryDomains {
                    energy: empty_energy_registry(),
                    fluid: fluid::build_fluid_registry(),
                    capabilities,
                    equipment: empty_equipment_registry(),
                    structural: structural::build_structural_registry(),
                    materials: materials::build_material_registry(),
                    ore_processing,
                    thermal,
                    production,
                },
            )
        });

        assert!(result.is_err());
    }
}
