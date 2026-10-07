//! Derived definition-level commodity sources and uses for player-facing handbook views.

use crate::core::quantity::Mass;
use crate::energy::EnergyStoreDefinitionId;
use crate::equipment::EquipmentDefinitionId;
use crate::inventory::StorageDefinitionId;
use crate::labor::SurfaceGatheringMethodId;
use crate::material::{CommodityKey, FormDefinition, MaterialDefinition};
use crate::production::ProcessId;

use super::Registries;

mod infrastructure;
mod processing;

use infrastructure::{
    collect_energy_relationships, collect_equipment_relationships, collect_storage_relationships,
};
use processing::{
    collect_crafting_relationships, collect_ore_processing_relationships,
    collect_separation_relationships, collect_thermal_relationships,
};

/// Runtime condition under which an equipment-disassembly source is available.
///
/// Pristine disassembly returns the authored assembly commodity unchanged. Worn component
/// disassembly is available only for component-maintained equipment and reforms the complete worn
/// component into its authored spent commodity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EquipmentDisassemblyRecovery {
    PristineExact,
    WornComponentSpent,
}

/// One authored way a commodity can enter inventory custody.
///
/// These are definition-level relationships, not claims that the source is currently reachable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommoditySource {
    SurfaceGathering {
        method: SurfaceGatheringMethodId,
        maximum_batch_mass: Mass,
    },
    ManualCraft {
        process: ProcessId,
        output_mass: Mass,
    },
    EquipmentDisassembly {
        equipment: EquipmentDefinitionId,
        recovered_mass: Mass,
        recovery: EquipmentDisassemblyRecovery,
    },
    EnergyStoreDisassembly {
        store: EnergyStoreDefinitionId,
        recovered_mass: Mass,
    },
    StorageDismantling {
        storage: StorageDefinitionId,
        recovered_mass: Mass,
    },
    EquipmentMaintenanceSpent {
        equipment: EquipmentDefinitionId,
        full_service_mass: Mass,
    },
    OreProcessing {
        process: ProcessId,
    },
    ThermalPhaseChange {
        process: ProcessId,
    },
}

/// One authored definition-level sink for a commodity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommodityUse {
    ManualCraft {
        process: ProcessId,
        required_mass: Mass,
    },
    EquipmentAssembly {
        equipment: EquipmentDefinitionId,
        required_mass: Mass,
    },
    EquipmentUpgrade {
        from: EquipmentDefinitionId,
        to: EquipmentDefinitionId,
        required_mass: Mass,
    },
    EquipmentMaintenance {
        equipment: EquipmentDefinitionId,
        full_service_mass: Mass,
    },
    StorageConstruction {
        storage: StorageDefinitionId,
        required_mass: Mass,
    },
    EnergyStoreAssembly {
        store: EnergyStoreDefinitionId,
        required_mass: Mass,
    },
    EnergyStoreUpgrade {
        from: EnergyStoreDefinitionId,
        to: EnergyStoreDefinitionId,
        required_mass: Mass,
    },
    OreProcessing {
        process: ProcessId,
    },
    ThermalPhaseChange {
        process: ProcessId,
    },
}

/// Derived handbook entry for one exact material/form commodity.
///
/// Names remain references to canonical material/form definitions. Sources and uses are rebuilt
/// from authored registries on demand rather than stored as a parallel recipe database.
#[derive(Debug)]
pub struct CommodityHandbookEntry<'a> {
    commodity: CommodityKey,
    name: &'a str,
    material: &'a MaterialDefinition,
    form: &'a FormDefinition,
    sources: Vec<CommoditySource>,
    uses: Vec<CommodityUse>,
}

impl<'a> CommodityHandbookEntry<'a> {
    #[must_use]
    pub const fn commodity(&self) -> CommodityKey {
        self.commodity
    }

    #[must_use]
    pub const fn name(&self) -> &'a str {
        self.name
    }

    #[must_use]
    pub const fn material(&self) -> &'a MaterialDefinition {
        self.material
    }

    #[must_use]
    pub const fn form(&self) -> &'a FormDefinition {
        self.form
    }

    #[must_use]
    pub fn sources(&self) -> &[CommoditySource] {
        &self.sources
    }

    #[must_use]
    pub fn uses(&self) -> &[CommodityUse] {
        &self.uses
    }
}

impl Registries {
    /// Builds the contextual "how do I get this / what is it used for" catalog entry for one
    /// authored commodity. The result describes authored topology only; runtime reachability,
    /// inventory sufficiency, equipment condition, knowledge, and world access remain separate.
    #[must_use]
    pub fn commodity_handbook_entry(
        &self,
        commodity: CommodityKey,
    ) -> Option<CommodityHandbookEntry<'_>> {
        if !self.materials().has_commodity(commodity) {
            return None;
        }
        let name = self
            .materials()
            .commodity_name(commodity)
            .unwrap_or_else(|| unreachable!("authored commodity must have a player-facing name"));
        let material = self
            .materials()
            .get_material(commodity.material())
            .unwrap_or_else(|| unreachable!("authored commodity must reference known material"));
        let form = self
            .materials()
            .get_form(commodity.form())
            .unwrap_or_else(|| unreachable!("authored commodity must reference known form"));

        let mut sources = Vec::new();
        let mut uses = Vec::new();

        sources.extend(
            self.labor()
                .surface_gathering_definitions()
                .filter(|definition| definition.commodity() == commodity)
                .map(|definition| CommoditySource::SurfaceGathering {
                    method: definition.id(),
                    maximum_batch_mass: definition.maximum_batch_mass(),
                }),
        );
        collect_crafting_relationships(self, commodity, &mut sources, &mut uses);
        collect_ore_processing_relationships(self, commodity, &mut sources, &mut uses);
        collect_separation_relationships(self, commodity, &mut sources, &mut uses);
        collect_thermal_relationships(self, commodity, &mut sources, &mut uses);
        collect_equipment_relationships(self, commodity, &mut sources, &mut uses);
        collect_storage_relationships(self, commodity, &mut sources, &mut uses);
        collect_energy_relationships(self, commodity, &mut sources, &mut uses);

        Some(CommodityHandbookEntry {
            commodity,
            name,
            material,
            form,
            sources,
            uses,
        })
    }
}

#[cfg(test)]
#[path = "commodity_handbook_tests.rs"]
mod tests;
