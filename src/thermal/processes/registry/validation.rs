//! Validates thermal resolver references against production, capability, and material authority.

use crate::capability::{
    CapabilityComparison, CapabilityId, CapabilityRegistry, CapabilityValueKind,
};
use crate::material::{CommodityKey, MaterialPhase, MaterialRegistry, ParticleSizeStatePolicy};
use crate::production::{ProcessId, ProcessInputPolicy, ProductionRegistry};

use super::{CastingProcessDefinition, MeltingProcessDefinition};

pub(super) fn validate_casting_material_references(
    definition: CastingProcessDefinition,
    materials: &MaterialRegistry,
) {
    let liquid_form = materials
        .get_form(definition.liquid_form())
        .unwrap_or_else(|| {
            panic!(
                "casting process {} references missing input form {}",
                definition.process().value(),
                definition.liquid_form().value()
            )
        });
    assert_eq!(
        liquid_form.phase(),
        MaterialPhase::Liquid,
        "casting process {} input form {} must be liquid",
        definition.process().value(),
        definition.liquid_form().value()
    );
    let solid_form = materials
        .get_form(definition.solid_form())
        .unwrap_or_else(|| {
            panic!(
                "casting process {} references missing output form {}",
                definition.process().value(),
                definition.solid_form().value()
            )
        });
    assert_eq!(
        solid_form.phase(),
        MaterialPhase::Solid,
        "casting process {} output form {} must be solid",
        definition.process().value(),
        definition.solid_form().value()
    );
    assert_eq!(
        solid_form.particle_size_policy(),
        ParticleSizeStatePolicy::Untracked,
        "casting process {} output form {} cannot require particle-size state because casting has no authored particulate output distribution",
        definition.process().value(),
        definition.solid_form().value()
    );
    validate_casting_applicable_materials(definition, materials);
}

fn validate_casting_applicable_materials(
    definition: CastingProcessDefinition,
    materials: &MaterialRegistry,
) {
    let material = materials
        .get_material(definition.material())
        .unwrap_or_else(|| {
            panic!(
                "casting process {} references unknown material {}",
                definition.process().value(),
                definition.material().value()
            )
        });
    assert!(
        materials.has_commodity(CommodityKey::new(
            definition.material(),
            definition.liquid_form()
        )) && materials.has_commodity(CommodityKey::new(
            definition.material(),
            definition.solid_form()
        )),
        "casting process {} material {} must be authored in both input form {} and output form {}",
        definition.process().value(),
        definition.material().value(),
        definition.liquid_form().value(),
        definition.solid_form().value()
    );
    let melting_point = material
        .properties()
        .thermal()
        .melting_point()
        .unwrap_or_else(|| {
            panic!(
                "casting process {} material {} has no fusion properties",
                definition.process().value(),
                definition.material().value()
            )
        });
    assert!(
        definition.output_temperature() <= melting_point,
        "casting process {} output temperature {} mK exceeds material {} melting point {} mK",
        definition.process().value(),
        definition.output_temperature().millikelvin(),
        definition.material().value(),
        melting_point.millikelvin()
    );
}

pub(super) fn validate_melting_form_references(
    definition: &MeltingProcessDefinition,
    materials: &MaterialRegistry,
) {
    let material = materials
        .get_material(definition.material())
        .unwrap_or_else(|| {
            panic!(
                "melting process {} references unknown material {}",
                definition.process().value(),
                definition.material().value()
            )
        });
    assert!(
        material.properties().thermal().melting_point().is_some(),
        "melting process {} material {} has no fusion properties",
        definition.process().value(),
        definition.material().value()
    );
    for &solid_form_id in definition.solid_forms() {
        let solid_form = materials.get_form(solid_form_id).unwrap_or_else(|| {
            panic!(
                "melting process {} references missing input form {}",
                definition.process().value(),
                solid_form_id.value()
            )
        });
        assert_eq!(
            solid_form.phase(),
            MaterialPhase::Solid,
            "melting process {} input form {} must be solid",
            definition.process().value(),
            solid_form_id.value()
        );
        assert!(
            materials.has_commodity(CommodityKey::new(definition.material(), solid_form_id)),
            "melting process {} material {} is not authored in accepted input form {}",
            definition.process().value(),
            definition.material().value(),
            solid_form_id.value()
        );
    }
    let liquid_form = materials
        .get_form(definition.liquid_form())
        .unwrap_or_else(|| {
            panic!(
                "melting process {} references missing output form {}",
                definition.process().value(),
                definition.liquid_form().value()
            )
        });
    assert_eq!(
        liquid_form.phase(),
        MaterialPhase::Liquid,
        "melting process {} output form {} must be liquid",
        definition.process().value(),
        definition.liquid_form().value()
    );
    assert!(
        materials.has_commodity(CommodityKey::new(
            definition.material(),
            definition.liquid_form()
        )),
        "melting process {} material {} is not authored in output form {}",
        definition.process().value(),
        definition.material().value(),
        definition.liquid_form().value()
    );
}

pub(super) fn validate_common_thermal_references(
    process: ProcessId,
    thermal_power_capability: CapabilityId,
    max_temperature_capability: CapabilityId,
    max_batch_mass_capability: CapabilityId,
    production: &ProductionRegistry,
    capabilities: &CapabilityRegistry,
) {
    let process_definition = match production.get_process(process) {
        Some(definition) => definition,
        None => panic!(
            "thermal definition references missing process {}",
            process.value()
        ),
    };
    assert!(
        matches!(
            process_definition.input_policy(),
            ProcessInputPolicy::SelectedBatch
        ),
        "thermal process {} must use selected-batch input policy",
        process.value()
    );
    let power = match capabilities.get_capability(thermal_power_capability) {
        Some(capability) => capability,
        None => panic!(
            "thermal process {} references missing thermal-transfer-power capability {}",
            process.value(),
            thermal_power_capability.value()
        ),
    };
    assert_eq!(
        power.kind(),
        CapabilityValueKind::Power,
        "thermal process {} thermal-transfer-power capability must be Power",
        process.value()
    );
    let maximum = match capabilities.get_capability(max_temperature_capability) {
        Some(capability) => capability,
        None => panic!(
            "thermal process {} references missing maximum-temperature capability {}",
            process.value(),
            max_temperature_capability.value()
        ),
    };
    assert_eq!(
        maximum.kind(),
        CapabilityValueKind::Temperature,
        "thermal process {} maximum-temperature capability must be Temperature",
        process.value()
    );
    let maximum_batch = match capabilities.get_capability(max_batch_mass_capability) {
        Some(capability) => capability,
        None => panic!(
            "thermal process {} references missing maximum-batch-mass capability {}",
            process.value(),
            max_batch_mass_capability.value()
        ),
    };
    assert_eq!(
        maximum_batch.kind(),
        CapabilityValueKind::Mass,
        "thermal process {} maximum-batch-mass capability must be Mass",
        process.value()
    );
    for (capability, role) in [
        (thermal_power_capability, "thermal-transfer-power"),
        (max_temperature_capability, "maximum-temperature"),
        (max_batch_mass_capability, "maximum-batch-mass"),
    ] {
        let requirement = process_definition
            .get_capability_requirement(capability)
            .unwrap_or_else(|| {
                panic!(
                    "thermal process {} must require its resolver-owned {role} capability {}",
                    process.value(),
                    capability.value()
                )
            });
        assert_eq!(
            requirement.comparison(),
            CapabilityComparison::AtLeast,
            "thermal process {} resolver-owned {role} capability {} must use AtLeast comparison",
            process.value(),
            capability.value()
        );
    }
}
