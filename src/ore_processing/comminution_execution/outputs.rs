//! Pure material and particle-size projection for comminution inputs.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Mass, Temperature};
use crate::inventory::ConsumedMaterialTrace;
use crate::material::{
    CommodityKey, FormId, MaterialComposition, MaterialLotSpec, MaterialLotSpecError,
    ParticleSizeRange,
};
use crate::ore_processing::ComminutionProcessDefinition;

/// Failure while mapping exact selected material traces to comminuted output specifications.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComminutionBatchError {
    EmptyInput,
    InputFormMismatch {
        expected: FormId,
        found: FormId,
    },
    MissingInputParticleSize {
        required: ParticleSizeRange,
    },
    InputParticleSizeOutsideOperatingRange {
        required: ParticleSizeRange,
        found: ParticleSizeRange,
    },
    ParticleSizeNotReduced {
        input: ParticleSizeRange,
        output: ParticleSizeRange,
    },
    MassOverflow,
    Output(MaterialLotSpecError),
}

impl Display for ComminutionBatchError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyInput => formatter.write_str("comminution batch contains no material"),
            Self::InputFormMismatch { expected, found } => write!(
                formatter,
                "comminution batch requires input form {} but selected form {}",
                expected.value(),
                found.value()
            ),
            Self::MissingInputParticleSize { required } => write!(
                formatter,
                "comminution feed must resolve particle sizes inside {}..={} um",
                required.minimum_diameter().micrometers(),
                required.maximum_diameter().micrometers()
            ),
            Self::InputParticleSizeOutsideOperatingRange { required, found } => write!(
                formatter,
                "comminution feed {}..={} um lies outside authored operating range {}..={} um",
                found.minimum_diameter().micrometers(),
                found.maximum_diameter().micrometers(),
                required.minimum_diameter().micrometers(),
                required.maximum_diameter().micrometers()
            ),
            Self::ParticleSizeNotReduced { input, output } => write!(
                formatter,
                "comminution output {}..={} um does not strictly reduce input {}..={} um without coarsening fines",
                output.minimum_diameter().micrometers(),
                output.maximum_diameter().micrometers(),
                input.minimum_diameter().micrometers(),
                input.maximum_diameter().micrometers()
            ),
            Self::MassOverflow => formatter.write_str("comminution output mass overflowed"),
            Self::Output(error) => write!(
                formatter,
                "comminution output specification could not preserve its material profile: {error}"
            ),
        }
    }
}

impl Error for ComminutionBatchError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Output(error) => Some(error),
            Self::InputFormMismatch { .. }
            | Self::MissingInputParticleSize { .. }
            | Self::InputParticleSizeOutsideOperatingRange { .. }
            | Self::ParticleSizeNotReduced { .. }
            | Self::EmptyInput
            | Self::MassOverflow => None,
        }
    }
}

type ComminutionOutputKey = (CommodityKey, Temperature, MaterialComposition);

fn validate_input_particle_size(
    definition: &ComminutionProcessDefinition,
    trace: &ConsumedMaterialTrace,
) -> Result<(), ComminutionBatchError> {
    let profile = trace.profile();
    if let Some(required) = definition.input_particle_size_range() {
        let found = profile
            .particle_size()
            .ok_or(ComminutionBatchError::MissingInputParticleSize { required })?;
        if found.minimum_diameter() < required.minimum_diameter()
            || found.maximum_diameter() > required.maximum_diameter()
        {
            return Err(
                ComminutionBatchError::InputParticleSizeOutsideOperatingRange { required, found },
            );
        }
    }
    if let Some(input) = profile.particle_size() {
        let output = definition.output_particle_size();
        if output.minimum_diameter() > input.minimum_diameter()
            || output.maximum_diameter() >= input.maximum_diameter()
        {
            return Err(ComminutionBatchError::ParticleSizeNotReduced { input, output });
        }
    }
    Ok(())
}

fn group_comminution_inputs(
    definition: &ComminutionProcessDefinition,
    traces: &[ConsumedMaterialTrace],
) -> Result<BTreeMap<ComminutionOutputKey, Mass>, ComminutionBatchError> {
    let mut grouped = BTreeMap::new();
    for trace in traces {
        let profile = trace.profile();
        let input_form = profile.commodity().form();
        if input_form != definition.input_form() {
            return Err(ComminutionBatchError::InputFormMismatch {
                expected: definition.input_form(),
                found: input_form,
            });
        }
        validate_input_particle_size(definition, trace)?;
        let commodity = CommodityKey::new(profile.commodity().material(), definition.output_form());
        let key = (
            commodity,
            profile.temperature(),
            profile.composition().clone(),
        );
        let current = grouped.get(&key).copied().unwrap_or(Mass::ZERO);
        let next = current
            .checked_add(trace.mass())
            .ok_or(ComminutionBatchError::MassOverflow)?;
        grouped.insert(key, next);
    }
    Ok(grouped)
}

fn build_comminution_outputs(
    definition: &ComminutionProcessDefinition,
    grouped: BTreeMap<ComminutionOutputKey, Mass>,
) -> Result<Vec<MaterialLotSpec>, ComminutionBatchError> {
    let mut outputs = grouped
        .into_iter()
        .map(|((commodity, temperature, composition), mass)| {
            MaterialLotSpec::with_composition_and_particle_size(
                commodity,
                mass,
                temperature,
                composition,
                definition.output_particle_size_distribution().clone(),
            )
            .map_err(ComminutionBatchError::Output)
        })
        .collect::<Result<Vec<_>, _>>()?;
    outputs.sort();
    Ok(outputs)
}

pub(super) fn resolve_comminution_outputs(
    definition: &ComminutionProcessDefinition,
    traces: &[ConsumedMaterialTrace],
) -> Result<Vec<MaterialLotSpec>, ComminutionBatchError> {
    if traces.is_empty() {
        return Err(ComminutionBatchError::EmptyInput);
    }
    build_comminution_outputs(definition, group_comminution_inputs(definition, traces)?)
}
