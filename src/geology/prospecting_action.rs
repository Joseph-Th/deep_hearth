//! Timed player field prospecting that converts bounded regional observation into geological knowledge.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::equipment::{
    EquipmentId, EquipmentOccupancy, EquipmentProviderError, equipment_occupancy,
    resolve_equipment_provider,
};
use crate::labor::{
    PlayerWork, PlayerWorkCommitError, PlayerWorkStartError, ProspectingMethodId, ProspectingWork,
    ValidatedPlayerWorkStart, validate_player_work_start,
};
use crate::maintenance::{
    ActiveConditionDurationError, calculate_usable_condition_after_active_ticks,
};
use crate::material::MaterialId;
use crate::mining::MiningJobId;
use crate::production::ProductionJobId;
use crate::registry::Registries;
use crate::spatial::VoxelBounds;

mod abundance;
mod hardness;
mod tick;

pub use tick::FieldProspectingOutcome;
pub(crate) use tick::{
    FieldProspectingTickError, apply_field_prospecting_tick, decide_field_prospecting_tick,
};

/// One player-selected geological prospecting action over an authored-bounded region.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldProspectingRequest {
    method: ProspectingMethodId,
    region: VoxelBounds,
    material: MaterialId,
    equipment: Option<EquipmentId>,
}

impl FieldProspectingRequest {
    pub const fn new(
        method: ProspectingMethodId,
        region: VoxelBounds,
        material: MaterialId,
    ) -> Self {
        Self {
            method,
            region,
            material,
            equipment: None,
        }
    }

    pub const fn new_with_equipment(
        method: ProspectingMethodId,
        region: VoxelBounds,
        material: MaterialId,
        equipment: EquipmentId,
    ) -> Self {
        Self {
            method,
            region,
            material,
            equipment: Some(equipment),
        }
    }

    #[must_use]
    pub const fn method(self) -> ProspectingMethodId {
        self.method
    }

    #[must_use]
    pub const fn region(self) -> VoxelBounds {
        self.region
    }

    #[must_use]
    pub const fn material(self) -> MaterialId {
        self.material
    }

    #[must_use]
    pub const fn equipment(self) -> Option<EquipmentId> {
        self.equipment
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldProspectingStartError {
    UnknownMethod {
        method: ProspectingMethodId,
    },
    UnknownMaterial {
        material: MaterialId,
    },
    RegionVolumeOverflow,
    RegionTooLarge {
        actual: u128,
        maximum: u128,
    },
    EquipmentRequired {
        method: ProspectingMethodId,
    },
    UnexpectedEquipment {
        method: ProspectingMethodId,
        equipment: EquipmentId,
    },
    Equipment(EquipmentProviderError),
    EquipmentMounted {
        equipment: EquipmentId,
    },
    EquipmentDefinitionNotAccepted {
        method: ProspectingMethodId,
        equipment: EquipmentId,
    },
    EquipmentBusyProduction {
        equipment: EquipmentId,
        job: ProductionJobId,
    },
    EquipmentBusyMining {
        equipment: EquipmentId,
        job: MiningJobId,
    },
    EquipmentBusyManualPower {
        equipment: EquipmentId,
    },
    ConditionDuration(ActiveConditionDurationError),
    CompletionTickOverflow,
    Work(PlayerWorkStartError),
}

impl Display for FieldProspectingStartError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownMethod { method } => {
                write!(formatter, "unknown prospecting method {}", method.value())
            }
            Self::UnknownMaterial { material } => {
                write!(
                    formatter,
                    "unknown prospecting material {}",
                    material.value()
                )
            }
            Self::RegionVolumeOverflow => {
                formatter.write_str("prospecting region voxel count overflowed")
            }
            Self::RegionTooLarge { actual, maximum } => write!(
                formatter,
                "prospecting region contains {actual} voxels but method allows at most {maximum}"
            ),
            Self::EquipmentRequired { method } => write!(
                formatter,
                "prospecting method {} requires a physical sampling instrument",
                method.value()
            ),
            Self::UnexpectedEquipment { method, equipment } => write!(
                formatter,
                "prospecting method {} does not use equipment but equipment {} was supplied",
                method.value(),
                equipment.value()
            ),
            Self::Equipment(error) => {
                write!(formatter, "prospecting equipment unavailable: {error}")
            }
            Self::EquipmentMounted { equipment } => write!(
                formatter,
                "prospecting sampling instrument {} must be portable and unmounted",
                equipment.value()
            ),
            Self::EquipmentDefinitionNotAccepted { method, equipment } => write!(
                formatter,
                "prospecting method {} does not accept equipment {}",
                method.value(),
                equipment.value()
            ),
            Self::EquipmentBusyProduction { equipment, job } => write!(
                formatter,
                "prospecting equipment {} is occupied by production job {}",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "prospecting equipment {} is occupied by mining job {}",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyManualPower { equipment } => write!(
                formatter,
                "prospecting equipment {} is occupied by direct manual power work",
                equipment.value()
            ),
            Self::ConditionDuration(error) => write!(
                formatter,
                "prospecting sampling instrument cannot survive the survey: {error}"
            ),
            Self::CompletionTickOverflow => {
                formatter.write_str("prospecting completion tick overflowed")
            }
            Self::Work(error) => write!(formatter, "prospecting labor admission failed: {error}"),
        }
    }
}

