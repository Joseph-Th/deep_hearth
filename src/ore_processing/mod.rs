//! Owns ore/material preparation definitions, shared physics, and execution APIs.

mod comminution_execution;
mod definitions;
mod powered_physics;
mod screening_execution;
mod separation_execution;
mod throughput;

use crate::capability::{CapabilityComparison, CapabilityRegistry, CapabilityValueKind};
use crate::material::{
    CommodityKey, MaterialFormCohesion, MaterialPhase, MaterialRegistry, ParticleSizeStatePolicy,
};
use crate::production::{ProcessId, ProcessInputPolicy, ProductionRegistry};
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

fn validate_powered_process_contract(
    operation: &str,
    process: ProcessId,
    profile: PoweredOreProcessProfile,
    production: &ProductionRegistry,
    capabilities: &CapabilityRegistry,
) {
    let definition = production.get_process(process).unwrap_or_else(|| {
        panic!(
            "{operation} definition references missing process {}",
            process.value()
        )
    });
    assert!(
        matches!(definition.input_policy(), ProcessInputPolicy::SelectedBatch),
        "{operation} process {} must use selected-batch input policy",
        process.value()
    );
    for (capability, kind, role) in [
        (
            profile.mass_flow_capability(),
            CapabilityValueKind::MassFlow,
            "throughput",
        ),
        (
            profile.max_batch_mass_capability(),
            CapabilityValueKind::Mass,
            "maximum-batch",
        ),
    ] {
        let authored = capabilities.get_capability(capability).unwrap_or_else(|| {
            panic!(
                "{operation} process {} references missing {role} capability {}",
                process.value(),
                capability.value()
            )
        });
        assert_eq!(
            authored.kind(),
            kind,
            "{operation} process {} {role} capability has wrong physical kind",
            process.value()
        );
        let requirement = definition
            .get_capability_requirement(capability)
            .unwrap_or_else(|| {
                panic!(
                    "{operation} process {} must require its resolver-owned {role} capability {}",
                    process.value(),
                    capability.value()
                )
            });
        assert_eq!(
            requirement.comparison(),
            CapabilityComparison::AtLeast,
            "{operation} process {} resolver-owned {role} capability {} must use AtLeast comparison",
            process.value(),
            capability.value()
        );
    }
}

fn validate_comminution_references(
    definition: &ComminutionProcessDefinition,
    production: &ProductionRegistry,
    capabilities: &CapabilityRegistry,
    materials: &MaterialRegistry,
) {
    validate_powered_process_contract(
        "comminution",
        definition.process(),
        definition.operating_profile(),
        production,
        capabilities,
    );
    validate_comminution_material_references(
        definition.process(),
        definition.input_form(),
        definition.output_form(),
        definition.input_particle_size_range(),
        definition.output_particle_size(),
        materials,
    );
}

fn validate_comminution_material_references(
    process: ProcessId,
    input_form: crate::material::FormId,
    output_form: crate::material::FormId,
    input_particle_size_range: Option<crate::material::ParticleSizeRange>,
    output_particle_size: crate::material::ParticleSizeRange,
    materials: &MaterialRegistry,
) {
    for (form, role) in [(input_form, "input"), (output_form, "output")] {
        let authored = match materials.get_form(form) {
            Some(authored) => authored,
            None => panic!(
                "comminution process {} references missing {role} form {}",
                process.value(),
                form.value()
            ),
        };
        assert_eq!(
            authored.phase(),
            MaterialPhase::Solid,
            "comminution process {} {role} form {} must be solid",
            process.value(),
            form.value()
        );
    }
    let output_form_definition = match materials.get_form(output_form) {
        Some(output_form) => output_form,
        None => unreachable!("comminution output form was resolved above"),
    };
    assert_eq!(
        output_form_definition.particle_size_policy(),
        ParticleSizeStatePolicy::Required,
        "comminution process {} output form {} must require particle-size state",
        process.value(),
        output_form.value()
    );
    if let Some(input_range) = input_particle_size_range {
        let input_form_definition = match materials.get_form(input_form) {
            Some(input_form) => input_form,
            None => unreachable!("comminution input form was resolved above"),
        };
        assert_eq!(
            input_form_definition.particle_size_policy(),
            ParticleSizeStatePolicy::Required,
            "comminution process {} with a feed-size range requires particulate input form {}",
            process.value(),
            input_form.value()
        );
        assert!(
            output_particle_size.minimum_diameter() <= input_range.minimum_diameter()
                && output_particle_size.maximum_diameter() < input_range.maximum_diameter(),
            "comminution process {} feed-size range {}..={} um cannot admit a strictly reducing output {}..={} um",
            process.value(),
            input_range.minimum_diameter().micrometers(),
            input_range.maximum_diameter().micrometers(),
            output_particle_size.minimum_diameter().micrometers(),
            output_particle_size.maximum_diameter().micrometers()
        );
    }
}

fn validate_manual_comminution_references(
    definition: &ManualComminutionProcessDefinition,
    production: &ProductionRegistry,
    materials: &MaterialRegistry,
) {
    validate_manual_process_contract("manual comminution", definition.process(), production);
    validate_comminution_material_references(
        definition.process(),
        definition.input_form(),
        definition.output_form(),
        definition.input_particle_size_range(),
        definition.output_particle_size(),
        materials,
    );
}

