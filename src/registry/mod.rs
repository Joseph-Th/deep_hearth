//! Immutable definition aggregate loaded once and passed explicitly to simulation systems.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::capability::CapabilityRegistry;
use crate::core::quantity::Acceleration;
use crate::core::time::{CalendarDefinition, PhysicalTickDuration};
use crate::crafting::CraftingRegistry;
use crate::energy::EnergyRegistry;
use crate::equipment::EquipmentRegistry;
use crate::fluid::FluidRegistry;
use crate::inventory::StorageRegistry;
use crate::labor::LaborRegistry;
use crate::material::{MaterialAssemblyProfile, MaterialRegistry};
use crate::mining::MiningRegistry;
use crate::ore_processing::OreProcessingRegistry;
use crate::production::{ProcessId, ProductionRegistry};
use crate::shader::ShaderRegistry;
use crate::structural::StructuralRegistry;
use crate::survival::{SurvivalExertion, SurvivalRegistry};
use crate::texture::TextureRegistry;
use crate::thermal::ThermalRegistry;

mod process_topology;

#[cfg(test)]
#[path = "process_topology_tests.rs"]
mod process_topology_tests;

#[cfg(test)]
use process_topology::build_partial_process_topology_for_owner_tests;
use process_topology::build_process_topology;
pub use process_topology::{ProcessEnergyRole, ProcessExecutionFamily, ProcessTopology};

/// Schema version for stable authored registry identities and cross-reference semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RegistrySchemaVersion(u32);

impl RegistrySchemaVersion {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        assert!(value != 0, "registry schema version must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Domain-neutral immutable definitions needed by the simulation core.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreDefinitions {
    gravity: Acceleration,
    calendar: CalendarDefinition,
}

impl CoreDefinitions {
    pub(crate) const fn new(gravity: Acceleration, calendar: CalendarDefinition) -> Self {
        Self { gravity, calendar }
    }

    /// Returns the physical world-time represented by one authoritative simulation tick.
    #[must_use]
    pub const fn physical_tick_duration(&self) -> PhysicalTickDuration {
        self.calendar.physical_tick_duration()
    }

    /// Returns the authored world gravitational acceleration used by weight-bearing systems.
    #[must_use]
    pub const fn gravity(&self) -> Acceleration {
        self.gravity
    }

    /// Returns the immutable calendar used to project ticks into days and seasons.
    #[must_use]
    pub const fn calendar(&self) -> CalendarDefinition {
        self.calendar
    }
}

/// Root immutable registry set for all authored static definitions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Registries {
    schema_version: RegistrySchemaVersion,
    core: CoreDefinitions,
    domains: RegistryDomains,
    process_topology: BTreeMap<ProcessId, ProcessTopology>,
}

/// Domain registry bundle used to assemble the immutable root without a wide positional API.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RegistryDomains {
    pub(crate) energy: EnergyRegistry,
    pub(crate) fluid: FluidRegistry,
    pub(crate) capabilities: CapabilityRegistry,
    pub(crate) crafting: CraftingRegistry,
    pub(crate) labor: LaborRegistry,
    pub(crate) equipment: EquipmentRegistry,
    pub(crate) storage: StorageRegistry,
    pub(crate) structural: StructuralRegistry,
    pub(crate) materials: MaterialRegistry,
    pub(crate) mining: MiningRegistry,
    pub(crate) ore_processing: OreProcessingRegistry,
    pub(crate) thermal: ThermalRegistry,
    pub(crate) production: ProductionRegistry,
    pub(crate) survival: SurvivalRegistry,
    pub(crate) presentation: RegistryPresentation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RegistryPresentation {
    pub(crate) textures: TextureRegistry,
    pub(crate) shaders: ShaderRegistry,
}

fn assert_nonperishable_infrastructure_assembly(
    owner: &str,
    assembly: &MaterialAssemblyProfile,
    survival: &SurvivalRegistry,
) {
    for input in assembly.inputs() {
        assert!(
            !survival.has_food_material(input.commodity().material()),
            "{owner} cannot embody material {} because that material has an authored edible form and embodied infrastructure does not track storage age",
            input.commodity().material().value()
        );
    }
}

