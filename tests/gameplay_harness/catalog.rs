//! Read-only authored process topology used by gameplay diagnostics and catalog contracts.

use deep_hearth::production::ProcessId;
pub(super) use deep_hearth::registry::ProcessExecutionFamily as ProcessResolverKind;
use deep_hearth::registry::{ProcessEnergyRole, Registries};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ProcessCatalogEntry {
    pub(super) process: ProcessId,
    pub(super) name: String,
    pub(super) resolver: ProcessResolverKind,
    pub(super) nominal_provider_count: usize,
    pub(super) authored_acquisition_provider_count: usize,
    pub(super) compatible_energy_store_count: usize,
    pub(super) authored_assembly_energy_store_count: usize,
    pub(super) energy_role: ProcessEnergyRole,
}

pub(super) fn process_catalog_entries(registries: &Registries) -> Vec<ProcessCatalogEntry> {
    registries
        .production()
        .definitions()
        .map(|process| {
            let topology = registries
                .process_topology(process.id())
                .unwrap_or_else(|| {
                    panic!(
                        "authored process {} has no physical execution family",
                        process.id().value()
                    )
                });
            let nominal_provider_count = topology.nominal_providers().len();
            let authored_acquisition_provider_count = topology
                .nominal_providers()
                .iter()
                .filter(|provider| {
                    registries
                        .equipment()
                        .get_equipment(**provider)
                        .is_some_and(|definition| definition.has_authored_acquisition_edge())
                })
                .count();
            let compatible_energy_store_count = topology.compatible_energy_stores().len();
            let authored_assembly_energy_store_count = topology
                .compatible_energy_stores()
                .iter()
                .filter(|store| {
                    registries
                        .energy()
                        .get_store(**store)
                        .is_some_and(|definition| definition.has_authored_assembly_edge())
                })
                .count();
            ProcessCatalogEntry {
                process: process.id(),
                name: process.name().to_owned(),
                resolver: topology.execution_family(),
                nominal_provider_count,
                authored_acquisition_provider_count,
                compatible_energy_store_count,
                authored_assembly_energy_store_count,
                energy_role: topology.energy_role(),
            }
        })
        .collect()
}
