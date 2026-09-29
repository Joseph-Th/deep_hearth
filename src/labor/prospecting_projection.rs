//! Read-only player-work projection for authored prospecting methods.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::time::TickSpan;
use crate::registry::Registries;
use crate::spatial::VoxelBounds;
use crate::survival::SurvivalExertion;

use super::{
    PlayerWorkResourceBudget, PlayerWorkResourceBudgetError, ProspectingMethodId,
    ProspectingRegionError, calculate_player_work_resource_budget,
};

/// Authored work cost for one prospecting request before current-state authorization.
///
/// The projection validates the requested footprint and physiological workload. It does not prove
/// current equipment ownership, occupancy, player reserve, geological identity headroom, or state
/// revisions; runtime work must still use field-prospecting admission.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProspectingWorkProjection {
    duration: TickSpan,
    observation_count: u32,
    exertion: SurvivalExertion,
    resource_budget: PlayerWorkResourceBudget,
    requires_equipment: bool,
}

impl ProspectingWorkProjection {
    #[must_use]
    pub const fn duration(self) -> TickSpan {
        self.duration
    }

    #[must_use]
    pub const fn observation_count(self) -> u32 {
        self.observation_count
    }

    #[must_use]
    pub const fn exertion(self) -> SurvivalExertion {
        self.exertion
    }

    #[must_use]
    pub const fn resource_budget(self) -> PlayerWorkResourceBudget {
        self.resource_budget
    }

    #[must_use]
    pub const fn requires_equipment(self) -> bool {
        self.requires_equipment
    }
}

/// Failure while projecting prospecting from immutable authored definitions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProspectingWorkProjectionError {
    UnknownMethod { method: ProspectingMethodId },
    RegionVolumeOverflow,
    RegionTooLarge { actual: u128, maximum: u128 },
    ResourceBudgetOverflow,
}

impl Display for ProspectingWorkProjectionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownMethod { method } => {
                write!(
                    formatter,
                    "prospecting method {} is unknown",
                    method.value()
                )
            }
            Self::RegionVolumeOverflow => {
                formatter.write_str("prospecting region volume cannot be represented")
            }
            Self::RegionTooLarge { actual, maximum } => write!(
                formatter,
                "prospecting region contains {actual} voxels but the authored method allows at most {maximum}"
            ),
            Self::ResourceBudgetOverflow => {
                formatter.write_str("prospecting projected physiological budget overflows")
            }
        }
    }
}

impl Error for ProspectingWorkProjectionError {}

/// Projects one prospecting workload without requiring enough current reserve to admit it.
pub fn project_prospecting_work(
    registries: &Registries,
    method: ProspectingMethodId,
    region: VoxelBounds,
) -> Result<ProspectingWorkProjection, ProspectingWorkProjectionError> {
    let definition = registries
        .labor()
        .get_prospecting(method)
        .copied()
        .ok_or(ProspectingWorkProjectionError::UnknownMethod { method })?;
    let observation_count = definition
        .resolve_region_observation_count(region)
        .map_err(|error| match error {
            ProspectingRegionError::VolumeOverflow => {
                ProspectingWorkProjectionError::RegionVolumeOverflow
            }
            ProspectingRegionError::TooLarge { actual, maximum } => {
                ProspectingWorkProjectionError::RegionTooLarge { actual, maximum }
            }
        })?;
    let resource_budget = calculate_player_work_resource_budget(
        registries.survival().physiology(),
        definition.exertion(),
        definition.duration(),
    )
    .map_err(|error| match error {
        PlayerWorkResourceBudgetError::EnergyOverflow
        | PlayerWorkResourceBudgetError::HydrationOverflow => {
            ProspectingWorkProjectionError::ResourceBudgetOverflow
        }
    })?;
    Ok(ProspectingWorkProjection {
        duration: definition.duration(),
        observation_count,
        exertion: definition.exertion(),
        resource_budget,
        requires_equipment: definition.equipment().is_some(),
    })
}

#[cfg(test)]
#[path = "prospecting_projection_tests.rs"]
mod tests;
