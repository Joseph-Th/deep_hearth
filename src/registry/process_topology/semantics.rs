//! Resolves each authored process to its one physical execution family and energy relationship.

use crate::production::ProcessId;

use super::{ProcessEnergyRole, ProcessExecutionFamily};
use crate::registry::RegistryDomains;

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

pub(super) fn process_execution_semantics(
    domains: &RegistryDomains,
    process: ProcessId,
) -> Option<(ProcessExecutionFamily, ProcessEnergyRole)> {
    let candidates = [
        domains
            .crafting
            .get_manual(process)
            .map(|_| (ProcessExecutionFamily::ManualCraft, ProcessEnergyRole::None)),
        domains.crafting.get_powered(process).map(|definition| {
            (
                ProcessExecutionFamily::PoweredCraft,
                ProcessEnergyRole::Supply(definition.energy_carrier()),
            )
        }),
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
