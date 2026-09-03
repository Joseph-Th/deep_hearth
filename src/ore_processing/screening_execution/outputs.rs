//! Pure particle-size projection for screening inputs.

use std::collections::BTreeMap;

use crate::core::arithmetic::greatest_common_divisor_u64;
use crate::core::quantity::{Mass, Temperature};
use crate::inventory::ConsumedMaterialTrace;
use crate::material::{
    CommodityKey, MaterialComposition, MaterialLotSpec, ParticleSizeClass, ParticleSizeDistribution,
};
use crate::production::ProcessOutputStream;

use crate::ore_processing::ScreeningProcessDefinition;

use super::errors::ScreeningBatchError;

pub(super) struct ScreeningOutputs {
    pub(super) streams: Vec<ProcessOutputStream>,
    pub(super) undersize_mass: Mass,
    pub(super) oversize_mass: Mass,
}

type ScreeningGroupKey = (
    CommodityKey,
    Temperature,
    MaterialComposition,
    ParticleSizeDistribution,
);

struct ClassifiedParticleSizes {
    undersize: Vec<ParticleSizeClass>,
    oversize: Vec<ParticleSizeClass>,
    undersize_weight: u64,
}

#[derive(Clone)]
struct ScreeningOutputProfile {
    commodity: CommodityKey,
    temperature: Temperature,
    composition: MaterialComposition,
}

struct ScreeningOutputAccumulator {
    undersize: Vec<MaterialLotSpec>,
    oversize: Vec<MaterialLotSpec>,
    undersize_mass: Mass,
    oversize_mass: Mass,
}

impl ScreeningOutputAccumulator {
    fn new() -> Self {
        Self {
            undersize: Vec::new(),
            oversize: Vec::new(),
            undersize_mass: Mass::ZERO,
            oversize_mass: Mass::ZERO,
        }
    }

    fn add_group(
        &mut self,
        definition: ScreeningProcessDefinition,
        key: ScreeningGroupKey,
        mass: Mass,
    ) -> Result<(), ScreeningBatchError> {
        let (commodity, temperature, composition, distribution) = key;
        let profile = ScreeningOutputProfile {
            commodity,
            temperature,
            composition,
        };
        let classified = classify_particle_sizes(definition, &distribution)?;
        let (group_undersize, group_oversize) = split_group_mass(
            mass,
            classified.undersize_weight,
            distribution.total_weight(),
        )?;

        if !group_undersize.is_zero() {
            self.undersize.push(build_output_lot(
                profile.clone(),
                group_undersize,
                classified.undersize,
            )?);
            self.undersize_mass = self
                .undersize_mass
                .checked_add(group_undersize)
                .ok_or(ScreeningBatchError::MassOverflow)?;
        }
        if !group_oversize.is_zero() {
            self.oversize.push(build_output_lot(
                profile,
                group_oversize,
                classified.oversize,
            )?);
            self.oversize_mass = self
                .oversize_mass
                .checked_add(group_oversize)
                .ok_or(ScreeningBatchError::MassOverflow)?;
        }
        Ok(())
    }

    fn finish(mut self) -> ScreeningOutputs {
        self.undersize.sort();
        self.oversize.sort();
        let mut streams = Vec::with_capacity(2);
        if !self.undersize.is_empty() {
            streams.push(ProcessOutputStream::new(
                ScreeningProcessDefinition::UNDERSIZE_STREAM,
                self.undersize,
            ));
        }
        if !self.oversize.is_empty() {
            streams.push(ProcessOutputStream::new(
                ScreeningProcessDefinition::OVERSIZE_STREAM,
                self.oversize,
            ));
        }
        ScreeningOutputs {
            streams,
            undersize_mass: self.undersize_mass,
            oversize_mass: self.oversize_mass,
        }
    }
}

fn group_screening_inputs(
    definition: ScreeningProcessDefinition,
    traces: &[ConsumedMaterialTrace],
) -> Result<BTreeMap<ScreeningGroupKey, Mass>, ScreeningBatchError> {
    let mut grouped = BTreeMap::new();
    for trace in traces {
        let profile = trace.profile();
        let input_form = profile.commodity().form();
        if input_form != definition.input_form() {
            return Err(ScreeningBatchError::InputFormMismatch {
                expected: definition.input_form(),
                found: input_form,
            });
        }
        let distribution = profile
            .particle_size_distribution()
            .cloned()
            .ok_or(ScreeningBatchError::MissingParticleSize)?;
        let commodity = CommodityKey::new(profile.commodity().material(), definition.output_form());
        let key = (
            commodity,
            profile.temperature(),
            profile.composition().clone(),
            distribution,
        );
        let current = grouped.get(&key).copied().unwrap_or(Mass::ZERO);
        grouped.insert(
            key,
            current
                .checked_add(trace.mass())
                .ok_or(ScreeningBatchError::MassOverflow)?,
        );
    }
    Ok(grouped)
}

