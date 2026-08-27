//! Shared pure-material phase-change batch physics for melting and solidification.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Mass, Temperature};
use crate::inventory::ConsumedMaterialTrace;
use crate::material::{
    CommodityKey, FormId, MaterialComposition, MaterialId, MaterialLotSpec, MaterialLotSpecError,
    MaterialPhase, MaterialRegistry,
};

use super::{FusionHeatError, SensibleHeatError, calculate_fusion_heat, calculate_sensible_heat};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PurePhaseChangeDirection {
    Melt,
    Solidify,
}

impl PurePhaseChangeDirection {
    const fn input_phase(self) -> MaterialPhase {
        match self {
            Self::Melt => MaterialPhase::Solid,
            Self::Solidify => MaterialPhase::Liquid,
        }
    }

    fn input_temperature_is_valid(self, current: Temperature, melting_point: Temperature) -> bool {
        match self {
            Self::Melt => current <= melting_point,
            Self::Solidify => current >= melting_point,
        }
    }
}

/// Failure while resolving conserved matter and energy for a pure-material phase transition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PurePhaseChangeBatchError {
    EmptyInput,
    UnknownInputForm {
        form: FormId,
    },
    InputPhaseMismatch {
        form: FormId,
        expected: MaterialPhase,
        found: MaterialPhase,
    },
    InputFormMismatch {
        expected: FormId,
        found: FormId,
    },
    ImpureInput {
        commodity: CommodityKey,
    },
    PureMaterialDoesNotMatchCommodity {
        commodity: CommodityKey,
        pure: MaterialId,
    },
    MixedMaterials {
        expected: MaterialId,
        found: MaterialId,
    },
    InputTemperatureOutsidePhaseRange {
        material: MaterialId,
        phase: MaterialPhase,
        current: Temperature,
        melting_point: Temperature,
    },
    SensibleHeat {
        material: MaterialId,
        error: SensibleHeatError,
    },
    FusionHeat {
        material: MaterialId,
        error: FusionHeatError,
    },
    EnergyOverflow,
    MassOverflow,
    Output(MaterialLotSpecError),
}

impl Display for PurePhaseChangeBatchError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyInput => formatter.write_str("phase-change batch contains no material"),
            Self::UnknownInputForm { form } => write!(
                formatter,
                "phase-change batch references unknown form {}",
                form.value()
            ),
            Self::InputPhaseMismatch {
                form,
                expected,
                found,
            } => write!(
                formatter,
                "phase-change input form {} is {found:?} rather than required {expected:?}",
                form.value(),
            ),
            Self::InputFormMismatch { expected, found } => write!(
                formatter,
                "phase-change process requires input form {} but selected form {} was provided",
                expected.value(),
                found.value()
            ),
            Self::ImpureInput { commodity } => write!(
                formatter,
                "phase-change input material {} in form {} is compositionally mixed; alloy phase diagrams are not modeled",
                commodity.material().value(),
                commodity.form().value()
            ),
            Self::PureMaterialDoesNotMatchCommodity { commodity, pure } => write!(
                formatter,
                "phase-change input material {} in form {} claims pure material {} instead",
                commodity.material().value(),
                commodity.form().value(),
                pure.value()
            ),
            Self::MixedMaterials { expected, found } => write!(
                formatter,
                "phase-change batch mixes material {} with material {}; alloy transitions require a dedicated resolver",
                expected.value(),
                found.value()
            ),
            Self::InputTemperatureOutsidePhaseRange {
                material,
                phase,
                current,
                melting_point,
            } => write!(
                formatter,
                "{phase:?} material {} at {} mK is on the wrong side of its {} mK melting point",
                material.value(),
                current.millikelvin(),
                melting_point.millikelvin()
            ),
            Self::SensibleHeat { material, error } => write!(
                formatter,
                "material {} cannot reach its fusion boundary: {error}",
                material.value()
            ),
            Self::FusionHeat { material, error } => write!(
                formatter,
                "material {} cannot resolve latent heat: {error}",
                material.value()
            ),
            Self::EnergyOverflow => formatter.write_str("phase-change energy overflowed"),
            Self::MassOverflow => formatter.write_str("phase-change batch mass overflowed"),
            Self::Output(error) => write!(
                formatter,
                "phase-change output construction failed: {error}"
            ),
        }
    }
}

