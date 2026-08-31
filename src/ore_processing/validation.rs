//! Cross-registry validation for authored ore-processing semantics.

use crate::capability::{CapabilityComparison, CapabilityRegistry, CapabilityValueKind};
use crate::material::{
    CommodityKey, FormId, MaterialFormCohesion, MaterialPhase, MaterialRegistry, ParticleSizeRange,
    ParticleSizeStatePolicy,
};
use crate::production::{ProcessId, ProcessInputPolicy, ProductionRegistry};

use super::definitions::ConstituentSeparationPhysics;
use super::{
    ComminutionProcessDefinition, ConstituentSeparationProcessDefinition,
    ManualComminutionProcessDefinition, ManualConstituentSeparationProcessDefinition,
    OreProcessingRegistry, PoweredOreProcessProfile, ScreeningProcessDefinition,
};

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

fn validate_comminution_material_references(
    process: ProcessId,
    input_form: FormId,
    output_form: FormId,
    input_particle_size_range: Option<ParticleSizeRange>,
    output_particle_size: ParticleSizeRange,
    materials: &MaterialRegistry,
) {
    for (form, role) in [(input_form, "input"), (output_form, "output")] {
        let authored = materials.get_form(form).unwrap_or_else(|| {
            panic!(
                "comminution process {} references missing {role} form {}",
                process.value(),
                form.value()
            )
        });
        assert_eq!(
            authored.phase(),
            MaterialPhase::Solid,
            "comminution process {} {role} form {} must be solid",
            process.value(),
            form.value()
        );
    }
    let output_form_definition = materials
        .get_form(output_form)
        .unwrap_or_else(|| unreachable!("comminution output form was resolved above"));
    assert_eq!(
        output_form_definition.particle_size_policy(),
        ParticleSizeStatePolicy::Required,
        "comminution process {} output form {} must require particle-size state",
        process.value(),
        output_form.value()
    );
    if let Some(input_range) = input_particle_size_range {
        let input_form_definition = materials
            .get_form(input_form)
            .unwrap_or_else(|| unreachable!("comminution input form was resolved above"));
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

fn validate_separation_material_references(
    process: ProcessId,
    physics: ConstituentSeparationPhysics,
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
    assert_eq!(
        target_output.phase(),
        MaterialPhase::Solid,
        "constituent-separation process {} target output form {} must be solid",
        process.value(),
        target_form.value()
    );
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
    assert_eq!(
        residue_output.phase(),
        MaterialPhase::Solid,
        "constituent-separation process {} residue output form {} must be solid",
        process.value(),
        residue_form.value()
    );
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
