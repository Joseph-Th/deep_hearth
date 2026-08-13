//! Immutable process definitions and deterministic lookup registry for the production subsystem.

use std::collections::BTreeMap;
#[cfg(test)]
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::capability::{CapabilityRegistry, CapabilityRequirement};
use crate::core::quantity::Mass;
use crate::material::{MaterialInputSpec, MaterialLotSpec, MaterialRegistry};

/// Stable authored identifier for one physical production process definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ProcessId(u32);

impl ProcessId {
    #[must_use]
    pub const fn new(value: u32) -> Self {
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
    inputs: Vec<MaterialInputSpec>,
    capability_requirements: Vec<CapabilityRequirement>,
    input_mass: Mass,
}

impl ProcessDefinition {
    /// Builds normalized static material and capability requirements.
    ///
    /// Duration, recovery, output composition, output temperature, and other operation-specific
    /// physical outcomes belong in a resolved process plan. Equipment/tool/worker requirements are
    /// authored here as typed capabilities rather than generic technology tiers.
    #[must_use]
    pub fn new(
        id: ProcessId,
        name: impl Into<String>,
        mut inputs: Vec<MaterialInputSpec>,
        mut capability_requirements: Vec<CapabilityRequirement>,
    ) -> Self {
        assert!(id.value() != 0, "process id must be nonzero");
        let name = name.into();
        assert!(!name.trim().is_empty(), "process name must not be empty");
        assert!(
            !inputs.is_empty(),
            "material process {} has no input requirements",
            id.value()
        );

        inputs.sort();
        validate_inputs(id, &inputs);
        capability_requirements.sort();
        validate_capability_requirements(id, &capability_requirements);
        let input_mass = match sum_input_spec_mass(&inputs) {
            Some(mass) => mass,
            None => panic!("process {} input mass overflows", id.value()),
        };

        Self {
            id,
            name,
            inputs,
            capability_requirements,
            input_mass,
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
    pub fn inputs(&self) -> &[MaterialInputSpec] {
        &self.inputs
    }

    #[must_use]
    pub fn capability_requirements(&self) -> &[CapabilityRequirement] {
        &self.capability_requirements
    }

    #[must_use]
    pub const fn input_mass(&self) -> Mass {
        self.input_mass
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

fn validate_inputs(id: ProcessId, inputs: &[MaterialInputSpec]) {
    for input in inputs {
        assert!(
            !input.mass().is_zero(),
            "process {} contains zero-mass input",
            id.value()
        );
    }
    for pair in inputs.windows(2) {
        assert!(
            pair[0] != pair[1],
            "process {} contains duplicate input specification",
            id.value()
        );
    }
}

fn sum_input_spec_mass(entries: &[MaterialInputSpec]) -> Option<Mass> {
    let mut total = Mass::ZERO;
    for entry in entries {
        total = total.checked_add(entry.mass())?;
    }
    Some(total)
}

#[cfg(test)]
pub(super) fn validate_resolved_outputs(id: ProcessId, outputs: &[MaterialLotSpec]) {
    let mut seen = BTreeSet::new();
    for output in outputs {
        assert!(
            !output.mass().is_zero(),
            "process {} contains zero-mass output",
            id.value()
        );
        if let Err(error) = output.composition().validate() {
            panic!(
                "process {} contains invalid output composition: {error}",
                id.value()
            );
        }
        assert!(
            output
                .composition()
                .parts_per_million(output.commodity().material())
                > 0,
            "process {} output composition omits host material {}",
            id.value(),
            output.commodity().material().value()
        );
        assert!(
            seen.insert(output.clone()),
            "process {} contains duplicate resolved output lot specification",
            id.value()
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

    #[cfg(test)]
    pub(crate) fn register_process_for_test(&mut self, definition: ProcessDefinition) {
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

    pub(crate) fn validate_references(
        &self,
        materials: &MaterialRegistry,
        capabilities: &CapabilityRegistry,
    ) {
        for definition in self.definitions.values() {
            for input in definition.inputs() {
                assert!(
                    materials.has_commodity(input.commodity()),
                    "process {} references missing input material {} or form {}",
                    definition.id().value(),
                    input.commodity().material().value(),
                    input.commodity().form().value()
                );
                for constraint in input.constraints() {
                    assert!(
                        materials.get_material(constraint.material()).is_some(),
                        "process {} input constraint references missing material {}",
                        definition.id().value(),
                        constraint.material().value()
                    );
                }
            }
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
    }
}
