//! Cross-registry validation for authored equipment definitions.

use crate::capability::CapabilityRegistry;
use crate::material::{CommodityKey, MaterialPhase, MaterialRegistry, ParticleSizeStatePolicy};

use super::{EquipmentDefinition, EquipmentRegistry};

mod upgrade;

use upgrade::{validate_equipment_upgrade_ancestry, validate_equipment_upgrade_references};

fn validate_equipment_capability_references(
    definition: &EquipmentDefinition,
    capabilities: &CapabilityRegistry,
) {
    for (capability, value) in definition.capabilities().entries() {
        let Some(capability_definition) = capabilities.get_capability(capability) else {
            panic!(
                "equipment definition {} references missing capability {}",
                definition.id().value(),
                capability.value()
            );
        };
        assert_eq!(
            value.kind(),
            capability_definition.kind(),
            "equipment definition {} capability {} has wrong physical value kind",
            definition.id().value(),
            capability.value()
        );
        let Some(curve) = definition.get_capability_condition_curve(capability) else {
            continue;
        };
        let improvement = capability_definition.improvement().unwrap_or_else(|| {
            panic!(
                "equipment definition {} condition curve for capability {} requires an authored improvement direction",
                definition.id().value(),
                capability.value()
            )
        });
        for point in curve.points() {
            let ordering = point.value().compare(value).unwrap_or_else(|| {
                unreachable!("validated condition curve and nominal capability kinds match")
            });
            assert!(
                !improvement.is_improvement(ordering),
                "equipment definition {} condition curve makes capability {} better than its pristine value at {} ppm condition",
                definition.id().value(),
                capability.value(),
                point.condition().parts_per_million()
            );
        }
    }
}

fn validate_equipment_maintenance_references(
    definition: &EquipmentDefinition,
    materials: &MaterialRegistry,
) {
    let Some(maintenance) = definition.maintenance_profile() else {
        return;
    };
    if maintenance.is_component_replacement() {
        let assembly = definition.assembly_profile().unwrap_or_else(|| {
            panic!(
                "equipment definition {} component maintenance requires an assembly profile",
                definition.id().value()
            )
        });
        let matching = assembly
            .inputs()
            .iter()
            .find(|input| input.commodity() == maintenance.replacement())
            .unwrap_or_else(|| {
                panic!(
                    "equipment definition {} component maintenance replacement must identify an assembly input",
                    definition.id().value()
                )
            });
        assert_eq!(
            matching.mass(),
            maintenance.full_service_replacement_mass(),
            "equipment definition {} component maintenance mass must equal the complete authored assembly component mass",
            definition.id().value()
        );
    } else {
        assert!(
            definition.assembly_profile().is_none(),
            "equipment definition {} cannot apply aggregate maintenance to exact assembly traces",
            definition.id().value()
        );
    }
    for commodity in [maintenance.replacement(), maintenance.spent()] {
        assert!(
            materials.has_commodity(commodity),
            "equipment definition {} maintenance profile references unauthored material {} form {}",
            definition.id().value(),
            commodity.material().value(),
            commodity.form().value()
        );
    }
    let replacement_form = materials
        .get_form(maintenance.replacement().form())
        .unwrap_or_else(|| {
            unreachable!("validated maintenance replacement commodity has its form")
        });
    let spent_form = materials
        .get_form(maintenance.spent().form())
        .unwrap_or_else(|| unreachable!("validated maintenance spent commodity has its form"));
    assert_eq!(
        replacement_form.phase(),
        spent_form.phase(),
        "equipment definition {} maintenance cannot change material phase without a thermal process",
        definition.id().value()
    );
    assert_eq!(
        replacement_form.particle_size_policy(),
        spent_form.particle_size_policy(),
        "equipment definition {} maintenance cannot change particle-size state without a particle-transform process",
        definition.id().value()
    );
}

fn validate_equipment_assembly_references(
    definition: &EquipmentDefinition,
    materials: &MaterialRegistry,
) {
    let Some(assembly) = definition.assembly_profile() else {
        return;
    };
    assert_eq!(
        assembly.input_mass(),
        definition.mass(),
        "equipment definition {} assembly mass disagrees with authored equipment mass",
        definition.id().value()
    );
    assert!(
        assembly
            .validate_infrastructure_references(materials)
            .is_ok(),
        "equipment definition {} assembly profile must use existing consolidated solid commodities",
        definition.id().value()
    );
}

fn validate_worn_recovery_references(
    definition: &EquipmentDefinition,
    materials: &MaterialRegistry,
) {
    let Some(recovery_form) = definition.worn_recovery_form() else {
        return;
    };
    let assembly = definition.assembly_profile().unwrap_or_else(|| {
        panic!(
            "equipment definition {} has worn recovery but no assembly profile",
            definition.id().value()
        )
    });
    let form = materials.get_form(recovery_form).unwrap_or_else(|| {
        panic!(
            "equipment definition {} references missing worn-recovery form {}",
            definition.id().value(),
            recovery_form.value()
        )
    });
    assert_eq!(
        form.phase(),
        MaterialPhase::Solid,
        "equipment definition {} worn-recovery form {} must be solid",
        definition.id().value(),
        recovery_form.value()
    );
    assert_eq!(
        form.particle_size_policy(),
        ParticleSizeStatePolicy::Untracked,
        "equipment definition {} worn-recovery form {} must not require particulate state",
        definition.id().value(),
        recovery_form.value()
    );
    assert!(
        assembly
            .inputs()
            .iter()
            .all(|input| input.commodity().form() != recovery_form),
        "equipment definition {} worn-recovery form {} cannot also be a direct assembly input",
        definition.id().value(),
        recovery_form.value()
    );
    assert!(
        assembly.inputs().iter().all(|input| {
            materials.has_commodity(CommodityKey::new(
                input.commodity().material(),
                recovery_form,
            ))
        }),
        "equipment definition {} worn-recovery form {} must be authored for every embodied assembly material",
        definition.id().value(),
        recovery_form.value()
    );
}

impl EquipmentRegistry {
    pub(crate) fn validate_references(
        &self,
        capabilities: &CapabilityRegistry,
        materials: &MaterialRegistry,
    ) {
        for definition in self.definitions.values() {
            validate_equipment_capability_references(definition, capabilities);
            validate_equipment_maintenance_references(definition, materials);
            validate_equipment_assembly_references(definition, materials);
            validate_worn_recovery_references(definition, materials);
        }

        validate_equipment_upgrade_ancestry(self);
        for target in self.definitions.values() {
            validate_equipment_upgrade_references(self, target, capabilities, materials);
        }
    }
}
