//! Shared direct-labor ore-processing throughput and persisted-job physics.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::throughput::{MassFlowDurationError, calculate_mass_flow_duration_ceiling};
use crate::core::time::{PhysicalTickDuration, TickSpan};

use super::ManualOreProcessProfile;

mod equipment;
mod job_validation;

pub use equipment::ManualOreEquipmentError;
pub(super) use equipment::resolve_manual_ore_equipment;
pub use job_validation::ManualOreJobValidationError;
pub(super) use job_validation::{
    validate_manual_ore_job_admission, validate_manual_ore_job_duration,
};

/// Shared physical failure for one direct-labor ore-processing batch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManualOrePhysicsError {
    ZeroBatchMass,
    BatchMassExceeded { selected: Mass, maximum: Mass },
    ThroughputDuration(MassFlowDurationError),
}

impl Display for ManualOrePhysicsError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroBatchMass => {
                formatter.write_str("manual ore-processing batch mass must be nonzero")
            }
            Self::BatchMassExceeded { selected, maximum } => write!(
                formatter,
                "selected manual ore-processing batch {} mg exceeds maximum {} mg",
                selected.milligrams(),
                maximum.milligrams()
            ),
            Self::ThroughputDuration(error) => {
                write!(formatter, "manual ore-processing duration failed: {error}")
            }
        }
    }
}

impl Error for ManualOrePhysicsError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ThroughputDuration(error) => Some(error),
            Self::ZeroBatchMass | Self::BatchMassExceeded { .. } => None,
        }
    }
}

/// Rejects a selected mass outside the authored manual processing envelope.
pub(super) fn validate_manual_ore_batch(
    profile: ManualOreProcessProfile,
    selected: Mass,
) -> Result<(), ManualOrePhysicsError> {
    if selected.is_zero() {
        return Err(ManualOrePhysicsError::ZeroBatchMass);
    }
    if selected > profile.max_batch_mass() {
        return Err(ManualOrePhysicsError::BatchMassExceeded {
            selected,
            maximum: profile.max_batch_mass(),
        });
    }
    Ok(())
}

/// Resolves exact whole-tick attention time from the shared manual throughput profile.
pub(crate) fn resolve_manual_ore_duration(
    physical_tick_duration: PhysicalTickDuration,
    profile: ManualOreProcessProfile,
    selected: Mass,
) -> Result<TickSpan, ManualOrePhysicsError> {
    calculate_mass_flow_duration_ceiling(
        profile.processing_rate(),
        selected,
        physical_tick_duration,
    )
    .map_err(ManualOrePhysicsError::ThroughputDuration)
}

/// Projects exact whole-tick attention for one authored manual ore-processing batch.
///
/// This is physical planning evidence only. Runtime process resolution still owns material
/// selection, output semantics, player-work admission, survival reserve, and state revisions.
pub fn project_manual_ore_duration(
    physical_tick_duration: PhysicalTickDuration,
    profile: ManualOreProcessProfile,
    selected: Mass,
) -> Result<TickSpan, ManualOrePhysicsError> {
    validate_manual_ore_batch(profile, selected)?;
    resolve_manual_ore_duration(physical_tick_duration, profile, selected)
}
