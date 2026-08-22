//! Admission of finite world-generated geological matter into authoritative state.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::material::{
    FormId, MaterialId, MaterialPhase, MaterialPhaseStateError, ParticleSizeStatePolicy,
    validate_material_phase_state,
};
use crate::registry::Registries;

use super::state::{
    GeneratedDepositSpec, GeologicalDepositId, GeologicalDepositLifecycle, GeologicalDepositRecord,
};

/// Failure while admitting a finite world-generated geological deposit into authoritative state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InsertGeneratedDepositError {
    UnknownMaterial { material: MaterialId },
    UnknownForm { form: FormId },
    UnsupportedPhase { form: FormId, phase: MaterialPhase },
    UnsupportedParticulateForm { form: FormId },
    InvalidPhaseState(MaterialPhaseStateError),
    UnknownCompositionMaterial { material: MaterialId },
    IdExhausted,
    RevisionExhausted,
}

impl Display for InsertGeneratedDepositError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownMaterial { material } => write!(
                formatter,
                "generated geological deposit references unknown material {}",
                material.value()
            ),
            Self::UnknownForm { form } => write!(
                formatter,
                "generated geological deposit references unknown form {}",
                form.value()
            ),
            Self::UnsupportedPhase { form, phase } => write!(
                formatter,
                "generated geological deposit form {} is {phase:?}; finite geological deposits must be solid",
                form.value()
            ),
            Self::UnsupportedParticulateForm { form } => write!(
                formatter,
                "generated geological deposit form {} requires processed particle-size state; natural geological deposits cannot own it",
                form.value()
            ),
            Self::InvalidPhaseState(error) => write!(
                formatter,
                "generated geological deposit has invalid material phase state: {error}"
            ),
            Self::UnknownCompositionMaterial { material } => write!(
                formatter,
                "generated geological deposit composition references unknown material {}",
                material.value()
            ),
            Self::IdExhausted => {
                formatter.write_str("geological deposit identifier space is exhausted")
            }
            Self::RevisionExhausted => formatter.write_str("geology revision space is exhausted"),
        }
    }
}

impl Error for InsertGeneratedDepositError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidPhaseState(error) => Some(error),
            Self::UnknownMaterial { material: _ }
            | Self::UnknownForm { form: _ }
            | Self::UnsupportedPhase { form: _, phase: _ }
            | Self::UnsupportedParticulateForm { form: _ }
            | Self::UnknownCompositionMaterial { material: _ }
            | Self::IdExhausted
            | Self::RevisionExhausted => None,
        }
    }
}

/// Inserts matter supplied by a world-generation owner, preserving its physical profile exactly.
///
/// This is not a player mining operation. It establishes finite geological matter that the mining
/// subsystem may later reserve and excavate through its tool/labor-gated transaction.
pub fn insert_generated_deposit(
    registries: &Registries,
    state: &mut AppState,
    spec: GeneratedDepositSpec,
) -> Result<GeologicalDepositId, InsertGeneratedDepositError> {
    if registries
        .materials()
        .get_material(spec.commodity().material())
        .is_none()
    {
        return Err(InsertGeneratedDepositError::UnknownMaterial {
            material: spec.commodity().material(),
        });
    }
    let Some(form) = registries.materials().get_form(spec.commodity().form()) else {
        return Err(InsertGeneratedDepositError::UnknownForm {
            form: spec.commodity().form(),
        });
    };
    if form.phase() != MaterialPhase::Solid {
        return Err(InsertGeneratedDepositError::UnsupportedPhase {
            form: spec.commodity().form(),
            phase: form.phase(),
        });
    }
    if form.particle_size_policy() == ParticleSizeStatePolicy::Required {
        return Err(InsertGeneratedDepositError::UnsupportedParticulateForm {
            form: spec.commodity().form(),
        });
    }
    for component in spec.composition().components() {
        if registries
            .materials()
            .get_material(component.material())
            .is_none()
        {
            return Err(InsertGeneratedDepositError::UnknownCompositionMaterial {
                material: component.material(),
            });
        }
    }
    validate_material_phase_state(
        registries.materials(),
        spec.commodity(),
        spec.composition(),
        spec.temperature(),
    )
    .map_err(InsertGeneratedDepositError::InvalidPhaseState)?;

    let geology = state.geology();
    let id = GeologicalDepositId::new(geology.next_deposit_id());
    let Some(next_id) = geology.next_deposit_id().checked_add(1) else {
        return Err(InsertGeneratedDepositError::IdExhausted);
    };
    let Some(next_revision) = geology.revision().checked_add(1) else {
        return Err(InsertGeneratedDepositError::RevisionExhausted);
    };
    let generated_at = state.tick();
    let record = GeologicalDepositRecord {
        id,
        bounds: spec.bounds(),
        commodity: spec.commodity(),
        initial_mass: spec.mass(),
        remaining_mass: spec.mass(),
        temperature: spec.temperature(),
        excavation_hardness: spec.excavation_hardness(),
        composition: spec.composition().clone(),
        lifecycle: GeologicalDepositLifecycle::Available,
        generated_at,
    };

    state
        .geology_state_mut()
        .insert_deposit(record, next_id, next_revision);
    Ok(id)
}

#[cfg(test)]
#[path = "generation_execution_tests.rs"]
mod tests;
