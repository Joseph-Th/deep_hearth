//! Immutable ore/material-preparation resolver registry and definition lookup.

use std::collections::{BTreeMap, BTreeSet};

use crate::production::ProcessId;

use super::{
    ComminutionProcessDefinition, ConstituentSeparationProcessDefinition,
    ManualComminutionProcessDefinition, ManualConstituentSeparationProcessDefinition,
    ScreeningProcessDefinition,
};

/// Immutable lookup table for ore/material-preparation process semantics.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OreProcessingRegistry {
    pub(super) comminution: BTreeMap<ProcessId, ComminutionProcessDefinition>,
    pub(super) manual_comminution: BTreeMap<ProcessId, ManualComminutionProcessDefinition>,
    pub(super) screening: BTreeMap<ProcessId, ScreeningProcessDefinition>,
    pub(super) separation: BTreeMap<ProcessId, ConstituentSeparationProcessDefinition>,
    pub(super) manual_separation: BTreeMap<ProcessId, ManualConstituentSeparationProcessDefinition>,
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

    /// Iterates powered comminution definitions in stable process-ID order.
    pub fn comminution_definitions(&self) -> impl Iterator<Item = &ComminutionProcessDefinition> {
        self.comminution.values()
    }

    /// Iterates direct-labor comminution definitions in stable process-ID order.
    pub fn manual_comminution_definitions(
        &self,
    ) -> impl Iterator<Item = &ManualComminutionProcessDefinition> {
        self.manual_comminution.values()
    }

    /// Iterates powered screening definitions in stable process-ID order.
    pub fn screening_definitions(&self) -> impl Iterator<Item = ScreeningProcessDefinition> + '_ {
        self.screening.values().copied()
    }

    /// Iterates powered constituent-separation definitions in stable process-ID order.
    pub fn constituent_separation_definitions(
        &self,
    ) -> impl Iterator<Item = ConstituentSeparationProcessDefinition> + '_ {
        self.separation.values().copied()
    }

    /// Iterates direct-labor constituent-separation definitions in stable process-ID order.
    pub fn manual_constituent_separation_definitions(
        &self,
    ) -> impl Iterator<Item = ManualConstituentSeparationProcessDefinition> + '_ {
        self.manual_separation.values().copied()
    }
}
