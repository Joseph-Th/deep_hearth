//! Shared exact-material assembly profiles for persistent physical infrastructure.

use crate::core::quantity::Mass;

use super::{
    CommodityKey, MaterialInputSpec, MaterialPhase, MaterialRegistry, ParticleSizeStatePolicy,
};

/// Authored assembly matter must describe consolidated solid object material.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MaterialAssemblyReferenceError {
    UnknownCommodity {
        commodity: CommodityKey,
    },
    UnsupportedPhase {
        commodity: CommodityKey,
        phase: MaterialPhase,
    },
    UnsupportedParticulateForm {
        commodity: CommodityKey,
    },
}

/// Exact pure-material inputs required to materialize one persistent physical object.
///
/// The profile is intentionally owner-neutral: equipment and energy-storage infrastructure use the
/// same conserved-material contract instead of maintaining parallel recipe-like assembly types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterialAssemblyProfile {
    inputs: Vec<MaterialInputSpec>,
    input_mass: Mass,
}

impl MaterialAssemblyProfile {
    #[must_use]
    pub fn new(mut inputs: Vec<MaterialInputSpec>) -> Self {
        assert!(
            !inputs.is_empty(),
            "material assembly profile must contain material inputs"
        );
        inputs.sort();
        for pair in inputs.windows(2) {
            assert_ne!(
                pair[0].commodity(),
                pair[1].commodity(),
                "material assembly profile contains duplicate commodity {}",
                pair[0].commodity().value()
            );
        }
        let mut input_mass = Mass::ZERO;
        for input in &inputs {
            assert!(
                !input.mass().is_zero(),
                "material assembly input mass must be nonzero"
            );
            input_mass = input_mass
                .checked_add(input.mass())
                .unwrap_or_else(|| panic!("material assembly input mass overflows"));
        }
        Self { inputs, input_mass }
    }

    #[must_use]
    pub fn inputs(&self) -> &[MaterialInputSpec] {
        &self.inputs
    }

    #[must_use]
    pub const fn input_mass(&self) -> Mass {
        self.input_mass
    }

    /// Validates references and the physical-form boundary shared by persistent rigid assemblies.
    ///
    /// Equipment and energy-store embodiment currently have no internal fluid-container, binder,
    /// compaction, or sintering owner. Liquid and explicitly particulate forms therefore cannot be
    /// authored as if they were already consolidated object components.
    pub(crate) fn validate_infrastructure_references(
        &self,
        materials: &MaterialRegistry,
    ) -> Result<(), MaterialAssemblyReferenceError> {
        for input in &self.inputs {
            let commodity = input.commodity();
            if !materials.has_commodity(commodity) {
                return Err(MaterialAssemblyReferenceError::UnknownCommodity { commodity });
            }
            let form = materials
                .get_form(commodity.form())
                .unwrap_or_else(|| unreachable!("resolved commodity has a form definition"));
            if form.phase() != MaterialPhase::Solid {
                return Err(MaterialAssemblyReferenceError::UnsupportedPhase {
                    commodity,
                    phase: form.phase(),
                });
            }
            if form.particle_size_policy() == ParticleSizeStatePolicy::Required {
                return Err(MaterialAssemblyReferenceError::UnsupportedParticulateForm {
                    commodity,
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{FORM_CRUSHED, FORM_MOLTEN, MATERIAL_COPPER, build_registries};

    #[test]
    fn infrastructure_assembly_rejects_liquid_and_particulate_forms() {
        let registries = build_registries();
        let liquid = CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN);
        let particulate = CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED);

        assert_eq!(
            MaterialAssemblyProfile::new(vec![MaterialInputSpec::new(
                liquid,
                Mass::from_milligrams(1),
            )])
            .validate_infrastructure_references(registries.materials()),
            Err(MaterialAssemblyReferenceError::UnsupportedPhase {
                commodity: liquid,
                phase: MaterialPhase::Liquid,
            })
        );
        assert_eq!(
            MaterialAssemblyProfile::new(vec![MaterialInputSpec::new(
                particulate,
                Mass::from_milligrams(1),
            )])
            .validate_infrastructure_references(registries.materials()),
            Err(MaterialAssemblyReferenceError::UnsupportedParticulateForm {
                commodity: particulate,
            })
        );
    }
}
