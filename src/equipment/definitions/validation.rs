//! Cross-registry validation for authored equipment definitions.

use std::collections::BTreeMap;

use crate::capability::CapabilityRegistry;
use crate::core::quantity::Mass;
use crate::material::{CommodityKey, MaterialPhase, MaterialRegistry, ParticleSizeStatePolicy};

use super::{EquipmentDefinition, EquipmentRegistry};

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
            .filter(|input| input.commodity() == maintenance.replacement())
            .collect::<Vec<_>>();
        assert_eq!(
            matching.len(),
            1,
            "equipment definition {} component maintenance replacement must identify exactly one assembly input",
            definition.id().value()
        );
        assert_eq!(
            matching[0].mass(),
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

fn validate_equipment_upgrade_references(
    registry: &EquipmentRegistry,
    target: &EquipmentDefinition,
    materials: &MaterialRegistry,
) {
    let Some(upgrade) = target.upgrade_profile() else {
        return;
    };
    let base = registry
        .definitions
        .get(&upgrade.from())
        .unwrap_or_else(|| {
            panic!(
                "equipment definition {} upgrade references missing base definition {}",
                target.id().value(),
                upgrade.from().value()
            )
        });
    assert!(
        upgrade
            .additions()
            .validate_infrastructure_references(materials)
            .is_ok(),
        "equipment definition {} upgrade additions must use existing consolidated solid commodities",
        target.id().value()
    );
    let base_assembly = base.assembly_profile().unwrap_or_else(|| {
        panic!(
            "equipment definition {} upgrade base {} has no material assembly profile",
            target.id().value(),
            base.id().value()
        )
    });
    let target_assembly = target.assembly_profile().unwrap_or_else(|| {
        panic!(
            "equipment definition {} has an upgrade profile but no material assembly profile",
            target.id().value()
        )
    });
    let expected_mass = base
        .mass()
        .checked_add(upgrade.additions().input_mass())
        .unwrap_or_else(|| {
            panic!(
                "equipment definition {} upgrade mass overflows",
                target.id().value()
            )
        });
    assert_eq!(
        target.mass(),
        expected_mass,
        "equipment definition {} upgrade mass must equal base mass plus additive material",
        target.id().value()
    );

    let mut expected_inputs = BTreeMap::new();
    for input in base_assembly
        .inputs()
        .iter()
        .chain(upgrade.additions().inputs())
    {
        let previous = expected_inputs
            .get(&input.commodity())
            .copied()
            .unwrap_or(Mass::ZERO);
        let combined = previous.checked_add(input.mass()).unwrap_or_else(|| {
            panic!(
                "equipment definition {} upgrade material quantity overflows for commodity {}",
                target.id().value(),
                input.commodity().value()
            )
        });
        expected_inputs.insert(input.commodity(), combined);
    }
    assert_eq!(
        expected_inputs.len(),
        target_assembly.inputs().len(),
        "equipment definition {} upgrade target assembly has extra or missing commodities",
        target.id().value()
    );
    for input in target_assembly.inputs() {
        assert_eq!(
            expected_inputs.get(&input.commodity()).copied(),
            Some(input.mass()),
            "equipment definition {} upgrade target assembly disagrees with base plus additive material for commodity {}",
            target.id().value(),
            input.commodity().value()
        );
    }
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

        for target in self.definitions.values() {
            validate_equipment_upgrade_references(self, target, materials);
        }
    }
}