impl Error for FieldProspectingStartError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Work(error) => Some(error),
            Self::Equipment(error) => Some(error),
            Self::ConditionDuration(error) => Some(error),
            Self::UnknownMethod { .. }
            | Self::UnknownMaterial { .. }
            | Self::RegionVolumeOverflow
            | Self::RegionTooLarge { .. }
            | Self::EquipmentRequired { .. }
            | Self::UnexpectedEquipment { .. }
            | Self::EquipmentMounted { .. }
            | Self::EquipmentDefinitionNotAccepted { .. }
            | Self::EquipmentBusyProduction { .. }
            | Self::EquipmentBusyMining { .. }
            | Self::EquipmentBusyManualPower { .. }
            | Self::CompletionTickOverflow => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldProspectingCommitError {
    Work(PlayerWorkCommitError),
    StaleEquipmentRevision {
        expected: u64,
        actual: u64,
    },
    EquipmentBusyProduction {
        equipment: EquipmentId,
        job: ProductionJobId,
    },
    EquipmentBusyMining {
        equipment: EquipmentId,
        job: MiningJobId,
    },
    EquipmentBusyManualPower {
        equipment: EquipmentId,
    },
}

impl Display for FieldProspectingCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Work(error) => write!(formatter, "prospecting labor commit failed: {error}"),
            Self::StaleEquipmentRevision { expected, actual } => write!(
                formatter,
                "prospecting equipment expected revision {expected} but current revision is {actual}"
            ),
            Self::EquipmentBusyProduction { equipment, job } => write!(
                formatter,
                "prospecting equipment {} became occupied by production job {}",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "prospecting equipment {} became occupied by mining job {}",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyManualPower { equipment } => write!(
                formatter,
                "prospecting equipment {} became occupied by direct manual power work",
                equipment.value()
            ),
        }
    }
}

impl Error for FieldProspectingCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Work(error) => Some(error),
            Self::StaleEquipmentRevision { .. }
            | Self::EquipmentBusyProduction { .. }
            | Self::EquipmentBusyMining { .. }
            | Self::EquipmentBusyManualPower { .. } => None,
        }
    }
}

#[must_use]
pub struct ValidatedFieldProspectingStart {
    work_start: ValidatedPlayerWorkStart,
    work: ProspectingWork,
    expected_equipment_revision: Option<u64>,
}

impl ValidatedFieldProspectingStart {
    pub fn commit(self, state: &mut AppState) -> Result<(), FieldProspectingCommitError> {
        self.work_start
            .precheck(state)
            .map_err(FieldProspectingCommitError::Work)?;
        if let Some(expected) = self.expected_equipment_revision {
            let actual = state.equipment().revision();
            if actual != expected {
                return Err(FieldProspectingCommitError::StaleEquipmentRevision {
                    expected,
                    actual,
                });
            }
        }
        if let Some(equipment) = self.work.equipment() {
            match equipment_occupancy(state, equipment) {
                Some(EquipmentOccupancy::Production { job, .. }) => {
                    return Err(FieldProspectingCommitError::EquipmentBusyProduction {
                        equipment,
                        job,
                    });
                }
                Some(EquipmentOccupancy::Mining { job }) => {
                    return Err(FieldProspectingCommitError::EquipmentBusyMining {
                        equipment,
                        job,
                    });
                }
                Some(EquipmentOccupancy::ManualPower { .. }) => {
                    return Err(FieldProspectingCommitError::EquipmentBusyManualPower {
                        equipment,
                    });
                }
                Some(
                    EquipmentOccupancy::Prospecting { .. } | EquipmentOccupancy::Maintenance { .. },
                )
                | None => {}
            }
        }
        self.work_start.apply(state);
        Ok(())
    }

