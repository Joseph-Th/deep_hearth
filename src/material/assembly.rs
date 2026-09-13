//! Shared exact-material assembly profiles for persistent physical infrastructure.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use crate::core::quantity::Mass;

use super::{CommodityKey, MaterialInputSpec, MaterialRegistry};

/// Authored assembly matter must describe consolidated solid object material.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MaterialAssemblyReferenceError {
    UnknownCommodity { commodity: CommodityKey },
    UnconsolidatedForm { commodity: CommodityKey },
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
            assert!(
                input.requires_pure_material(),
                "material assembly commodity {} must require pure host material",
                input.commodity().value()
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

    /// Returns whether this profile is exactly the base profile plus authored additive material.
    ///
    /// Assembly profiles are stored in unique commodity order, so additive-upgrade validation can
    /// merge the two source profiles without allocating a second commodity map. Shared commodities
    /// must conserve their summed mass exactly; arithmetic overflow is never a valid extension.
    pub(crate) fn is_exact_additive_extension_of(&self, base: &Self, additions: &Self) -> bool {
        let mut base_inputs = base.inputs.iter().peekable();
        let mut addition_inputs = additions.inputs.iter().peekable();
        let mut target_inputs = self.inputs.iter();

        loop {
            let expected = match (base_inputs.peek().copied(), addition_inputs.peek().copied()) {
                (Some(base_input), Some(addition_input)) => {
                    match base_input.commodity().cmp(&addition_input.commodity()) {
                        Ordering::Less => {
                            let _ = base_inputs.next();
                            Some((base_input.commodity(), base_input.mass()))
                        }
                        Ordering::Greater => {
                            let _ = addition_inputs.next();
                            Some((addition_input.commodity(), addition_input.mass()))
                        }
                        Ordering::Equal => {
                            let _ = base_inputs.next();
                            let _ = addition_inputs.next();
                            let Some(mass) = base_input.mass().checked_add(addition_input.mass())
                            else {
                                return false;
                            };
                            Some((base_input.commodity(), mass))
                        }
                    }
                }
                (Some(base_input), None) => {
                    let _ = base_inputs.next();
                    Some((base_input.commodity(), base_input.mass()))
                }
                (None, Some(addition_input)) => {
                    let _ = addition_inputs.next();
                    Some((addition_input.commodity(), addition_input.mass()))
                }
                (None, None) => None,
            };
            let Some((commodity, mass)) = expected else {
                break;
            };
            let Some(target_input) = target_inputs.next() else {
                return false;
            };
            if target_input.commodity() != commodity || target_input.mass() != mass {
                return false;
            }
        }

        target_inputs.next().is_none()
    }

    /// Returns the first commodity whose actual mass differs from this exact assembly profile.
    ///
    /// Both representations are canonically ordered, so the comparison is linear and does not
    /// destructively remove entries from the caller's aggregate map.
    pub(crate) fn first_mass_mismatch(
        &self,
        actual: &BTreeMap<CommodityKey, Mass>,
    ) -> Option<(CommodityKey, Mass, Mass)> {
        let mut expected = self
            .inputs
            .iter()
            .map(|input| (input.commodity(), input.mass()))
            .peekable();
        let mut actual = actual
            .iter()
            .map(|(&commodity, &mass)| (commodity, mass))
            .peekable();

        loop {
            match (expected.peek().copied(), actual.peek().copied()) {
                (
                    Some((expected_commodity, expected_mass)),
                    Some((actual_commodity, actual_mass)),
                ) => match expected_commodity.cmp(&actual_commodity) {
                    Ordering::Less => {
                        return Some((expected_commodity, Mass::ZERO, expected_mass));
                    }
                    Ordering::Greater => {
                        return Some((actual_commodity, actual_mass, Mass::ZERO));
                    }
                    Ordering::Equal => {
                        let _ = expected.next();
                        let _ = actual.next();
                        if actual_mass != expected_mass {
                            return Some((expected_commodity, actual_mass, expected_mass));
                        }
                    }
                },
                (Some((commodity, expected_mass)), None) => {
                    return Some((commodity, Mass::ZERO, expected_mass));
                }
                (None, Some((commodity, actual_mass))) => {
                    return Some((commodity, actual_mass, Mass::ZERO));
                }
                (None, None) => return None,
            }
        }
    }

    /// Validates references and the physical-form boundary shared by persistent rigid assemblies.
    ///
    /// Equipment and energy-store embodiment has no shaping, compaction, casting, or sintering
    /// owner. Components must therefore use forms explicitly authored as consolidated.
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
            if !form.is_consolidated() {
                return Err(MaterialAssemblyReferenceError::UnconsolidatedForm { commodity });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "assembly_tests.rs"]
mod tests;
