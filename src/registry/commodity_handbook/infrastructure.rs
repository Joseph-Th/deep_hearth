//! Infrastructure assembly, recovery, upgrade, and maintenance handbook relationships.

use crate::core::quantity::Mass;
use crate::material::{CommodityKey, MaterialAssemblyProfile};

use super::{CommoditySource, CommodityUse, EquipmentDisassemblyRecovery};
use crate::registry::Registries;

fn profile_mass(profile: &MaterialAssemblyProfile, commodity: CommodityKey) -> Option<Mass> {
    profile
        .inputs()
        .iter()
        .find(|input| input.commodity() == commodity)
        .map(|input| input.mass())
}

pub(super) fn collect_equipment_relationships(
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
                recovery: EquipmentDisassemblyRecovery::PristineExact,
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
                let full_service_mass = maintenance.full_service_replacement_mass();
                sources.push(CommoditySource::EquipmentMaintenanceSpent {
                    equipment: definition.id(),
                    full_service_mass,
                });
                if maintenance.is_component_replacement() {
                    sources.push(CommoditySource::EquipmentDisassembly {
                        equipment: definition.id(),
                        recovered_mass: full_service_mass,
                        recovery: EquipmentDisassemblyRecovery::WornComponentSpent,
                    });
                }
            }
        }
    }
}

pub(super) fn collect_storage_relationships(
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

pub(super) fn collect_energy_relationships(
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