fn validate_screening_references(
    definition: ScreeningProcessDefinition,
    production: &ProductionRegistry,
    capabilities: &CapabilityRegistry,
    materials: &MaterialRegistry,
) {
    validate_powered_process_contract(
        "screening",
        definition.process(),
        definition.operating_profile(),
        production,
        capabilities,
    );
    for (form, role) in [
        (definition.input_form(), "input"),
        (definition.output_form(), "output"),
    ] {
        let authored = materials.get_form(form).unwrap_or_else(|| {
            panic!(
                "screening process {} references missing {role} form {}",
                definition.process().value(),
                form.value()
            )
        });
        assert_eq!(
            authored.phase(),
            MaterialPhase::Solid,
            "screening process {} {role} form {} must be solid",
            definition.process().value(),
            form.value()
        );
        assert_eq!(
            authored.particle_size_policy(),
            ParticleSizeStatePolicy::Required,
            "screening process {} {role} form {} must require particle-size state",
            definition.process().value(),
            form.value()
        );
    }
}

fn validate_manual_process_contract(
    operation: &str,
    process: ProcessId,
    production: &ProductionRegistry,
) {
    let definition = production.get_process(process).unwrap_or_else(|| {
        panic!(
            "{operation} definition references missing process {}",
            process.value()
        )
    });
    assert!(
        matches!(definition.input_policy(), ProcessInputPolicy::SelectedBatch),
        "{operation} process {} must use selected-batch input policy",
        process.value()
    );
    assert!(
        definition.capability_requirements().is_empty(),
        "{operation} process {} is direct player labor and cannot require equipment capabilities",
        process.value()
    );
}

fn validate_separation_material_references(
    process: ProcessId,
    physics: definitions::ConstituentSeparationPhysics,
    materials: &MaterialRegistry,
) {
    let input_form = materials.get_form(physics.input_form()).unwrap_or_else(|| {
        panic!(
            "constituent-separation process {} references missing input form {}",
            process.value(),
            physics.input_form().value()
        )
    });
    assert_eq!(
        input_form.phase(),
        MaterialPhase::Solid,
        "constituent-separation process {} input must be solid",
        process.value()
    );
    assert_eq!(
        input_form.particle_size_policy(),
        ParticleSizeStatePolicy::Required,
        "constituent-separation process {} requires liberated particulate feed",
        process.value()
    );
    let target_material = physics.target_material();
    let target_form = physics.target_output_form();
    assert!(
        materials.get_material(target_material).is_some(),
        "constituent-separation process {} references missing target material {}",
        process.value(),
        target_material.value()
    );
    assert!(
        materials.has_commodity(CommodityKey::new(target_material, target_form)),
        "constituent-separation process {} references invalid target material/form {}:{}",
        process.value(),
        target_material.value(),
        target_form.value()
    );
    let target_output = materials
        .get_form(target_form)
        .unwrap_or_else(|| unreachable!("validated target commodity requires its form"));
    assert_eq!(target_output.phase(), MaterialPhase::Solid);
    assert_eq!(
        target_output.cohesion(),
        MaterialFormCohesion::Loose,
        "constituent-separation process {} target output form {} cannot become consolidated without an explicit consolidation operation",
        process.value(),
        target_form.value()
    );
    if physics.is_concentration() {
        assert_eq!(
            target_output.particle_size_policy(),
            ParticleSizeStatePolicy::Required,
            "constituent concentration process {} target output must retain particle-size state",
            process.value()
        );
    }

    let residue_form = physics.residue_output_form();
    let residue_output = materials.get_form(residue_form).unwrap_or_else(|| {
        panic!(
            "constituent-separation process {} references missing residue form {}",
            process.value(),
            residue_form.value()
        )
    });
    assert_eq!(residue_output.phase(), MaterialPhase::Solid);
    assert_eq!(
        residue_output.particle_size_policy(),
        ParticleSizeStatePolicy::Required,
        "constituent-separation process {} residue output must retain particle-size state",
        process.value()
    );
    assert!(
        materials.definitions().any(|material| {
            material.id() != target_material
                && materials.has_commodity(CommodityKey::new(material.id(), residue_form))
        }),
        "constituent-separation process {} residue form {} has no authored non-target material commodity",
        process.value(),
        residue_form.value()
    );
}

fn validate_separation_references(
    definition: ConstituentSeparationProcessDefinition,
    production: &ProductionRegistry,
    capabilities: &CapabilityRegistry,
    materials: &MaterialRegistry,
) {
    validate_powered_process_contract(
        "constituent-separation",
        definition.process(),
        definition.operating_profile(),
        production,
        capabilities,
    );
    validate_separation_material_references(definition.process(), definition.physics(), materials);
}

fn validate_manual_separation_references(
    definition: ManualConstituentSeparationProcessDefinition,
    production: &ProductionRegistry,
    materials: &MaterialRegistry,
) {
    validate_manual_process_contract(
        "manual constituent-separation",
        definition.process(),
        production,
    );
    validate_separation_material_references(definition.process(), definition.physics(), materials);
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

    pub(crate) fn validate_references(
        &self,
        production: &ProductionRegistry,
        capabilities: &CapabilityRegistry,
        materials: &MaterialRegistry,
    ) {
        for definition in self.comminution.values() {
            validate_comminution_references(definition, production, capabilities, materials);
        }
        for definition in self.manual_comminution.values() {
            validate_manual_comminution_references(definition, production, materials);
        }
        for definition in self.screening.values().copied() {
            validate_screening_references(definition, production, capabilities, materials);
        }
        for definition in self.separation.values().copied() {
            validate_separation_references(definition, production, capabilities, materials);
        }
        for definition in self.manual_separation.values().copied() {
            validate_manual_separation_references(definition, production, materials);
        }
    }
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
