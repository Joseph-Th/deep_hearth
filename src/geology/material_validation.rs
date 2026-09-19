//! Shared registry-resolved material invariants for finite geological deposits.

use crate::core::quantity::Temperature;
use crate::material::{
    CommodityKey, FormId, MaterialComposition, MaterialId, MaterialPhase, MaterialPhaseStateError,
    MaterialRegistry, ParticleSizeStatePolicy, validate_material_phase_state,
};

/// Registry-resolved material-state failure shared by generation admission and trusted load.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum GeologicalMaterialStateError {
    UnknownCommodityMaterial { material: MaterialId },
    UnknownCommodityForm { form: FormId },
    UnsupportedCommodity { commodity: CommodityKey },
    UnsupportedCommodityPhase { form: FormId, phase: MaterialPhase },
    UnsupportedCommodityParticulateForm { form: FormId },
    UnknownCompositionMaterial { material: MaterialId },
    InvalidPhaseState(MaterialPhaseStateError),
}

/// Validates the registry-dependent physical material profile common to every geological owner.
///
/// Registry-independent born invariants such as nonzero mass, nonzero excavation hardness, valid
/// composition totals, and host-material presence remain owned by the creating specification or
/// trusted-load record validation. This function owns the registry-resolved constraints so runtime
/// admission and persistence replay cannot drift into accepting different geological matter.
pub(super) fn validate_geological_material_state(
    materials: &MaterialRegistry,
    commodity: CommodityKey,
    composition: &MaterialComposition,
    temperature: Temperature,
) -> Result<(), GeologicalMaterialStateError> {
    if materials.get_material(commodity.material()).is_none() {
        return Err(GeologicalMaterialStateError::UnknownCommodityMaterial {
            material: commodity.material(),
        });
    }
    let Some(form) = materials.get_form(commodity.form()) else {
        return Err(GeologicalMaterialStateError::UnknownCommodityForm {
            form: commodity.form(),
        });
    };
    if !materials.has_commodity(commodity) {
        return Err(GeologicalMaterialStateError::UnsupportedCommodity { commodity });
    }
    if form.phase() != MaterialPhase::Solid {
        return Err(GeologicalMaterialStateError::UnsupportedCommodityPhase {
            form: commodity.form(),
            phase: form.phase(),
        });
    }
    if form.particle_size_policy() == ParticleSizeStatePolicy::Required {
        return Err(
            GeologicalMaterialStateError::UnsupportedCommodityParticulateForm {
                form: commodity.form(),
            },
        );
    }
    for component in composition.components() {
        if materials.get_material(component.material()).is_none() {
            return Err(GeologicalMaterialStateError::UnknownCompositionMaterial {
                material: component.material(),
            });
        }
    }
    validate_material_phase_state(materials, commodity, composition, temperature)
        .map_err(GeologicalMaterialStateError::InvalidPhaseState)
}
