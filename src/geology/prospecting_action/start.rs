//! Admission and commit boundary for timed field prospecting.

use crate::core::state::AppState;
use crate::equipment::{
    EquipmentId, EquipmentOccupancy, EquipmentOperationTrace, equipment_occupancy,
    resolve_equipment_provider_with_occupancy,
};
use crate::labor::{
    PlayerWork, ProspectingDefinition, ProspectingMethodId, ProspectingWork,
    ValidatedPlayerWorkStart, validate_player_work_start,
};
use crate::maintenance::{Condition, calculate_usable_condition_after_active_ticks};
use crate::material::MaterialId;
use crate::registry::Registries;
use crate::spatial::VoxelBounds;

use super::errors::{FieldProspectingCommitError, FieldProspectingStartError};
use super::prospecting_observation_count;

/// One player-selected geological prospecting action over an authored-bounded region.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldProspectingRequest {
    method: ProspectingMethodId,
    region: VoxelBounds,
    material: MaterialId,
    equipment: Option<EquipmentId>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ProspectingEquipmentPlan {
    trace: Option<EquipmentOperationTrace>,
    condition_after: Option<Condition>,
    expected_revision: Option<u64>,
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
        validate_commit_equipment(
            state,
            self.work.equipment(),
            self.expected_equipment_revision,
        )?;
        self.work_start.apply(state);
        Ok(())
    }

    #[must_use]
    pub const fn work(&self) -> ProspectingWork {
        self.work
    }
}

fn validate_commit_equipment(
    state: &AppState,
    equipment: Option<EquipmentId>,
    expected_revision: Option<u64>,
) -> Result<(), FieldProspectingCommitError> {
    if let Some(expected) = expected_revision {
        let actual = state.equipment().revision();
        if actual != expected {
            return Err(FieldProspectingCommitError::StaleEquipmentRevision { expected, actual });
        }
    }
    let Some(equipment) = equipment else {
        return Ok(());
    };
    match equipment_occupancy(state, equipment) {
        Some(EquipmentOccupancy::Production { job, .. }) => {
            Err(FieldProspectingCommitError::EquipmentBusyProduction { equipment, job })
        }
        Some(EquipmentOccupancy::Mining { job }) => {
            Err(FieldProspectingCommitError::EquipmentBusyMining { equipment, job })
        }
        Some(EquipmentOccupancy::ManualPower { .. }) => {
            Err(FieldProspectingCommitError::EquipmentBusyManualPower { equipment })
        }
        Some(EquipmentOccupancy::Prospecting { .. } | EquipmentOccupancy::Maintenance { .. })
        | None => Ok(()),
    }
}

fn validate_prospecting_target(
    registries: &Registries,
    request: FieldProspectingRequest,
    method: ProspectingDefinition,
) -> Result<(), FieldProspectingStartError> {
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
    Ok(())
}

fn validate_start_equipment_occupancy(
    occupancy: Option<EquipmentOccupancy>,
    equipment: EquipmentId,
) -> Result<(), FieldProspectingStartError> {
    match occupancy {
        Some(EquipmentOccupancy::Production { job, .. }) => {
            Err(FieldProspectingStartError::EquipmentBusyProduction { equipment, job })
        }
        Some(EquipmentOccupancy::Mining { job }) => {
            Err(FieldProspectingStartError::EquipmentBusyMining { equipment, job })
        }
        Some(EquipmentOccupancy::ManualPower { .. }) => {
            Err(FieldProspectingStartError::EquipmentBusyManualPower { equipment })
        }
        Some(EquipmentOccupancy::Prospecting { .. } | EquipmentOccupancy::Maintenance { .. })
        | None => Ok(()),
    }
}

fn resolve_prospecting_equipment_plan(
    registries: &Registries,
    state: &AppState,
    request: FieldProspectingRequest,
    method: ProspectingDefinition,
) -> Result<ProspectingEquipmentPlan, FieldProspectingStartError> {
    let (profile, equipment) = match (method.equipment(), request.equipment) {
        (None, None) => return Ok(ProspectingEquipmentPlan::default()),
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
        (Some(profile), Some(equipment)) => (profile, equipment),
    };
    let (provider, occupancy) =
        resolve_equipment_provider_with_occupancy(registries, state, equipment)
            .map_err(FieldProspectingStartError::Equipment)?;
    let equipment_definition = provider.definition().id();
    let Some(condition_wear_ppm_per_active_tick) =
        profile.condition_wear_ppm_per_active_tick(equipment_definition)
    else {
        return Err(FieldProspectingStartError::EquipmentDefinitionNotAccepted {
            method: request.method,
            equipment,
        });
    };
    if state
        .equipment()
        .get_equipment(equipment)
        .is_some_and(|record| record.supported_by().is_some())
    {
        return Err(FieldProspectingStartError::EquipmentMounted { equipment });
    }
    validate_start_equipment_occupancy(occupancy, equipment)?;
    let use_trace = provider.validated_use();
    let condition_after = calculate_usable_condition_after_active_ticks(
        condition_wear_ppm_per_active_tick,
        provider.condition(),
        method.duration(),
    )
    .map_err(FieldProspectingStartError::ConditionDuration)?;
    Ok(ProspectingEquipmentPlan {
        trace: Some(use_trace.trace()),
        condition_after: Some(condition_after),
        expected_revision: Some(use_trace.expected_equipment_revision()),
    })
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
    validate_prospecting_target(registries, request, method)?;
    let equipment_plan = resolve_prospecting_equipment_plan(registries, state, request, method)?;
    let observation_count =
        prospecting_observation_count(method.spatial_resolution(), request.region)
            .ok_or(FieldProspectingStartError::ObservationIdExhausted)?;
    state
        .geological_knowledge()
        .next_observation_id()
        .checked_add(observation_count)
        .ok_or(FieldProspectingStartError::ObservationIdExhausted)?;
    state
        .geological_knowledge()
        .revision()
        .checked_add(u64::from(observation_count))
        .ok_or(FieldProspectingStartError::KnowledgeRevisionExhausted)?;
    if equipment_plan.trace.is_some() {
        state
            .equipment()
            .revision()
            .checked_add(1)
            .ok_or(FieldProspectingStartError::EquipmentRevisionExhausted)?;
    }
    let completes_at = state
        .tick()
        .checked_add_span(method.duration())
        .ok_or(FieldProspectingStartError::CompletionTickOverflow)?;
    let work = ProspectingWork::new(
        request.method,
        request.region,
        request.material,
        equipment_plan.trace,
        equipment_plan.condition_after,
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
        expected_equipment_revision: equipment_plan.expected_revision,
    })
}
