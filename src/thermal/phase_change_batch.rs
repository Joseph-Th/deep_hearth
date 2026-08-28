//! Shared pure-material phase-change batch physics for melting and solidification.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Mass, Temperature};
use crate::inventory::ConsumedMaterialTrace;
use crate::material::{
    CommodityKey, FormId, MaterialComposition, MaterialId, MaterialLotSpec, MaterialLotSpecError,
    MaterialPhase, MaterialRegistry,
};

use super::{
    FusionHeatError, PhaseSensibleHeatError, SensibleHeatError, calculate_fusion_heat,
    calculate_sensible_heat,
};

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
    SolidCooling {
        material: MaterialId,
        error: PhaseSensibleHeatError,
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
            Self::SolidCooling { material, error } => write!(
                formatter,
                "solid material {} cannot reach its authored casting output temperature: {error}",
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
            Self::SolidCooling { error, .. } => Some(error),
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
    pub(super) transfer_energy: Energy,
    pub(super) output: MaterialLotSpec,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PhaseChangeTracePhysics {
    melting_point: Temperature,
    transfer_energy: Energy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PhaseChangeBatchAccumulator {
    material: Option<MaterialId>,
    melting_point: Option<Temperature>,
    hottest_input: Temperature,
    total_mass: Mass,
    transfer_energy: Energy,
}

impl PhaseChangeBatchAccumulator {
    fn new() -> Self {
        Self {
            material: None,
            melting_point: None,
            hottest_input: Temperature::ZERO,
            total_mass: Mass::ZERO,
            transfer_energy: Energy::ZERO,
        }
    }

    fn accept_material(&mut self, material: MaterialId) -> Result<(), PurePhaseChangeBatchError> {
        if let Some(expected) = self.material {
            if expected != material {
                return Err(PurePhaseChangeBatchError::MixedMaterials {
                    expected,
                    found: material,
                });
            }
        } else {
            self.material = Some(material);
        }
        Ok(())
    }

    fn add_trace(
        &mut self,
        trace: &ConsumedMaterialTrace,
        physics: PhaseChangeTracePhysics,
    ) -> Result<(), PurePhaseChangeBatchError> {
        if let Some(expected) = self.melting_point {
            debug_assert_eq!(expected, physics.melting_point);
        } else {
            self.melting_point = Some(physics.melting_point);
        }
        self.hottest_input = self.hottest_input.max(trace.profile().temperature());
        self.transfer_energy = self
            .transfer_energy
            .checked_add(physics.transfer_energy)
            .ok_or(PurePhaseChangeBatchError::EnergyOverflow)?;
        self.total_mass = self
            .total_mass
            .checked_add(trace.mass())
            .ok_or(PurePhaseChangeBatchError::MassOverflow)?;
        Ok(())
    }

    fn finish(
        self,
        output_form: FormId,
    ) -> Result<PurePhaseChangeBatch, PurePhaseChangeBatchError> {
        let Some(material) = self.material else {
            return Err(PurePhaseChangeBatchError::EmptyInput);
        };
        let Some(melting_point) = self.melting_point else {
            return Err(PurePhaseChangeBatchError::EmptyInput);
        };
        let output = MaterialLotSpec::with_composition(
            CommodityKey::new(material, output_form),
            self.total_mass,
            melting_point,
            MaterialComposition::pure(material),
        )
        .map_err(PurePhaseChangeBatchError::Output)?;
        Ok(PurePhaseChangeBatch {
            material,
            melting_point,
            hottest_input: self.hottest_input,
            transfer_energy: self.transfer_energy,
            output,
        })
    }
}

fn resolve_phase_change_trace_material(
    materials: &MaterialRegistry,
    input_form: FormId,
    direction: PurePhaseChangeDirection,
    trace: &ConsumedMaterialTrace,
) -> Result<MaterialId, PurePhaseChangeBatchError> {
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
    Ok(material)
}

fn resolve_phase_change_trace_physics(
    materials: &MaterialRegistry,
    direction: PurePhaseChangeDirection,
    trace: &ConsumedMaterialTrace,
    material: MaterialId,
) -> Result<PhaseChangeTracePhysics, PurePhaseChangeBatchError> {
    let profile = trace.profile();
    let fusion = calculate_fusion_heat(materials, trace.mass(), material)
        .map_err(|error| PurePhaseChangeBatchError::FusionHeat { material, error })?;
    let melting_point = fusion.melting_point();
    if !direction.input_temperature_is_valid(profile.temperature(), melting_point) {
        return Err(
            PurePhaseChangeBatchError::InputTemperatureOutsidePhaseRange {
                material,
                phase: direction.input_phase(),
                current: profile.temperature(),
                melting_point,
            },
        );
    }
    let sensible = calculate_sensible_heat(
        materials,
        trace.mass(),
        profile.composition(),
        profile.temperature(),
        melting_point,
    )
    .map_err(|error| PurePhaseChangeBatchError::SensibleHeat { material, error })?;
    let transfer_energy = sensible
        .energy()
        .checked_add(fusion.energy())
        .ok_or(PurePhaseChangeBatchError::EnergyOverflow)?;
    Ok(PhaseChangeTracePhysics {
        melting_point,
        transfer_energy,
    })
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
    let mut batch = PhaseChangeBatchAccumulator::new();
    for trace in traces {
        let material =
            resolve_phase_change_trace_material(materials, input_form, direction, trace)?;
        batch.accept_material(material)?;
        let physics = resolve_phase_change_trace_physics(materials, direction, trace, material)?;
        batch.add_trace(trace, physics)?;
    }
    batch.finish(output_form)
}