fn classify_particle_sizes(
    definition: ScreeningProcessDefinition,
    distribution: &ParticleSizeDistribution,
) -> Result<ClassifiedParticleSizes, ScreeningBatchError> {
    let mut undersize = Vec::new();
    let mut oversize = Vec::new();
    let mut undersize_weight = 0_u64;
    for class in distribution.classes() {
        let range = class.range();
        if range.maximum_diameter() <= definition.aperture() {
            undersize_weight = undersize_weight
                .checked_add(u64::from(class.weight()))
                .ok_or(ScreeningBatchError::MassOverflow)?;
            undersize.push(*class);
        } else if range.minimum_diameter() > definition.aperture() {
            oversize.push(*class);
        } else {
            return Err(ScreeningBatchError::UnresolvedParticleClass {
                aperture: definition.aperture(),
                class: range,
            });
        }
    }
    Ok(ClassifiedParticleSizes {
        undersize,
        oversize,
        undersize_weight,
    })
}

fn split_group_mass(
    mass: Mass,
    undersize_weight: u64,
    total_weight: u64,
) -> Result<(Mass, Mass), ScreeningBatchError> {
    let weighted_mass = u128::from(mass.milligrams()) * u128::from(undersize_weight);
    let total_weight_u128 = u128::from(total_weight);
    if weighted_mass % total_weight_u128 != 0 {
        return Err(ScreeningBatchError::UnrepresentableClassMass {
            mass,
            undersize_weight,
            total_weight,
        });
    }
    let undersize_milligrams = u64::try_from(weighted_mass / total_weight_u128)
        .map_err(|_| ScreeningBatchError::MassOverflow)?;
    let undersize = Mass::from_milligrams(undersize_milligrams);
    let oversize = mass
        .checked_sub(undersize)
        .ok_or(ScreeningBatchError::MassOverflow)?;
    Ok((undersize, oversize))
}

pub(super) fn representable_screening_mass_floor(
    definition: ScreeningProcessDefinition,
    distribution: &ParticleSizeDistribution,
    requested: Mass,
) -> Result<Mass, ScreeningBatchError> {
    if requested.is_zero() {
        return Ok(Mass::ZERO);
    }
    let classified = classify_particle_sizes(definition, distribution)?;
    let total_weight = distribution.total_weight();
    if definition.input_form() == definition.output_form()
        && (classified.undersize_weight == 0 || classified.undersize_weight == total_weight)
    {
        return Err(ScreeningBatchError::NoParticleSizePartition {
            aperture: definition.aperture(),
        });
    }
    let quantum =
        total_weight / greatest_common_divisor_u64(classified.undersize_weight, total_weight);
    let representable = requested.milligrams() - requested.milligrams() % quantum;
    Ok(Mass::from_milligrams(representable))
}

fn build_output_lot(
    profile: ScreeningOutputProfile,
    mass: Mass,
    classes: Vec<ParticleSizeClass>,
) -> Result<MaterialLotSpec, ScreeningBatchError> {
    let distribution =
        ParticleSizeDistribution::new(classes).map_err(ScreeningBatchError::Distribution)?;
    MaterialLotSpec::with_composition_and_particle_size(
        profile.commodity,
        mass,
        profile.temperature,
        profile.composition,
        distribution,
    )
    .map_err(ScreeningBatchError::Output)
}

pub(super) fn resolve_screening_outputs(
    definition: ScreeningProcessDefinition,
    traces: &[ConsumedMaterialTrace],
) -> Result<ScreeningOutputs, ScreeningBatchError> {
    if traces.is_empty() {
        return Err(ScreeningBatchError::EmptyInput);
    }
    let grouped = group_screening_inputs(definition, traces)?;
    let mut outputs = ScreeningOutputAccumulator::new();
    for (key, mass) in grouped {
        outputs.add_group(definition, key, mass)?;
    }
    let outputs = outputs.finish();
    if definition.input_form() == definition.output_form()
        && (outputs.undersize_mass.is_zero() || outputs.oversize_mass.is_zero())
    {
        return Err(ScreeningBatchError::NoParticleSizePartition {
            aperture: definition.aperture(),
        });
    }
    Ok(outputs)
}
