//! Validation and exact grouping of constituent-separation input traces.

use std::collections::BTreeMap;

use crate::core::quantity::{Mass, Temperature};
use crate::inventory::ConsumedMaterialTrace;
use crate::material::{CommodityKey, MaterialId, MaterialRegistry, ParticleSizeDistribution};
use crate::ore_processing::definitions::ConstituentSeparationPhysics;

use super::super::ConstituentSeparationBatchError;

pub(super) type SeparationInputKey = (Temperature, ParticleSizeDistribution);

#[derive(Debug)]
pub(super) struct ExactInputProfile {
    pub(super) mass: Mass,
    pub(super) constituent_numerators: BTreeMap<MaterialId, u128>,
}

impl ExactInputProfile {
    fn new() -> Self {
        Self {
            mass: Mass::ZERO,
            constituent_numerators: BTreeMap::new(),
        }
    }

    fn add_trace(
        &mut self,
        trace: &ConsumedMaterialTrace,
    ) -> Result<(), ConstituentSeparationBatchError> {
        self.mass = self
            .mass
            .checked_add(trace.mass())
            .ok_or(ConstituentSeparationBatchError::MassOverflow)?;
        for component in trace.profile().composition().components() {
            let numerator =
                u128::from(trace.mass().milligrams()) * u128::from(component.parts_per_million());
            let current = self
                .constituent_numerators
                .get(&component.material())
                .copied()
                .unwrap_or(0);
            self.constituent_numerators.insert(
                component.material(),
                current
                    .checked_add(numerator)
                    .ok_or(ConstituentSeparationBatchError::MassOverflow)?,
            );
        }
        Ok(())
    }
}

pub(super) struct CollectedInputs {
    pub(super) selected_mass: Mass,
    pub(super) grouped: BTreeMap<SeparationInputKey, ExactInputProfile>,
}

fn validate_input_identity(
    definition: ConstituentSeparationPhysics,
    trace: &ConsumedMaterialTrace,
) -> Result<(), ConstituentSeparationBatchError> {
    let profile = trace.profile();
    if profile.commodity().form() != definition.input_form() {
        return Err(ConstituentSeparationBatchError::InputFormMismatch {
            expected: definition.input_form(),
            found: profile.commodity().form(),
        });
    }
    if definition.is_sorting() && profile.commodity().material() != definition.target_material() {
        return Err(
            ConstituentSeparationBatchError::SortingInputHostMaterialMismatch {
                expected: definition.target_material(),
                found: profile.commodity().material(),
            },
        );
    }
    Ok(())
}

fn validate_input_constituents(
    materials: &MaterialRegistry,
    definition: ConstituentSeparationPhysics,
    trace: &ConsumedMaterialTrace,
) -> Result<(), ConstituentSeparationBatchError> {
    let profile = trace.profile();
    let mut has_non_target = false;
    for component in profile.composition().components() {
        if component.material() == definition.target_material() {
            continue;
        }
        has_non_target = true;
        if !materials.has_commodity(CommodityKey::new(
            component.material(),
            definition.residue_output_form(),
        )) {
            return Err(ConstituentSeparationBatchError::UnsupportedResidueForm {
                material: component.material(),
                form: definition.residue_output_form(),
            });
        }
    }
    if profile
        .composition()
        .parts_per_million(definition.target_material())
        == 0
    {
        return Err(ConstituentSeparationBatchError::MissingTargetConstituent {
            material: definition.target_material(),
        });
    }
    if !has_non_target {
        return Err(ConstituentSeparationBatchError::MissingNonTargetConstituent);
    }
    Ok(())
}

fn validate_input_particle_size(
    definition: ConstituentSeparationPhysics,
    trace: &ConsumedMaterialTrace,
) -> Result<ParticleSizeDistribution, ConstituentSeparationBatchError> {
    let profile = trace.profile();
    let particle_size = profile
        .particle_size_distribution()
        .cloned()
        .unwrap_or_else(|| {
            unreachable!("authored constituent-separation input form requires particulate state")
        });
    if let Some(required) = definition.input_particle_size_range() {
        let found = particle_size.envelope();
        if found.minimum_diameter() < required.minimum_diameter()
            || found.maximum_diameter() > required.maximum_diameter()
        {
            return Err(
                ConstituentSeparationBatchError::InputParticleSizeOutsideOperatingRange {
                    required,
                    found,
                },
            );
        }
    }
    Ok(particle_size)
}

fn validate_input_trace(
    materials: &MaterialRegistry,
    definition: ConstituentSeparationPhysics,
    trace: &ConsumedMaterialTrace,
) -> Result<SeparationInputKey, ConstituentSeparationBatchError> {
    validate_input_identity(definition, trace)?;
    validate_input_constituents(materials, definition, trace)?;
    let particle_size = validate_input_particle_size(definition, trace)?;
    Ok((trace.profile().temperature(), particle_size))
}

pub(super) fn collect_inputs(
    materials: &MaterialRegistry,
    definition: ConstituentSeparationPhysics,
    traces: &[ConsumedMaterialTrace],
) -> Result<CollectedInputs, ConstituentSeparationBatchError> {
    if traces.is_empty() {
        return Err(ConstituentSeparationBatchError::EmptyInput);
    }

    let mut selected_mass = Mass::ZERO;
    let mut grouped = BTreeMap::<SeparationInputKey, ExactInputProfile>::new();
    for trace in traces {
        let key = validate_input_trace(materials, definition, trace)?;
        selected_mass = selected_mass
            .checked_add(trace.mass())
            .ok_or(ConstituentSeparationBatchError::MassOverflow)?;
        grouped
            .entry(key)
            .or_insert_with(ExactInputProfile::new)
            .add_trace(trace)?;
    }
    Ok(CollectedInputs {
        selected_mass,
        grouped,
    })
}
