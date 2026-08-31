//! Pure sensible-heating batch physics shared by runtime resolution and persistence replay.

use std::collections::BTreeMap;

use crate::core::quantity::{Energy, Mass, PreciseEnergy, Temperature};
use crate::inventory::ConsumedMaterialTrace;
use crate::material::{MaterialLotSpec, MaterialLotSpecError, MaterialRegistry};

use super::super::physics::calculate_phase_sensible_heat_precise;
use super::super::{HeatDirection, PhaseSensibleHeatError, SensibleHeatError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum SensibleHeatingBatchError {
    TargetBelowInputTemperature {
        current: Temperature,
        target: Temperature,
    },
    Heat(PhaseSensibleHeatError),
    ArithmeticOverflow,
    Output(MaterialLotSpecError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ResolvedSensibleHeatingBatch {
    required_energy: Energy,
    outputs: Vec<MaterialLotSpec>,
}

impl ResolvedSensibleHeatingBatch {
    pub(super) const fn required_energy(&self) -> Energy {
        self.required_energy
    }

    pub(super) fn into_outputs(self) -> Vec<MaterialLotSpec> {
        self.outputs
    }
}

/// Resolves material-specific sensible heat and the exact conserved output snapshot for one batch.
pub(super) fn resolve_sensible_heating_batch(
    materials: &MaterialRegistry,
    inputs: &[ConsumedMaterialTrace],
    target: Temperature,
) -> Result<ResolvedSensibleHeatingBatch, SensibleHeatingBatchError> {
    let mut required_energy = PreciseEnergy::ZERO;
    let mut output_masses = BTreeMap::new();

    for trace in inputs {
        let profile = trace.profile();
        if target < profile.temperature() {
            return Err(SensibleHeatingBatchError::TargetBelowInputTemperature {
                current: profile.temperature(),
                target,
            });
        }
        let (heat, direction) = calculate_phase_sensible_heat_precise(
            materials,
            trace.mass(),
            profile.commodity(),
            profile.composition(),
            profile.temperature(),
            target,
        )
        .map_err(SensibleHeatingBatchError::Heat)?;
        debug_assert!(matches!(
            direction,
            HeatDirection::None | HeatDirection::IntoMaterial
        ));
        required_energy = required_energy
            .checked_add(heat)
            .ok_or(SensibleHeatingBatchError::ArithmeticOverflow)?;

        let key = (
            profile.commodity(),
            profile.composition().clone(),
            profile.particle_size_distribution().cloned(),
        );
        let current = output_masses.get(&key).copied().unwrap_or(Mass::ZERO);
        output_masses.insert(
            key,
            current
                .checked_add(trace.mass())
                .ok_or(SensibleHeatingBatchError::ArithmeticOverflow)?,
        );
    }

    let mut outputs = Vec::with_capacity(output_masses.len());
    for ((commodity, composition, particle_size), mass) in output_masses {
        let output = match particle_size {
            Some(particle_size) => MaterialLotSpec::with_composition_and_particle_size(
                commodity,
                mass,
                target,
                composition,
                particle_size,
            ),
            None => MaterialLotSpec::with_composition(commodity, mass, target, composition),
        }
        .map_err(SensibleHeatingBatchError::Output)?;
        outputs.push(output);
    }

    let femtojoule_remainder = required_energy.femtojoule_remainder();
    let required_energy =
        required_energy
            .whole_nanojoules()
            .ok_or(SensibleHeatingBatchError::Heat(
                PhaseSensibleHeatError::Heat(SensibleHeatError::FractionalNanojoule {
                    femtojoule_remainder,
                }),
            ))?;

    Ok(ResolvedSensibleHeatingBatch {
        required_energy,
        outputs,
    })
}