    #[must_use]
    pub const fn work(&self) -> ProspectingWork {
        self.work
    }
}

pub fn validate_start_field_prospecting(
    registries: &Registries,
    state: &AppState,
    request: FieldProspectingRequest,
) -> Result<ValidatedFieldProspectingStart, FieldProspectingStartError> {
    let method = registries
        .labor()
        .get_prospecting(request.method)
        .copied()
        .ok_or(FieldProspectingStartError::UnknownMethod {
            method: request.method,
        })?;
    if registries
        .materials()
        .get_material(request.material)
        .is_none()
    {
        return Err(FieldProspectingStartError::UnknownMaterial {
            material: request.material,
        });
    }
    let region_voxels = request
        .region
        .voxel_count()
        .ok_or(FieldProspectingStartError::RegionVolumeOverflow)?;
    if region_voxels > method.maximum_region_voxels() {
        return Err(FieldProspectingStartError::RegionTooLarge {
            actual: region_voxels,
            maximum: method.maximum_region_voxels(),
        });
    }
    let (equipment_trace, condition_after, expected_equipment_revision) =
        match (method.equipment(), request.equipment) {
            (None, None) => (None, None, None),
            (None, Some(equipment)) => {
                return Err(FieldProspectingStartError::UnexpectedEquipment {
                    method: request.method,
                    equipment,
                });
            }
            (Some(_), None) => {
                return Err(FieldProspectingStartError::EquipmentRequired {
                    method: request.method,
                });
            }
            (Some(profile), Some(equipment)) => {
                let provider = resolve_equipment_provider(registries, state, equipment)
                    .map_err(FieldProspectingStartError::Equipment)?;
                if !profile.accepts(provider.definition().id()) {
                    return Err(FieldProspectingStartError::EquipmentDefinitionNotAccepted {
                        method: request.method,
                        equipment,
                    });
                }
                if state
                    .equipment()
                    .get_equipment(equipment)
                    .is_some_and(|record| record.supported_by().is_some())
                {
                    return Err(FieldProspectingStartError::EquipmentMounted { equipment });
                }
                match equipment_occupancy(state, equipment) {
                    Some(EquipmentOccupancy::Production { job, .. }) => {
                        return Err(FieldProspectingStartError::EquipmentBusyProduction {
                            equipment,
                            job,
                        });
                    }
                    Some(EquipmentOccupancy::Mining { job }) => {
                        return Err(FieldProspectingStartError::EquipmentBusyMining {
                            equipment,
                            job,
                        });
                    }
                    Some(EquipmentOccupancy::ManualPower { .. }) => {
                        return Err(FieldProspectingStartError::EquipmentBusyManualPower {
                            equipment,
                        });
                    }
                    Some(
                        EquipmentOccupancy::Prospecting { .. }
                        | EquipmentOccupancy::Maintenance { .. },
                    )
                    | None => {}
                }
                let use_trace = provider.validated_use();
                let condition_after = calculate_usable_condition_after_active_ticks(
                    profile.condition_wear_ppm_per_active_tick(),
                    provider.condition(),
                    method.duration(),
                )
                .map_err(FieldProspectingStartError::ConditionDuration)?;
                (
                    Some(use_trace.trace()),
                    Some(condition_after),
                    Some(use_trace.expected_equipment_revision()),
                )
            }
        };
    let completes_at = state
        .tick()
        .checked_add_span(method.duration())
        .ok_or(FieldProspectingStartError::CompletionTickOverflow)?;
    let work = ProspectingWork::new(
        request.method,
        request.region,
        request.material,
        equipment_trace,
        condition_after,
        state.tick(),
        completes_at,
    );
    let work_start = validate_player_work_start(
        registries,
        state,
        PlayerWork::Prospecting { work },
        method.duration(),
        method.exertion(),
    )
    .map_err(FieldProspectingStartError::Work)?;
    Ok(ValidatedFieldProspectingStart {
        work_start,
        work,
        expected_equipment_revision,
    })
}

#[cfg(test)]
#[path = "prospecting_action_tests.rs"]
mod tests;