impl Error for PurePhaseChangeBatchError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::SensibleHeat { error, .. } => Some(error),
            Self::FusionHeat { error, .. } => Some(error),
            Self::Output(error) => Some(error),
            Self::EmptyInput
            | Self::UnknownInputForm { .. }
            | Self::InputPhaseMismatch { .. }
            | Self::InputFormMismatch { .. }
            | Self::ImpureInput { .. }
            | Self::PureMaterialDoesNotMatchCommodity { .. }
            | Self::MixedMaterials { .. }
            | Self::InputTemperatureOutsidePhaseRange { .. }
            | Self::EnergyOverflow
            | Self::MassOverflow => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PurePhaseChangeBatch {
    pub(super) material: MaterialId,
    pub(super) melting_point: Temperature,
    pub(super) hottest_input: Temperature,
    pub(super) phase_energy: Energy,
    pub(super) output: MaterialLotSpec,
}

/// Resolves the conserved matter and energy magnitude for one pure-material phase transition.
///
/// Both melting and solidification use the same latent/sensible energy magnitudes. Direction only
/// controls the required input phase and which side of the melting boundary an input may occupy;
/// the caller decides whether the resolved energy is consumed from a source or released to a sink.
pub(super) fn resolve_pure_phase_change_batch(
    materials: &MaterialRegistry,
    input_form: FormId,
    output_form: FormId,
    direction: PurePhaseChangeDirection,
    traces: &[ConsumedMaterialTrace],
) -> Result<PurePhaseChangeBatch, PurePhaseChangeBatchError> {
    let mut batch_material = None;
    let mut melting_point = None;
    let mut hottest_input = Temperature::ZERO;
    let mut total_mass = Mass::ZERO;
    let mut phase_energy = Energy::ZERO;

    for trace in traces {
        let profile = trace.profile();
        let form_id = profile.commodity().form();
        let Some(form) = materials.get_form(form_id) else {
            return Err(PurePhaseChangeBatchError::UnknownInputForm { form: form_id });
        };
        let expected_phase = direction.input_phase();
        if form.phase() != expected_phase {
            return Err(PurePhaseChangeBatchError::InputPhaseMismatch {
                form: form_id,
                expected: expected_phase,
                found: form.phase(),
            });
        }
        if form_id != input_form {
            return Err(PurePhaseChangeBatchError::InputFormMismatch {
                expected: input_form,
                found: form_id,
            });
        }

        let Some(material) = profile.composition().pure_material() else {
            return Err(PurePhaseChangeBatchError::ImpureInput {
                commodity: profile.commodity(),
            });
        };
        if profile.commodity().material() != material {
            return Err(
                PurePhaseChangeBatchError::PureMaterialDoesNotMatchCommodity {
                    commodity: profile.commodity(),
                    pure: material,
                },
            );
        }
        if let Some(expected) = batch_material {
            if expected != material {
                return Err(PurePhaseChangeBatchError::MixedMaterials {
                    expected,
                    found: material,
                });
            }
        } else {
            batch_material = Some(material);
        }

        let fusion = calculate_fusion_heat(materials, trace.mass(), material)
            .map_err(|error| PurePhaseChangeBatchError::FusionHeat { material, error })?;
        if !direction.input_temperature_is_valid(profile.temperature(), fusion.melting_point()) {
            return Err(
                PurePhaseChangeBatchError::InputTemperatureOutsidePhaseRange {
                    material,
                    phase: expected_phase,
                    current: profile.temperature(),
                    melting_point: fusion.melting_point(),
                },
            );
        }
        if let Some(expected) = melting_point {
            debug_assert_eq!(expected, fusion.melting_point());
        } else {
            melting_point = Some(fusion.melting_point());
        }

        hottest_input = hottest_input.max(profile.temperature());
        let sensible = calculate_sensible_heat(
            materials,
            trace.mass(),
            profile.composition(),
            profile.temperature(),
            fusion.melting_point(),
        )
        .map_err(|error| PurePhaseChangeBatchError::SensibleHeat { material, error })?;
        phase_energy = phase_energy
            .checked_add(sensible.energy())
            .and_then(|energy| energy.checked_add(fusion.energy()))
            .ok_or(PurePhaseChangeBatchError::EnergyOverflow)?;
        total_mass = total_mass
            .checked_add(trace.mass())
            .ok_or(PurePhaseChangeBatchError::MassOverflow)?;
    }

    let Some(material) = batch_material else {
        return Err(PurePhaseChangeBatchError::EmptyInput);
    };
    let Some(melting_point) = melting_point else {
        return Err(PurePhaseChangeBatchError::EmptyInput);
    };
    let output = MaterialLotSpec::with_composition(
        CommodityKey::new(material, output_form),
        total_mass,
        melting_point,
        MaterialComposition::pure(material),
    )
    .map_err(PurePhaseChangeBatchError::Output)?;

    Ok(PurePhaseChangeBatch {
        material,
        melting_point,
        hottest_input,
        phase_energy,
        output,
    })
}
