//! Pure particle-size projection for screening inputs.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Mass, Temperature};
use crate::inventory::ConsumedMaterialTrace;
use crate::material::{
    CommodityKey, FormId, MaterialComposition, MaterialLotSpec, MaterialLotSpecError,
    ParticleSizeDistribution, ParticleSizeDistributionError, ParticleSizeRange,
};
use crate::production::ProcessOutputStream;

use crate::ore_processing::ScreeningProcessDefinition;

/// Failure while partitioning selected material into exact screen products.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScreeningBatchError {
    EmptyInput,
    InputFormMismatch {
        expected: FormId,
        found: FormId,
    },
    MissingParticleSize,
    UnresolvedParticleClass {
        aperture: crate::core::quantity::Length,
        class: ParticleSizeRange,
    },
    UnrepresentableClassMass {
        mass: Mass,
        undersize_weight: u64,
        total_weight: u64,
    },
    MassOverflow,
    Distribution(ParticleSizeDistributionError),
    Output(MaterialLotSpecError),
}

impl Display for ScreeningBatchError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyInput => formatter.write_str("screening batch contains no material"),
            Self::InputFormMismatch { expected, found } => write!(
                formatter,
                "screening batch requires input form {} but selected form {}",
                expected.value(),
                found.value()
            ),
            Self::MissingParticleSize => {
                formatter.write_str("screening input has no particulate size distribution")
            }
            Self::UnresolvedParticleClass { aperture, class } => write!(
                formatter,
                "screen aperture {} um intersects unresolved particle class {}..={} um",
                aperture.micrometers(),
                class.minimum_diameter().micrometers(),
                class.maximum_diameter().micrometers()
            ),
            Self::UnrepresentableClassMass {
                mass,
                undersize_weight,
                total_weight,
            } => write!(
                formatter,
                "screening {} mg with undersize weight {undersize_weight}/{total_weight} cannot be represented exactly at whole-milligram mass resolution",
                mass.milligrams()
            ),
            Self::MassOverflow => formatter.write_str("screening output mass overflowed"),
            Self::Distribution(error) => write!(
                formatter,
                "screening could not preserve a classified particle distribution: {error}"
            ),
            Self::Output(error) => write!(
                formatter,
                "screening output specification could not preserve its material profile: {error}"
            ),
        }
    }
}

impl Error for ScreeningBatchError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Distribution(error) => Some(error),
            Self::Output(error) => Some(error),
            Self::InputFormMismatch { .. }
            | Self::UnresolvedParticleClass { .. }
            | Self::UnrepresentableClassMass { .. }
            | Self::EmptyInput
            | Self::MissingParticleSize
            | Self::MassOverflow => None,
        }
    }
}

pub(super) struct ScreeningOutputs {
    pub(super) streams: Vec<ProcessOutputStream>,
    pub(super) undersize_mass: Mass,
    pub(super) oversize_mass: Mass,
}

pub(super) fn resolve_screening_outputs(
    definition: ScreeningProcessDefinition,
    traces: &[ConsumedMaterialTrace],
) -> Result<ScreeningOutputs, ScreeningBatchError> {
    if traces.is_empty() {
        return Err(ScreeningBatchError::EmptyInput);
    }

    let mut grouped = BTreeMap::<
        (
            CommodityKey,
            Temperature,
            MaterialComposition,
            ParticleSizeDistribution,
        ),
        Mass,
    >::new();
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

    let mut undersize = Vec::new();
    let mut oversize = Vec::new();
    let mut undersize_mass = Mass::ZERO;
    let mut oversize_mass = Mass::ZERO;
    for ((commodity, temperature, composition, distribution), mass) in grouped {
        let mut undersize_classes = Vec::new();
        let mut oversize_classes = Vec::new();
        let mut undersize_weight = 0_u64;
        for class in distribution.classes() {
            let range = class.range();
            if range.maximum_diameter() <= definition.aperture() {
                undersize_weight = undersize_weight
                    .checked_add(u64::from(class.weight()))
                    .ok_or(ScreeningBatchError::MassOverflow)?;
                undersize_classes.push(*class);
            } else if range.minimum_diameter() > definition.aperture() {
                oversize_classes.push(*class);
            } else {
                return Err(ScreeningBatchError::UnresolvedParticleClass {
                    aperture: definition.aperture(),
                    class: range,
                });
            }
        }

        let total_weight = distribution.total_weight();
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
        let group_undersize = Mass::from_milligrams(undersize_milligrams);
        let group_oversize = mass
            .checked_sub(group_undersize)
            .ok_or(ScreeningBatchError::MassOverflow)?;

        if !group_undersize.is_zero() {
            let distribution = ParticleSizeDistribution::new(undersize_classes)
                .map_err(ScreeningBatchError::Distribution)?;
            undersize.push(
                MaterialLotSpec::with_composition_and_particle_size(
                    commodity,
                    group_undersize,
                    temperature,
                    composition.clone(),
                    distribution,
                )
                .map_err(ScreeningBatchError::Output)?,
            );
            undersize_mass = undersize_mass
                .checked_add(group_undersize)
                .ok_or(ScreeningBatchError::MassOverflow)?;
        }
        if !group_oversize.is_zero() {
            let distribution = ParticleSizeDistribution::new(oversize_classes)
                .map_err(ScreeningBatchError::Distribution)?;
            oversize.push(
                MaterialLotSpec::with_composition_and_particle_size(
                    commodity,
                    group_oversize,
                    temperature,
                    composition,
                    distribution,
                )
                .map_err(ScreeningBatchError::Output)?,
            );
            oversize_mass = oversize_mass
                .checked_add(group_oversize)
                .ok_or(ScreeningBatchError::MassOverflow)?;
        }
    }

    undersize.sort();
    oversize.sort();
    let mut streams = Vec::with_capacity(2);
    if !undersize.is_empty() {
        streams.push(ProcessOutputStream::new(
            ScreeningProcessDefinition::UNDERSIZE_STREAM,
            undersize,
        ));
    }
    if !oversize.is_empty() {
        streams.push(ProcessOutputStream::new(
            ScreeningProcessDefinition::OVERSIZE_STREAM,
            oversize,
        ));
    }
    Ok(ScreeningOutputs {
        streams,
        undersize_mass,
        oversize_mass,
    })
}