fn validate_energy_infrastructure_perishability(
    energy: &EnergyRegistry,
    survival: &SurvivalRegistry,
) {
    for definition in energy.definitions() {
        if let Some(assembly) = definition.assembly_profile() {
            assert_nonperishable_infrastructure_assembly(
                "energy-store assembly",
                assembly,
                survival,
            );
        }
        if let Some(upgrade) = definition.upgrade_profile() {
            assert_nonperishable_infrastructure_assembly(
                "energy-store upgrade",
                upgrade.additions(),
                survival,
            );
        }
    }
}

fn validate_equipment_infrastructure_perishability(
    equipment: &EquipmentRegistry,
    survival: &SurvivalRegistry,
) {
    for definition in equipment.definitions() {
        if let Some(assembly) = definition.assembly_profile() {
            assert_nonperishable_infrastructure_assembly("equipment assembly", assembly, survival);
        }
        if let Some(upgrade) = definition.upgrade_profile() {
            assert_nonperishable_infrastructure_assembly(
                "equipment upgrade",
                upgrade.additions(),
                survival,
            );
        }
    }
}

fn validate_storage_infrastructure_perishability(
    storage: &StorageRegistry,
    survival: &SurvivalRegistry,
) {
    for definition in storage.definitions() {
        assert_nonperishable_infrastructure_assembly(
            "storage-enclosure assembly",
            definition.assembly_profile(),
            survival,
        );
    }
}

fn validate_infrastructure_perishability(domains: &RegistryDomains) {
    validate_energy_infrastructure_perishability(&domains.energy, &domains.survival);
    validate_equipment_infrastructure_perishability(&domains.equipment, &domains.survival);
    validate_storage_infrastructure_perishability(&domains.storage, &domains.survival);
}

fn validate_registry_domains(core: &CoreDefinitions, domains: &RegistryDomains) {
    domains
        .energy
        .validate_references(&domains.materials, core.physical_tick_duration());
    domains.fluid.validate_references(&domains.materials);
    domains
        .crafting
        .validate_references(&domains.production, &domains.materials);
    domains.labor.validate_references(&domains.capabilities);
    domains
        .equipment
        .validate_references(&domains.capabilities, &domains.materials);
    domains.storage.validate_references(&domains.materials);
    domains
        .production
        .validate_references(&domains.materials, &domains.capabilities);
    domains.mining.validate_references(&domains.capabilities);
    domains.ore_processing.validate_references(
        &domains.production,
        &domains.capabilities,
        &domains.materials,
    );
    domains
        .survival
        .validate_references(&domains.materials, &domains.fluid);
    validate_infrastructure_perishability(domains);
    domains.thermal.validate_references(
        &domains.production,
        &domains.capabilities,
        &domains.materials,
    );
    domains
        .presentation
        .textures
        .validate_references(&domains.materials, &domains.equipment);
}

impl Registries {
    pub(crate) fn new(
        schema_version: RegistrySchemaVersion,
        core: CoreDefinitions,
        domains: RegistryDomains,
    ) -> Self {
        validate_registry_domains(&core, &domains);
        let process_topology = build_process_topology(&domains);
        Self {
            schema_version,
            core,
            domains,
            process_topology,
        }
    }

    /// Assembles registries for isolated tests of the generic production owner without requiring a
    /// gameplay resolver family for the synthetic process definition.
    #[cfg(test)]
    pub(crate) fn new_for_isolated_production_owner_test(
        schema_version: RegistrySchemaVersion,
        core: CoreDefinitions,
        domains: RegistryDomains,
    ) -> Self {
        validate_registry_domains(&core, &domains);
        let process_topology = build_partial_process_topology_for_owner_tests(&domains);
        Self {
            schema_version,
            core,
            domains,
            process_topology,
        }
    }

    /// Returns the authored-ID schema version required by persisted runtime references.
    #[must_use]
    pub const fn schema_version(&self) -> RegistrySchemaVersion {
        self.schema_version
    }

