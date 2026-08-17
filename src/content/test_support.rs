//! Test-only registry assembly isolated from unrelated built-in runtime content.

use super::{REGISTRY_SCHEMA_VERSION, build_core_definitions, materials, structural};
use crate::capability::{CapabilityDefinition, CapabilityRegistry};
use crate::crafting::CraftingRegistry;
use crate::energy::{EnergyRegistry, EnergyStoreDefinition};
use crate::equipment::{EquipmentDefinition, EquipmentRegistry};
use crate::fluid::{FluidDefinition, FluidRegistry};
use crate::mining::MiningRegistry;
use crate::ore_processing::{
    ComminutionProcessDefinition, OreProcessingRegistry, ScreeningProcessDefinition,
};
use crate::production::{ProcessDefinition, ProductionRegistry};
use crate::registry::{Registries, RegistryDomains, RegistryPresentation};
use crate::shader::ShaderRegistry;
use crate::survival::SurvivalRegistry;
use crate::texture::TextureRegistry;
use crate::thermal::{
    CastingProcessDefinition, MeltingProcessDefinition, SensibleHeatingProcessDefinition,
    ThermalRegistry,
};

struct TestRegistryDomains {
    energy: EnergyRegistry,
    fluid: FluidRegistry,
    capabilities: CapabilityRegistry,
    crafting: CraftingRegistry,
    equipment: EquipmentRegistry,
    mining: MiningRegistry,
    ore_processing: OreProcessingRegistry,
    thermal: ThermalRegistry,
    production: ProductionRegistry,
    survival: SurvivalRegistry,
}

impl TestRegistryDomains {
    fn empty() -> Self {
        Self {
            energy: empty_energy_registry(),
            fluid: FluidRegistry::new(std::iter::empty()),
            capabilities: CapabilityRegistry::new(),
            crafting: CraftingRegistry::new(std::iter::empty()),
            equipment: empty_equipment_registry(),
            mining: MiningRegistry::new(std::iter::empty()),
            ore_processing: OreProcessingRegistry::new(std::iter::empty()),
            thermal: empty_thermal_registry(),
            production: ProductionRegistry::new(),
            survival: super::survival::build_test_survival_registry(),
        }
    }

    fn build(self) -> Registries {
        let Self {
            energy,
            fluid,
            capabilities,
            crafting,
            equipment,
            mining,
            ore_processing,
            thermal,
            production,
            survival,
        } = self;
        Registries::new(
            REGISTRY_SCHEMA_VERSION,
            build_core_definitions(),
            RegistryDomains {
                energy,
                fluid,
                capabilities,
                crafting,
                equipment,
                structural: structural::build_structural_registry(),
                materials: materials::build_material_registry(),
                mining,
                ore_processing,
                thermal,
                production,
                survival,
                presentation: RegistryPresentation {
                    textures: empty_texture_registry(),
                    shaders: empty_shader_registry(),
                },
            },
        )
    }
}

pub(super) fn empty_energy_registry() -> EnergyRegistry {
    EnergyRegistry::new(std::iter::empty())
}

pub(super) fn empty_equipment_registry() -> EquipmentRegistry {
    EquipmentRegistry::new(std::iter::empty())
}

pub(super) fn empty_thermal_registry() -> ThermalRegistry {
    ThermalRegistry::new(std::iter::empty(), std::iter::empty(), std::iter::empty())
}

pub(super) fn empty_texture_registry() -> TextureRegistry {
    TextureRegistry::empty()
}

pub(super) fn empty_shader_registry() -> ShaderRegistry {
    ShaderRegistry::empty()
}

fn build_capability_registry(
    definitions: impl IntoIterator<Item = CapabilityDefinition>,
) -> CapabilityRegistry {
    let mut registry = CapabilityRegistry::new();
    for definition in definitions {
        registry.register_capability(definition);
    }
    registry
}

fn build_production_registry(process: ProcessDefinition) -> ProductionRegistry {
    let mut registry = ProductionRegistry::new();
    registry.register_process_for_test(process);
    registry
}

pub(crate) fn make_test_registries_with_screening(
    capability_definitions: Vec<CapabilityDefinition>,
    equipment_definition: EquipmentDefinition,
    energy_definition: EnergyStoreDefinition,
    process: ProcessDefinition,
    screening_definition: ScreeningProcessDefinition,
) -> Registries {
    let mut domains = TestRegistryDomains::empty();
    domains.capabilities = build_capability_registry(capability_definitions);
    domains.equipment = EquipmentRegistry::new([equipment_definition]);
    domains.energy = EnergyRegistry::new([energy_definition]);
    domains.production = build_production_registry(process);
    domains.ore_processing =
        OreProcessingRegistry::new_with_screening(std::iter::empty(), [screening_definition]);
    domains.build()
}

