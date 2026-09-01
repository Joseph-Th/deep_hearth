//! Registry-derived authored process relationships shared by discovery and planning callers.

use std::collections::BTreeMap;

use crate::capability::evaluate_capabilities;
use crate::energy::{EnergyCarrier, EnergyStoreDefinitionId};
use crate::equipment::EquipmentDefinitionId;
use crate::production::ProcessId;

use super::RegistryDomains;

/// Unique authored execution family that supplies physical resolution semantics for a process.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProcessExecutionFamily {
    ManualCraft,
    ManualComminution,
    ManualSeparation,
    Comminution,
    Screening,
    ConstituentSeparation,
    SensibleHeating,
    Melting,
    Casting,
}

impl ProcessExecutionFamily {
    #[must_use]
    pub const fn is_manual(self) -> bool {
        matches!(
            self,
            Self::ManualCraft | Self::ManualComminution | Self::ManualSeparation
        )
    }
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
    pub const fn energy_role(&self) -> ProcessEnergyRole {
        self.energy_role
    }

    /// Equipment definitions whose nominal capability profile satisfies the process definition.
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

#[cfg(test)]
pub(super) fn build_partial_process_topology_for_owner_tests(
    domains: &RegistryDomains,
) -> BTreeMap<ProcessId, ProcessTopology> {
    domains
        .production
        .definitions()
        .filter_map(|process| derive_process_topology(domains, process))
        .collect()
}

fn derive_process_topology(
    domains: &RegistryDomains,
    process: &crate::production::ProcessDefinition,
) -> Option<(ProcessId, ProcessTopology)> {
    let (execution_family, energy_role) = process_execution_semantics(domains, process.id())?;
    let nominal_providers = if execution_family.is_manual() {
        Vec::new()
    } else {
        domains
            .equipment
            .definitions()
            .filter(|equipment| {
                evaluate_capabilities(
                    &domains.capabilities,
                    equipment.capabilities(),
                    process.capability_requirements(),
                )
                .is_ok()
            })
            .map(|equipment| equipment.id())
            .collect()
    };
    let compatible_energy_stores = domains
        .energy
        .definitions()
        .filter(|store| energy_store_matches_role(store, energy_role))
        .map(|store| store.id())
        .collect();
    Some((
        process.id(),
        ProcessTopology {
            execution_family,
            energy_role,
            nominal_providers,
            compatible_energy_stores,
        },
    ))
}

fn claim_execution_semantics(
    claimed: &mut Option<(ProcessExecutionFamily, ProcessEnergyRole)>,
    process: ProcessId,
    candidate: (ProcessExecutionFamily, ProcessEnergyRole),
) {
    assert!(
        claimed.replace(candidate).is_none(),
        "process {} cannot own multiple physical resolver semantics",
        process.value()
    );
}

fn process_execution_semantics(
    domains: &RegistryDomains,
    process: ProcessId,
) -> Option<(ProcessExecutionFamily, ProcessEnergyRole)> {
    let candidates = [
        domains
            .crafting
            .get_manual(process)
            .map(|_| (ProcessExecutionFamily::ManualCraft, ProcessEnergyRole::None)),
        domains
            .ore_processing
            .get_manual_comminution(process)
            .map(|_| {
                (
                    ProcessExecutionFamily::ManualComminution,
                    ProcessEnergyRole::None,
                )
            }),
        domains
            .ore_processing
            .get_manual_constituent_separation(process)
            .map(|_| {
                (
                    ProcessExecutionFamily::ManualSeparation,
                    ProcessEnergyRole::None,
                )
            }),
        domains
            .ore_processing
            .get_comminution(process)
            .map(|definition| {
                (
                    ProcessExecutionFamily::Comminution,
                    ProcessEnergyRole::Supply(definition.energy_carrier()),
                )
            }),
        domains
            .ore_processing
            .get_screening(process)
            .map(|definition| {
                (
                    ProcessExecutionFamily::Screening,
                    ProcessEnergyRole::Supply(definition.energy_carrier()),
                )
            }),
        domains
            .ore_processing
            .get_constituent_separation(process)
            .map(|definition| {
                (
                    ProcessExecutionFamily::ConstituentSeparation,
                    ProcessEnergyRole::Supply(definition.energy_carrier()),
                )
            }),
        domains
            .thermal
            .get_sensible_heating(process)
            .map(|definition| {
                (
                    ProcessExecutionFamily::SensibleHeating,
                    ProcessEnergyRole::Supply(definition.energy_carrier()),
                )
            }),
        domains.thermal.get_melting(process).map(|definition| {
            (
                ProcessExecutionFamily::Melting,
                ProcessEnergyRole::Supply(definition.energy_carrier()),
            )
        }),
        domains.thermal.get_casting(process).map(|definition| {
            (
                ProcessExecutionFamily::Casting,
                ProcessEnergyRole::Sink(definition.energy_carrier()),
            )
        }),
    ];
    let mut claimed = None;
    for candidate in candidates.into_iter().flatten() {
        claim_execution_semantics(&mut claimed, process, candidate);
    }
    claimed
}

fn energy_store_matches_role(
    definition: &crate::energy::EnergyStoreDefinition,
    role: ProcessEnergyRole,
) -> bool {
    match role {
        ProcessEnergyRole::None => false,
        ProcessEnergyRole::Supply(carrier) => {
            definition.carrier() == carrier && !definition.max_output_power().is_zero()
        }
        ProcessEnergyRole::Sink(carrier) => {
            definition.carrier() == carrier && !definition.max_input_power().is_zero()
        }
    }
}
