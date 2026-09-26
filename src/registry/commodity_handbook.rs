//! Derived definition-level commodity sources and uses for player-facing handbook views.

use crate::core::quantity::Mass;
use crate::energy::EnergyStoreDefinitionId;
use crate::equipment::EquipmentDefinitionId;
use crate::inventory::StorageDefinitionId;
use crate::material::{CommodityKey, FormDefinition, MaterialAssemblyProfile, MaterialDefinition};
use crate::production::ProcessId;

use super::Registries;

/// One authored way a commodity can enter inventory custody.
///
/// These are definition-level relationships, not claims that the source is currently reachable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommoditySource {
    ManualCraft {
        process: ProcessId,
        output_mass: Mass,
    },
    EquipmentDisassembly {
        equipment: EquipmentDefinitionId,
        recovered_mass: Mass,
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
        output_mass: Mass,
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

fn profile_mass(profile: &MaterialAssemblyProfile, commodity: CommodityKey) -> Option<Mass> {
    profile
        .inputs()
        .iter()
        .find(|input| input.commodity() == commodity)
        .map(|input| input.mass())
}

fn collect_crafting_relationships(
    registries: &Registries,
    commodity: CommodityKey,
    sources: &mut Vec<CommoditySource>,
    uses: &mut Vec<CommodityUse>,
) {
    for definition in registries.crafting().definitions() {
        if definition.input() == commodity {
            uses.push(CommodityUse::ManualCraft {
                process: definition.process(),
                required_mass: definition.input_mass(),
            });
        }
        for output in definition.outputs() {
            if output.commodity() == commodity {
                sources.push(CommoditySource::ManualCraft {
                    process: definition.process(),
                    output_mass: output.mass(),
                });
            }
        }
    }
}

fn collect_ore_processing_relationships(
    registries: &Registries,
    commodity: CommodityKey,
    sources: &mut Vec<CommoditySource>,
    uses: &mut Vec<CommodityUse>,
) {
    for definition in registries.ore_processing().manual_comminution_definitions() {
        if commodity.form() == definition.input_form() {
            uses.push(CommodityUse::OreProcessing {
                process: definition.process(),
            });
        }
        if commodity.form() == definition.output_form() {
            sources.push(CommoditySource::OreProcessing {
                process: definition.process(),
            });
        }
    }
    for definition in registries.ore_processing().comminution_definitions() {
        if commodity.form() == definition.input_form() {
            uses.push(CommodityUse::OreProcessing {
                process: definition.process(),
            });
        }
        if commodity.form() == definition.output_form() {
            sources.push(CommoditySource::OreProcessing {
                process: definition.process(),
            });
        }
    }
    for definition in registries.ore_processing().screening_definitions() {
        if commodity.form() == definition.input_form() {
            uses.push(CommodityUse::OreProcessing {
                process: definition.process(),
            });
        }
        if commodity.form() == definition.output_form() {
            sources.push(CommoditySource::OreProcessing {
                process: definition.process(),
            });
        }
    }
}

fn collect_separation_relationships(
    registries: &Registries,
    commodity: CommodityKey,
    sources: &mut Vec<CommoditySource>,
    uses: &mut Vec<CommodityUse>,
) {
    for definition in registries
        .ore_processing()
        .manual_constituent_separation_definitions()
    {
        if commodity.material() == definition.target_material()
            && commodity.form() == definition.input_form()
        {
            uses.push(CommodityUse::OreProcessing {
                process: definition.process(),
            });
        }
        if commodity.material() == definition.target_material()
            && commodity.form() == definition.target_output_form()
        {
            sources.push(CommoditySource::OreProcessing {
                process: definition.process(),
            });
        }
    }
    for definition in registries
        .ore_processing()
        .constituent_separation_definitions()
    {
        if commodity.form() == definition.input_form()
            && (!definition.requires_target_host()
                || commodity.material() == definition.target_material())
        {
            uses.push(CommodityUse::OreProcessing {
                process: definition.process(),
            });
        }
        if commodity.material() == definition.target_material()
            && commodity.form() == definition.target_output_form()
        {
            sources.push(CommoditySource::OreProcessing {
                process: definition.process(),
            });
        }
    }
}

fn collect_thermal_relationships(
    registries: &Registries,
    commodity: CommodityKey,
    sources: &mut Vec<CommoditySource>,
    uses: &mut Vec<CommodityUse>,
) {
    for definition in registries.thermal().melting_definitions() {
        if commodity.material() == definition.material() {
            if definition.solid_forms().contains(&commodity.form()) {
                uses.push(CommodityUse::ThermalPhaseChange {
                    process: definition.process(),
                });
            }
            if commodity.form() == definition.liquid_form() {
                sources.push(CommoditySource::ThermalPhaseChange {
                    process: definition.process(),
                });
            }
        }
    }
    for definition in registries.thermal().casting_definitions() {
        if commodity.material() == definition.material() {
            if commodity.form() == definition.liquid_form() {
                uses.push(CommodityUse::ThermalPhaseChange {
                    process: definition.process(),
                });
            }
            if commodity.form() == definition.solid_form() {
                sources.push(CommoditySource::ThermalPhaseChange {
                    process: definition.process(),
                });
            }
        }
    }
}

fn collect_equipment_relationships(
    registries: &Registries,
    commodity: CommodityKey,
    sources: &mut Vec<CommoditySource>,
    uses: &mut Vec<CommodityUse>,
) {
    for definition in registries.equipment().definitions() {
        if let Some(profile) = definition.assembly_profile()
            && let Some(required_mass) = profile_mass(profile, commodity)
        {
            uses.push(CommodityUse::EquipmentAssembly {
                equipment: definition.id(),
                required_mass,
            });
            sources.push(CommoditySource::EquipmentDisassembly {
                equipment: definition.id(),
                recovered_mass: required_mass,
            });
        }
        if let Some(upgrade) = definition.upgrade_profile()
            && let Some(required_mass) = profile_mass(upgrade.additions(), commodity)
        {
            uses.push(CommodityUse::EquipmentUpgrade {
                from: upgrade.from(),
                to: definition.id(),
                required_mass,
            });
        }
        if let Some(maintenance) = definition.maintenance_profile() {
            if maintenance.replacement() == commodity {
                uses.push(CommodityUse::EquipmentMaintenance {
                    equipment: definition.id(),
                    full_service_mass: maintenance.full_service_replacement_mass(),
                });
            }
            if maintenance.spent() == commodity {
                sources.push(CommoditySource::EquipmentMaintenanceSpent {
                    equipment: definition.id(),
                    output_mass: maintenance.full_service_replacement_mass(),
                });
            }
        }
    }
}

fn collect_storage_relationships(
    registries: &Registries,
    commodity: CommodityKey,
    sources: &mut Vec<CommoditySource>,
    uses: &mut Vec<CommodityUse>,
) {
    for definition in registries.storage().definitions() {
        if let Some(required_mass) = profile_mass(definition.assembly_profile(), commodity) {
            uses.push(CommodityUse::StorageConstruction {
                storage: definition.id(),
                required_mass,
            });
            sources.push(CommoditySource::StorageDismantling {
                storage: definition.id(),
                recovered_mass: required_mass,
            });
        }
    }
}

fn collect_energy_relationships(
    registries: &Registries,
    commodity: CommodityKey,
    sources: &mut Vec<CommoditySource>,
    uses: &mut Vec<CommodityUse>,
) {
    for definition in registries.energy().definitions() {
        if let Some(profile) = definition.assembly_profile()
            && let Some(required_mass) = profile_mass(profile, commodity)
        {
            uses.push(CommodityUse::EnergyStoreAssembly {
                store: definition.id(),
                required_mass,
            });
            sources.push(CommoditySource::EnergyStoreDisassembly {
                store: definition.id(),
                recovered_mass: required_mass,
            });
        }
        if let Some(upgrade) = definition.upgrade_profile()
            && let Some(required_mass) = profile_mass(upgrade.additions(), commodity)
        {
            uses.push(CommodityUse::EnergyStoreUpgrade {
                from: upgrade.from(),
                to: definition.id(),
                required_mass,
            });
        }
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
