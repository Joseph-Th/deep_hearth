//! Admission of finite world-generated geological matter into authoritative state.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::material::{CommodityKey, FormId, MaterialId, MaterialPhase, MaterialPhaseStateError};
use crate::registry::Registries;

use super::material_validation::{
    GeologicalMaterialStateError, validate_geological_material_state,
};
use super::state::{
    GeneratedDepositSpec, GeologicalDepositId, GeologicalDepositLifecycle, GeologicalDepositRecord,
};

/// Failure while admitting a finite world-generated geological deposit into authoritative state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InsertGeneratedDepositError {
    UnknownMaterial { material: MaterialId },
    UnknownForm { form: FormId },
    UnsupportedCommodity { commodity: CommodityKey },
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
            Self::UnsupportedCommodity { commodity } => write!(
                formatter,
                "generated geological deposit uses unauthored material {} form {}",
                commodity.material().value(),
                commodity.form().value()
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
            | Self::UnsupportedCommodity { commodity: _ }
            | Self::UnsupportedPhase { form: _, phase: _ }
            | Self::UnsupportedParticulateForm { form: _ }
            | Self::UnknownCompositionMaterial { material: _ }
            | Self::IdExhausted
            | Self::RevisionExhausted => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GenerateGeologicalDepositCommitError {
    Tick {
        expected: SimulationTick,
        actual: SimulationTick,
    },
    Revision {
        expected: u64,
        actual: u64,
    },
    NextDepositId {
        expected: u32,
        actual: u32,
    },
}

/// Prepared generation of actual geological matter. Validation cannot allocate
/// ownership early; commit prechecks identity and revision before any mutation.
#[must_use]
pub(crate) struct ValidatedGeologicalDepositGeneration {
    record: GeologicalDepositRecord,
    expected_tick: SimulationTick,
    expected_revision: u64,
    next_id: u32,
    next_revision: u64,
}

impl ValidatedGeologicalDepositGeneration {
    pub(crate) fn precheck(
        &self,
        state: &AppState,
    ) -> Result<(), GenerateGeologicalDepositCommitError> {
        if state.tick() != self.expected_tick {
            return Err(GenerateGeologicalDepositCommitError::Tick {
                expected: self.expected_tick,
                actual: state.tick(),
            });
        }
        if state.geology().revision() != self.expected_revision {
            return Err(GenerateGeologicalDepositCommitError::Revision {
                expected: self.expected_revision,
                actual: state.geology().revision(),
            });
        }
        if state.geology().next_deposit_id() != self.record.id.value() {
            return Err(GenerateGeologicalDepositCommitError::NextDepositId {
                expected: self.record.id.value(),
                actual: state.geology().next_deposit_id(),
            });
        }
        Ok(())
    }

    pub(crate) fn apply(self, state: &mut AppState) -> GeologicalDepositId {
        let id = self.record.id;
        state
            .geology_state_mut()
            .insert_deposit(self.record, self.next_id, self.next_revision);
        id
    }
}

/// Validates finite geological truth without pre-mutating its canonical owner.
/// World generation composes this plan atomically with surface and water owners.
pub(crate) fn validate_generate_geological_deposit(
    registries: &Registries,
    state: &AppState,
    spec: GeneratedDepositSpec,
) -> Result<ValidatedGeologicalDepositGeneration, InsertGeneratedDepositError> {
    validate_geological_material_state(
        registries.materials(),
        spec.commodity(),
        spec.composition(),
        spec.temperature(),
    )
    .map_err(|error| match error {
        GeologicalMaterialStateError::UnknownCommodityMaterial { material } => {
            InsertGeneratedDepositError::UnknownMaterial { material }
        }
        GeologicalMaterialStateError::UnknownCommodityForm { form } => {
            InsertGeneratedDepositError::UnknownForm { form }
        }
        GeologicalMaterialStateError::UnsupportedCommodity { commodity } => {
            InsertGeneratedDepositError::UnsupportedCommodity { commodity }
        }
        GeologicalMaterialStateError::UnsupportedCommodityPhase { form, phase } => {
            InsertGeneratedDepositError::UnsupportedPhase { form, phase }
        }
        GeologicalMaterialStateError::UnsupportedCommodityParticulateForm { form } => {
            InsertGeneratedDepositError::UnsupportedParticulateForm { form }
        }
        GeologicalMaterialStateError::UnknownCompositionMaterial { material } => {
            InsertGeneratedDepositError::UnknownCompositionMaterial { material }
        }
        GeologicalMaterialStateError::InvalidPhaseState(error) => {
            InsertGeneratedDepositError::InvalidPhaseState(error)
        }
    })?;

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
        depleted_at: None,
    };

    Ok(ValidatedGeologicalDepositGeneration {
        record,
        expected_tick: generated_at,
        expected_revision: geology.revision(),
        next_id,
        next_revision,
    })
}

/// Legacy fixture entry point shares exactly the production generation owner.
#[cfg(any(test, feature = "test-gameplay"))]
pub(crate) fn insert_generated_deposit(
    registries: &Registries,
    state: &mut AppState,
    spec: GeneratedDepositSpec,
) -> Result<GeologicalDepositId, InsertGeneratedDepositError> {
    let plan = validate_generate_geological_deposit(registries, state, spec)?;
    Ok(plan.apply(state))
}

#[cfg(test)]
#[path = "generation_execution_tests.rs"]
mod tests;
