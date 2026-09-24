//! Derives immutable equipment-provider and energy-store edges for authored process topology.

use crate::capability::{CapabilityValue, evaluate_capabilities};
use crate::energy::EnergyStoreDefinitionId;
use crate::equipment::EquipmentDefinitionId;
use crate::production::{ProcessDefinition, ProcessId};
use crate::registry::RegistryDomains;

use super::{ProcessEnergyRole, ProcessEquipmentRole, ProcessExecutionFamily};

pub(super) fn process_equipment_role(
    domains: &RegistryDomains,
    process: ProcessId,
    execution_family: ProcessExecutionFamily,
) -> ProcessEquipmentRole {
    match execution_family {
        ProcessExecutionFamily::ManualCraft => domains
            .crafting
            .get_manual(process)
            .unwrap_or_else(|| unreachable!("manual-craft topology has a crafting definition"))
            .equipment_profile()
            .map_or(ProcessEquipmentRole::None, |profile| {
                if profile.requires_equipment() {
                    ProcessEquipmentRole::Required
                } else {
                    ProcessEquipmentRole::Optional
                }
            }),
        ProcessExecutionFamily::ManualComminution => domains
            .ore_processing
            .get_manual_comminution(process)
            .unwrap_or_else(|| unreachable!("manual-comminution topology has a definition"))
            .operating_profile()
            .equipment_profile()
            .map_or(ProcessEquipmentRole::None, |_| {
                ProcessEquipmentRole::Optional
            }),
        ProcessExecutionFamily::ManualSeparation => domains
            .ore_processing
            .get_manual_constituent_separation(process)
            .unwrap_or_else(|| unreachable!("manual-separation topology has a definition"))
            .operating_profile()
            .equipment_profile()
            .map_or(ProcessEquipmentRole::None, |_| {
                ProcessEquipmentRole::Optional
            }),
        ProcessExecutionFamily::PoweredCraft
        | ProcessExecutionFamily::Comminution
        | ProcessExecutionFamily::Screening
        | ProcessExecutionFamily::ConstituentSeparation
        | ProcessExecutionFamily::SensibleHeating
        | ProcessExecutionFamily::Melting
        | ProcessExecutionFamily::Casting => ProcessEquipmentRole::Required,
    }
}

pub(super) fn nominal_providers(
    domains: &RegistryDomains,
    process: &ProcessDefinition,
    execution_family: ProcessExecutionFamily,
) -> Vec<EquipmentDefinitionId> {
    match execution_family {
        ProcessExecutionFamily::ManualCraft => {
            nominal_manual_craft_providers(domains, process.id())
        }
        ProcessExecutionFamily::ManualComminution | ProcessExecutionFamily::ManualSeparation => {
            nominal_manual_ore_providers(domains, process.id(), execution_family)
        }
        ProcessExecutionFamily::PoweredCraft
        | ProcessExecutionFamily::Comminution
        | ProcessExecutionFamily::Screening
        | ProcessExecutionFamily::ConstituentSeparation
        | ProcessExecutionFamily::SensibleHeating
        | ProcessExecutionFamily::Melting
        | ProcessExecutionFamily::Casting => nominal_machine_providers(domains, process),
    }
}

fn nominal_manual_craft_providers(
    domains: &RegistryDomains,
    process: ProcessId,
) -> Vec<EquipmentDefinitionId> {
    let Some(profile) = domains
        .crafting
        .get_manual(process)
        .and_then(|definition| definition.equipment_profile())
    else {
        return Vec::new();
    };
    let capability = profile.mass_flow_capability();
    domains
        .equipment
        .definitions()
        .filter(|equipment| {
            matches!(
                equipment.capabilities().get_capability(capability),
                Some(CapabilityValue::MassFlow(rate)) if !rate.is_zero()
            )
        })
        .map(|equipment| equipment.id())
        .collect()
}

fn nominal_manual_ore_providers(
    domains: &RegistryDomains,
    process: ProcessId,
    execution_family: ProcessExecutionFamily,
) -> Vec<EquipmentDefinitionId> {
    let profile = match execution_family {
        ProcessExecutionFamily::ManualComminution => domains
            .ore_processing
            .get_manual_comminution(process)
            .map(|definition| definition.operating_profile()),
        ProcessExecutionFamily::ManualSeparation => domains
            .ore_processing
            .get_manual_constituent_separation(process)
            .map(|definition| definition.operating_profile()),
        ProcessExecutionFamily::ManualCraft
        | ProcessExecutionFamily::PoweredCraft
        | ProcessExecutionFamily::Comminution
        | ProcessExecutionFamily::Screening
        | ProcessExecutionFamily::ConstituentSeparation
        | ProcessExecutionFamily::SensibleHeating
        | ProcessExecutionFamily::Melting
        | ProcessExecutionFamily::Casting => None,
    };
    let Some(capability) = profile
        .and_then(|profile| profile.equipment_profile())
        .map(|equipment| equipment.mass_flow_capability())
    else {
        return Vec::new();
    };
    domains
        .equipment
        .definitions()
        .filter(|equipment| {
            matches!(
                equipment.capabilities().get_capability(capability),
                Some(CapabilityValue::MassFlow(rate)) if !rate.is_zero()
            )
        })
        .map(|equipment| equipment.id())
        .collect()
}

fn nominal_machine_providers(
    domains: &RegistryDomains,
    process: &ProcessDefinition,
) -> Vec<EquipmentDefinitionId> {
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
}

pub(super) fn compatible_energy_stores(
    domains: &RegistryDomains,
    role: ProcessEnergyRole,
) -> Vec<EnergyStoreDefinitionId> {
    domains
        .energy
        .definitions()
        .filter(|store| energy_store_matches_role(store, role))
        .map(|store| store.id())
        .collect()
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
