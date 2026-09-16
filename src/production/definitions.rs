//! Immutable process definitions and deterministic lookup registry for the production subsystem.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::capability::{CapabilityId, CapabilityRegistry, CapabilityRequirement};
use crate::core::quantity::Mass;
use crate::material::MaterialLotSpec;

/// Stable authored identifier for one physical production process definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ProcessId(u32);

impl ProcessId {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        assert!(value != 0, "process id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Immutable authored requirements for one class of physical production operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessDefinition {
    id: ProcessId,
    name: String,
    capability_requirements: Vec<CapabilityRequirement>,
}

impl ProcessDefinition {
    /// Builds normalized process identity and provider-discovery requirements.
    ///
    /// Exact matter eligibility and quantity belong to the owning physical resolver together with
    /// duration, recovery, output composition, output temperature, and other operation-specific
    /// outcomes. Machine-provider requirements remain typed capabilities rather than generic tiers.
    #[must_use]
    pub fn new(
        id: ProcessId,
        name: impl Into<String>,
        mut capability_requirements: Vec<CapabilityRequirement>,
    ) -> Self {
        assert!(id.value() != 0, "process id must be nonzero");
        let name = name.into();
        assert!(!name.trim().is_empty(), "process name must not be empty");
        capability_requirements.sort();
        validate_capability_requirements(id, &capability_requirements);
        Self {
            id,
            name,
            capability_requirements,
        }
    }

    #[must_use]
    pub const fn id(&self) -> ProcessId {
        self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn capability_requirements(&self) -> &[CapabilityRequirement] {
        &self.capability_requirements
    }

    /// Returns the unique authored requirement for one capability dimension.
    #[must_use]
    pub(crate) fn get_capability_requirement(
        &self,
        capability: CapabilityId,
    ) -> Option<CapabilityRequirement> {
        self.capability_requirements
            .iter()
            .copied()
            .find(|requirement| requirement.capability() == capability)
    }
}

fn validate_capability_requirements(id: ProcessId, requirements: &[CapabilityRequirement]) {
    for pair in requirements.windows(2) {
        assert!(
            pair[0].capability() != pair[1].capability(),
            "process {} contains more than one requirement for capability {}",
            id.value(),
            pair[0].capability().value()
        );
    }
}

pub(crate) fn sum_lot_spec_mass(entries: &[MaterialLotSpec]) -> Option<Mass> {
    let mut total = Mass::ZERO;
    for entry in entries {
        total = total.checked_add(entry.mass())?;
    }
    Some(total)
}

fn validate_process_capability_references(
    definition: &ProcessDefinition,
    capabilities: &CapabilityRegistry,
) {
    for requirement in definition.capability_requirements() {
        let capability = requirement.capability();
        let Some(capability_definition) = capabilities.get_capability(capability) else {
            panic!(
                "process {} references missing capability {}",
                definition.id().value(),
                capability.value()
            );
        };
        assert_eq!(
            requirement.threshold().kind(),
            capability_definition.kind(),
            "process {} capability {} requirement has wrong physical value kind",
            definition.id().value(),
            capability.value()
        );
    }
}

/// Immutable deterministic process lookup table assembled from Rust content builders.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProductionRegistry {
    definitions: BTreeMap<ProcessId, ProcessDefinition>,
}

impl ProductionRegistry {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            definitions: BTreeMap::new(),
        }
    }

    pub(crate) fn register_process(&mut self, definition: ProcessDefinition) {
        let id = definition.id();
        assert!(
            self.definitions.insert(id, definition).is_none(),
            "duplicate process id {}",
            id.value()
        );
    }

    /// Returns one process definition by stable authored ID.
    #[must_use]
    pub fn get_process(&self, id: ProcessId) -> Option<&ProcessDefinition> {
        self.definitions.get(&id)
    }

    /// Iterates authored process definitions in stable process-ID order.
    pub fn definitions(&self) -> impl Iterator<Item = &ProcessDefinition> {
        self.definitions.values()
    }

    pub(crate) fn validate_references(&self, capabilities: &CapabilityRegistry) {
        for definition in self.definitions.values() {
            validate_process_capability_references(definition, capabilities);
        }
    }
}
