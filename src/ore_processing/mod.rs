//! Owns ore/material preparation definitions, shared physics, and execution APIs.

mod comminution_execution;
mod definitions;
mod powered_physics;
mod screening_execution;
mod separation_execution;
mod throughput;
mod validation;

use crate::production::ProcessId;
pub use powered_physics::{PoweredOreBottleneck, PoweredOreJobValidationError};
use std::collections::{BTreeMap, BTreeSet};

pub use comminution_execution::{
    ComminutionBatchError, ComminutionJobValidationError, ComminutionRequest,
    ComminutionResolutionError, ManualComminutionCommitError, ManualComminutionRequest,
    ManualComminutionResolutionError, ResolvedComminution, ResolvedManualComminution,
    StartManualComminutionError, ValidatedManualComminutionStart, resolve_comminution_process,
    resolve_manual_comminution_process, validate_start_manual_comminution,
};

pub(crate) use comminution_execution::validate_loaded_comminution_job;

pub use definitions::{
    ComminutionProcessDefinition, ConstituentRecoveryProfile,
    ConstituentSeparationProcessDefinition, ManualComminutionProcessDefinition,
    ManualConstituentSeparationProcessDefinition, ManualOreProcessProfile,
    PoweredOreProcessProfile, ScreeningProcessDefinition,
};

pub use separation_execution::{
    ConstituentSeparationBatchError, ConstituentSeparationJobValidationError,
    ConstituentSeparationRequest, ConstituentSeparationResolutionError,
    ManualConstituentSeparationCommitError, ManualConstituentSeparationRequest,
    ManualConstituentSeparationResolutionError, ResolvedConstituentSeparation,
    ResolvedManualConstituentSeparation, StartManualConstituentSeparationError,
    ValidatedManualConstituentSeparationStart, resolve_constituent_separation_process,
    resolve_manual_constituent_separation_process, validate_start_manual_constituent_separation,
};

pub(crate) use separation_execution::validate_loaded_constituent_separation_job;

pub use screening_execution::{
    ResolvedScreening, ScreeningBatchError, ScreeningJobValidationError, ScreeningRequest,
    ScreeningResolutionError, resolve_screening_process,
};

pub(crate) use screening_execution::validate_loaded_screening_job;

pub use throughput::{MassFlowDurationError, calculate_mass_flow_duration_ceiling};

/// Immutable lookup table for ore/material-preparation process semantics.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OreProcessingRegistry {
    comminution: BTreeMap<ProcessId, ComminutionProcessDefinition>,
    manual_comminution: BTreeMap<ProcessId, ManualComminutionProcessDefinition>,
    screening: BTreeMap<ProcessId, ScreeningProcessDefinition>,
    separation: BTreeMap<ProcessId, ConstituentSeparationProcessDefinition>,
    manual_separation: BTreeMap<ProcessId, ManualConstituentSeparationProcessDefinition>,
}

impl OreProcessingRegistry {
    #[cfg(test)]
    pub(crate) fn new(definitions: impl IntoIterator<Item = ComminutionProcessDefinition>) -> Self {
        Self::new_with_processes(definitions, std::iter::empty(), std::iter::empty())
    }

    #[cfg(test)]
    pub(crate) fn new_with_processes(
        comminution_definitions: impl IntoIterator<Item = ComminutionProcessDefinition>,
        screening_definitions: impl IntoIterator<Item = ScreeningProcessDefinition>,
        separation_definitions: impl IntoIterator<Item = ConstituentSeparationProcessDefinition>,
    ) -> Self {
        Self::new_with_manual_processes(
            comminution_definitions,
            screening_definitions,
            separation_definitions,
            std::iter::empty(),
            std::iter::empty(),
        )
    }

    pub(crate) fn new_with_manual_processes(
        comminution_definitions: impl IntoIterator<Item = ComminutionProcessDefinition>,
        screening_definitions: impl IntoIterator<Item = ScreeningProcessDefinition>,
        separation_definitions: impl IntoIterator<Item = ConstituentSeparationProcessDefinition>,
        manual_comminution_definitions: impl IntoIterator<Item = ManualComminutionProcessDefinition>,
        manual_separation_definitions: impl IntoIterator<
            Item = ManualConstituentSeparationProcessDefinition,
        >,
    ) -> Self {
        let mut comminution = BTreeMap::new();
        for definition in comminution_definitions {
            let process = definition.process();
            assert!(
                comminution.insert(process, definition).is_none(),
                "duplicate comminution definition for process {}",
                process.value()
            );
        }
        let mut manual_comminution = BTreeMap::new();
        for definition in manual_comminution_definitions {
            let process = definition.process();
            assert!(
                manual_comminution.insert(process, definition).is_none(),
                "duplicate manual comminution definition for process {}",
                process.value()
            );
        }
        let mut screening = BTreeMap::new();
        for definition in screening_definitions {
            let process = definition.process();
            assert!(
                screening.insert(process, definition).is_none(),
                "duplicate screening definition for process {}",
                process.value()
            );
        }
        let mut separation = BTreeMap::new();
        for definition in separation_definitions {
            let process = definition.process();
            assert!(
                separation.insert(process, definition).is_none(),
                "duplicate constituent-separation definition for process {}",
                process.value()
            );
        }
        let mut manual_separation = BTreeMap::new();
        for definition in manual_separation_definitions {
            let process = definition.process();
            assert!(
                manual_separation.insert(process, definition).is_none(),
                "duplicate manual constituent-separation definition for process {}",
                process.value()
            );
        }
        let mut claimed_processes = BTreeSet::new();
        for process in comminution
            .keys()
            .chain(manual_comminution.keys())
            .chain(screening.keys())
            .chain(separation.keys())
            .chain(manual_separation.keys())
        {
            assert!(
                claimed_processes.insert(*process),
                "process {} cannot own multiple ore-processing resolver semantics",
                process.value()
            );
        }
        Self {
            comminution,
            manual_comminution,
            screening,
            separation,
            manual_separation,
        }
    }

    #[must_use]
    pub fn get_comminution(&self, process: ProcessId) -> Option<&ComminutionProcessDefinition> {
        self.comminution.get(&process)
    }

    #[must_use]
    pub fn get_manual_comminution(
        &self,
        process: ProcessId,
    ) -> Option<&ManualComminutionProcessDefinition> {
        self.manual_comminution.get(&process)
    }

    #[must_use]
    pub fn get_screening(&self, process: ProcessId) -> Option<ScreeningProcessDefinition> {
        self.screening.get(&process).copied()
    }

    #[must_use]
    pub fn get_constituent_separation(
        &self,
        process: ProcessId,
    ) -> Option<ConstituentSeparationProcessDefinition> {
        self.separation.get(&process).copied()
    }

    #[must_use]
    pub fn get_manual_constituent_separation(
        &self,
        process: ProcessId,
    ) -> Option<ManualConstituentSeparationProcessDefinition> {
        self.manual_separation.get(&process).copied()
    }

    pub(crate) fn process_ids(&self) -> impl Iterator<Item = ProcessId> + '_ {
        self.comminution
            .keys()
            .chain(self.manual_comminution.keys())
            .chain(self.screening.keys())
            .chain(self.separation.keys())
            .chain(self.manual_separation.keys())
            .copied()
    }

    pub(crate) fn has_process(&self, process: ProcessId) -> bool {
        self.comminution.contains_key(&process)
            || self.manual_comminution.contains_key(&process)
            || self.screening.contains_key(&process)
            || self.separation.contains_key(&process)
            || self.manual_separation.contains_key(&process)
    }
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