    /// Returns immutable core definitions.
    #[must_use]
    pub const fn core(&self) -> &CoreDefinitions {
        &self.core
    }

    /// Returns immutable finite-energy store definitions.
    #[must_use]
    pub const fn energy(&self) -> &EnergyRegistry {
        &self.domains.energy
    }

    /// Returns immutable authored fluid identities.
    #[must_use]
    pub const fn fluid(&self) -> &FluidRegistry {
        &self.domains.fluid
    }

    /// Returns immutable authored physical/tool/equipment capability definitions.
    #[must_use]
    pub const fn capabilities(&self) -> &CapabilityRegistry {
        &self.domains.capabilities
    }

    /// Returns immutable manual shaping definitions.
    #[must_use]
    pub const fn crafting(&self) -> &CraftingRegistry {
        &self.domains.crafting
    }

    /// Returns immutable direct player-labor conversion definitions.
    #[must_use]
    pub const fn labor(&self) -> &LaborRegistry {
        &self.domains.labor
    }

    /// Returns immutable maintainable equipment definitions.
    #[must_use]
    pub const fn equipment(&self) -> &EquipmentRegistry {
        &self.domains.equipment
    }

    /// Returns immutable constructible material-storage enclosure definitions.
    #[must_use]
    pub const fn storage(&self) -> &StorageRegistry {
        &self.domains.storage
    }

    /// Returns immutable structural load-response profiles.
    #[must_use]
    pub const fn structural(&self) -> &StructuralRegistry {
        &self.domains.structural
    }

    /// Returns immutable material and physical-form definitions.
    #[must_use]
    pub const fn materials(&self) -> &MaterialRegistry {
        &self.domains.materials
    }

    /// Returns immutable physical mining method definitions.
    #[must_use]
    pub const fn mining(&self) -> &MiningRegistry {
        &self.domains.mining
    }

    /// Returns immutable physical ore/material-preparation resolver definitions.
    #[must_use]
    pub const fn ore_processing(&self) -> &OreProcessingRegistry {
        &self.domains.ore_processing
    }

    /// Returns immutable physical thermal-process resolution semantics.
    #[must_use]
    pub const fn thermal(&self) -> &ThermalRegistry {
        &self.domains.thermal
    }

    /// Returns immutable material-transformation definitions.
    #[must_use]
    pub const fn production(&self) -> &ProductionRegistry {
        &self.domains.production
    }

    /// Returns immutable authored execution/provider/energy topology for one process.
    ///
    /// Runtime registry assembly guarantees one topology for every authored process, so absence
    /// means the process identifier is not registered. The returned relationships describe
    /// definition-level possibilities, not current reachability or authorization.
    #[must_use]
    pub fn process_topology(&self, process: ProcessId) -> Option<&ProcessTopology> {
        self.process_topology.get(&process)
    }

    /// Returns immutable physiology and edible/drinkable definitions.
    #[must_use]
    pub const fn survival(&self) -> &SurvivalRegistry {
        &self.domains.survival
    }

    /// Returns the exact exertion for a production process that directly monopolizes player labor.
    ///
    /// Crafting and low-tech material processing remain separate domain owners while labor,
    /// persistence, and production can consume one canonical classification.
    #[must_use]
    pub(crate) fn manual_process_exertion(&self, process: ProcessId) -> Option<SurvivalExertion> {
        self.crafting()
            .get_manual(process)
            .map(|definition| definition.exertion())
            .or_else(|| {
                self.ore_processing()
                    .get_manual_comminution(process)
                    .map(|definition| definition.exertion())
            })
            .or_else(|| {
                self.ore_processing()
                    .get_manual_constituent_separation(process)
                    .map(|definition| definition.exertion())
            })
    }

    /// Returns immutable palette, texture, and block/object appearance definitions.
    #[must_use]
    pub const fn textures(&self) -> &TextureRegistry {
        &self.domains.presentation.textures
    }

    /// Returns immutable WGSL libraries and executable shader definitions.
    #[must_use]
    pub const fn shaders(&self) -> &ShaderRegistry {
        &self.domains.presentation.shaders
    }
}
