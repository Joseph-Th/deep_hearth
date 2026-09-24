//! Registry-derived authored process relationships shared by discovery and planning callers.

use std::collections::BTreeMap;

use crate::energy::{EnergyCarrier, EnergyStoreDefinitionId};
use crate::equipment::EquipmentDefinitionId;
use crate::production::ProcessId;

use super::RegistryDomains;

mod providers;
mod semantics;

/// Unique authored execution family that supplies physical resolution semantics for a process.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProcessExecutionFamily {
    ManualCraft,
    PoweredCraft,
    ManualComminution,
    ManualSeparation,
    Comminution,
    Screening,
    ConstituentSeparation,
    SensibleHeating,
    Melting,
    Casting,
}

/// Static equipment relationship required by one authored process execution family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessEquipmentRole {
    None,
    Optional,
    Required,
}

/// Static energy relationship required by one authored process execution family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessEnergyRole {
    None,
    Supply(EnergyCarrier),
    Sink(EnergyCarrier),
}

/// Immutable definition-level topology for one process.
///
/// Provider and store lists are authored possibilities only. They do not imply ordinary
/// reachability, current world availability, condition/support, or authorization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessTopology {
    execution_family: ProcessExecutionFamily,
    equipment_role: ProcessEquipmentRole,
    energy_role: ProcessEnergyRole,
    nominal_providers: Vec<EquipmentDefinitionId>,
    compatible_energy_stores: Vec<EnergyStoreDefinitionId>,
}

impl ProcessTopology {
    #[must_use]
    pub const fn execution_family(&self) -> ProcessExecutionFamily {
        self.execution_family
    }

    #[must_use]
    pub const fn equipment_role(&self) -> ProcessEquipmentRole {
        self.equipment_role
    }

    #[must_use]
    pub const fn energy_role(&self) -> ProcessEnergyRole {
        self.energy_role
    }

    /// Equipment definitions whose nominal capability profile can satisfy this authored
    /// execution family's equipment relationship.
    #[must_use]
    pub fn nominal_providers(&self) -> &[EquipmentDefinitionId] {
        &self.nominal_providers
    }

    /// Energy-store definitions with the required carrier and transfer direction.
    #[must_use]
    pub fn compatible_energy_stores(&self) -> &[EnergyStoreDefinitionId] {
        &self.compatible_energy_stores
    }
}

pub(super) fn build_process_topology(
    domains: &RegistryDomains,
) -> BTreeMap<ProcessId, ProcessTopology> {
    domains
        .production
        .definitions()
        .map(|process| {
            derive_process_topology(domains, process).unwrap_or_else(|| {
                panic!(
                    "process {} has no physical resolver semantics",
                    process.id().value()
                )
            })
        })
        .collect()
}

fn derive_process_topology(
    domains: &RegistryDomains,
    process: &crate::production::ProcessDefinition,
) -> Option<(ProcessId, ProcessTopology)> {
    let (execution_family, energy_role) =
        semantics::process_execution_semantics(domains, process.id())?;
    let equipment_role = providers::process_equipment_role(domains, process.id(), execution_family);
    let nominal_providers = providers::nominal_providers(domains, process, execution_family);
    let compatible_energy_stores = providers::compatible_energy_stores(domains, energy_role);
    let topology = ProcessTopology {
        execution_family,
        equipment_role,
        energy_role,
        nominal_providers,
        compatible_energy_stores,
    };
    assert_process_topology_has_required_edges(process.id(), &topology);
    Some((process.id(), topology))
}

fn assert_process_topology_has_required_edges(process: ProcessId, topology: &ProcessTopology) {
    match topology.equipment_role {
        ProcessEquipmentRole::None => assert!(
            topology.nominal_providers.is_empty(),
            "equipment-free process {} cannot expose equipment providers",
            process.value()
        ),
        ProcessEquipmentRole::Optional | ProcessEquipmentRole::Required => assert!(
            !topology.nominal_providers.is_empty(),
            "equipment-bearing process {} has no nominal equipment provider",
            process.value()
        ),
    }

    match topology.energy_role {
        ProcessEnergyRole::None => assert!(
            topology.compatible_energy_stores.is_empty(),
            "energy-free process {} cannot expose compatible energy stores",
            process.value()
        ),
        ProcessEnergyRole::Supply(_) | ProcessEnergyRole::Sink(_) => assert!(
            !topology.compatible_energy_stores.is_empty(),
            "energy-bearing process {} has no compatible energy store",
            process.value()
        ),
    }
}