pub(crate) fn make_test_registries_with_equipment(
    capability: CapabilityDefinition,
    equipment_definition: EquipmentDefinition,
) -> Registries {
    let mut domains = TestRegistryDomains::empty();
    domains.capabilities = build_capability_registry([capability]);
    domains.equipment = EquipmentRegistry::new([equipment_definition]);
    domains.build()
}

pub(crate) fn make_test_registries_with_process(process: ProcessDefinition) -> Registries {
    let mut domains = TestRegistryDomains::empty();
    domains.production = build_production_registry(process);
    domains.build()
}

pub(crate) fn make_test_registries_with_energy_store(
    definition: EnergyStoreDefinition,
) -> Registries {
    make_test_registries_with_energy_stores(vec![definition])
}

pub(crate) fn make_test_registries_with_energy_stores(
    definitions: Vec<EnergyStoreDefinition>,
) -> Registries {
    let mut domains = TestRegistryDomains::empty();
    domains.energy = EnergyRegistry::new(definitions);
    domains.build()
}

pub(crate) fn make_test_registries_with_energy_stores_and_process(
    definitions: Vec<EnergyStoreDefinition>,
    process: ProcessDefinition,
) -> Registries {
    let mut domains = TestRegistryDomains::empty();
    domains.energy = EnergyRegistry::new(definitions);
    domains.production = build_production_registry(process);
    domains.build()
}

pub(crate) fn make_test_registries_with_sensible_heating(
    capability_definitions: Vec<CapabilityDefinition>,
    equipment_definition: EquipmentDefinition,
    energy_definition: EnergyStoreDefinition,
    process: ProcessDefinition,
    thermal_definition: SensibleHeatingProcessDefinition,
) -> Registries {
    let mut domains = TestRegistryDomains::empty();
    domains.capabilities = build_capability_registry(capability_definitions);
    domains.equipment = EquipmentRegistry::new([equipment_definition]);
    domains.energy = EnergyRegistry::new([energy_definition]);
    domains.production = build_production_registry(process);
    domains.thermal =
        ThermalRegistry::new([thermal_definition], std::iter::empty(), std::iter::empty());
    domains.build()
}

pub(crate) fn make_test_registries_with_melting(
    capability_definitions: Vec<CapabilityDefinition>,
    equipment_definition: EquipmentDefinition,
    energy_definition: EnergyStoreDefinition,
    process: ProcessDefinition,
    thermal_definition: MeltingProcessDefinition,
) -> Registries {
    let mut domains = TestRegistryDomains::empty();
    domains.capabilities = build_capability_registry(capability_definitions);
    domains.equipment = EquipmentRegistry::new([equipment_definition]);
    domains.energy = EnergyRegistry::new([energy_definition]);
    domains.production = build_production_registry(process);
    domains.thermal =
        ThermalRegistry::new(std::iter::empty(), [thermal_definition], std::iter::empty());
    domains.build()
}

pub(crate) fn make_test_registries_with_casting(
    capability_definitions: Vec<CapabilityDefinition>,
    equipment_definition: EquipmentDefinition,
    energy_definition: EnergyStoreDefinition,
    process: ProcessDefinition,
    thermal_definition: CastingProcessDefinition,
) -> Registries {
    let mut domains = TestRegistryDomains::empty();
    domains.capabilities = build_capability_registry(capability_definitions);
    domains.equipment = EquipmentRegistry::new([equipment_definition]);
    domains.energy = EnergyRegistry::new([energy_definition]);
    domains.production = build_production_registry(process);
    domains.thermal =
        ThermalRegistry::new(std::iter::empty(), std::iter::empty(), [thermal_definition]);
    domains.build()
}

pub(crate) fn make_test_registries_with_fluids(definitions: Vec<FluidDefinition>) -> Registries {
    let mut domains = TestRegistryDomains::empty();
    domains.fluid = FluidRegistry::new(definitions);
    domains.build()
}

pub(crate) fn make_test_registries_with_comminution(
    capability_definitions: Vec<CapabilityDefinition>,
    equipment_definition: EquipmentDefinition,
    energy_definition: EnergyStoreDefinition,
    process: ProcessDefinition,
    comminution_definition: ComminutionProcessDefinition,
) -> Registries {
    let mut domains = TestRegistryDomains::empty();
    domains.capabilities = build_capability_registry(capability_definitions);
    domains.equipment = EquipmentRegistry::new([equipment_definition]);
    domains.energy = EnergyRegistry::new([energy_definition]);
    domains.production = build_production_registry(process);
    domains.ore_processing = OreProcessingRegistry::new([comminution_definition]);
    domains.build()
}
