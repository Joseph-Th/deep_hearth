//! Built-in immutable game definitions; sibling files own authored definitions for each registry domain.

mod capabilities;
mod energy;
mod equipment;
mod fluid;
mod materials;
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
use crate::production::{ProcessDefinition, ProductionRegistry};
#[cfg(test)]
use crate::thermal::{
    CastingProcessDefinition, MeltingProcessDefinition, SensibleHeatingProcessDefinition,
    ThermalRegistry,
};

pub use materials::{
    FORM_CONCENTRATE, FORM_INGOT, FORM_LOG, FORM_LUMP, FORM_MOLTEN, FORM_ORE, MATERIAL_CHARCOAL,
    MATERIAL_COPPER, MATERIAL_SLAG, MATERIAL_WOOD,
};
pub use structural::{STRUCTURAL_PROFILE_AXIAL_COMPRESSION, STRUCTURAL_PROFILE_AXIAL_TENSION};

const DEFAULT_TICKS_PER_SECOND: u16 = 20;
const DEFAULT_GRAVITY_MICROMETERS_PER_SECOND_SQUARED: u64 = 9_806_650;
const REGISTRY_SCHEMA_VERSION: RegistrySchemaVersion = RegistrySchemaVersion::new(10);

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
            energy: energy::build_energy_registry(),
            fluid: fluid::build_fluid_registry(),
            capabilities,
            equipment: EquipmentRegistry::new([equipment_definition]),
            structural: structural::build_structural_registry(),
            materials: materials::build_material_registry(),
            thermal: thermal::build_thermal_registry(),
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
            energy: energy::build_energy_registry(),
            fluid: fluid::build_fluid_registry(),
            capabilities: capabilities::build_capability_registry(),
            equipment: equipment::build_equipment_registry(),
            structural: structural::build_structural_registry(),
            materials: materials::build_material_registry(),
            thermal: thermal::build_thermal_registry(),
            production,
        },
    )
}

#[cfg(test)]
pub(crate) fn make_test_registries_with_energy_store(
    definition: EnergyStoreDefinition,
) -> Registries {
    Registries::new(
        REGISTRY_SCHEMA_VERSION,
        build_core_definitions(),
        RegistryDomains {
            energy: EnergyRegistry::new([definition]),
            fluid: fluid::build_fluid_registry(),
            capabilities: capabilities::build_capability_registry(),
            equipment: equipment::build_equipment_registry(),
            structural: structural::build_structural_registry(),
            materials: materials::build_material_registry(),
            thermal: thermal::build_thermal_registry(),
            production: ProductionRegistry::new(),
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
            energy: energy::build_energy_registry(),
            fluid: FluidRegistry::new(definitions),
            capabilities: capabilities::build_capability_registry(),
            equipment: equipment::build_equipment_registry(),
            structural: structural::build_structural_registry(),
            materials: materials::build_material_registry(),
            thermal: thermal::build_thermal_registry(),
            production: processes::build_production_registry(),
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
    use crate::core::quantity::{Mass, Temperature};
    use crate::material::{CommodityKey, MaterialInputSpec};
    use crate::production::{ProcessDefinition, ProcessId, ProductionRegistry};

    const TEST_CAPABILITY: CapabilityId = CapabilityId::new(700_001);
    const TEST_PROCESS: ProcessId = ProcessId::new(700_001);

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
                energy: energy::build_energy_registry(),
                fluid: fluid::build_fluid_registry(),
                capabilities,
                equipment: equipment::build_equipment_registry(),
                structural: structural::build_structural_registry(),
                materials: materials::build_material_registry(),
                thermal: thermal::build_thermal_registry(),
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
}
