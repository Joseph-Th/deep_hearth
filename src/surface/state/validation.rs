//! Trusted-load validation for finite loose surface-resource state.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::time::{ControlTick, SimulationTick};
use crate::material::{
    CommodityKey, CompositionError, FormId, MaterialId, MaterialPhase, MaterialPhaseStateError,
    MaterialRegistry, ParticleSizeStateError, validate_material_particle_size_state,
    validate_material_phase_state,
};
use crate::spatial::VoxelCoord;
use crate::surface::material_drop::PLAYER_MATERIAL_DROP_PICKUP_DELAY_CONTROL_TICKS;
use crate::world::{BlockCollisionShape, WorldRegistry, WorldState};

use super::{
    SurfaceResourceId, SurfaceResourceLifecycle, SurfaceResourceRecord, SurfaceResourceState,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SurfaceResourceValidationError {
    InvalidLocationIndex,
    EmbeddedInSolidWorld {
        resource: SurfaceResourceId,
        voxel: VoxelCoord,
    },
    ZeroNextResourceId,
    NextIdNotAfterExisting {
        next: u32,
        highest: SurfaceResourceId,
    },
    ZeroResourceId,
    IdMismatch {
        key: SurfaceResourceId,
        record: SurfaceResourceId,
    },
    ZeroInitialMass {
        resource: SurfaceResourceId,
    },
    RemainingMassExceedsInitial {
        resource: SurfaceResourceId,
    },
    AvailableWithoutMass {
        resource: SurfaceResourceId,
    },
    DepletedWithRemainingMass {
        resource: SurfaceResourceId,
    },
    AvailableHasDepletionTick {
        resource: SurfaceResourceId,
    },
    DepletedMissingDepletionTick {
        resource: SurfaceResourceId,
    },
    CreatedInFuture {
        resource: SurfaceResourceId,
    },
    DepletionBeforeCreation {
        resource: SurfaceResourceId,
    },
    DepletedInFuture {
        resource: SurfaceResourceId,
    },
    PickupDelayTickMismatch {
        resource: SurfaceResourceId,
    },
    PickupDelayStartedInFuture {
        resource: SurfaceResourceId,
    },
    PickupDelayScheduleOverflow {
        resource: SurfaceResourceId,
    },
    InvalidComposition {
        resource: SurfaceResourceId,
        error: CompositionError,
    },
    CompositionMissingHost {
        resource: SurfaceResourceId,
        host: MaterialId,
    },
    UnknownMaterial {
        resource: SurfaceResourceId,
        material: MaterialId,
    },
    UnknownForm {
        resource: SurfaceResourceId,
        form: FormId,
    },
    UnsupportedCommodity {
        resource: SurfaceResourceId,
        commodity: CommodityKey,
    },
    UnsupportedPhase {
        resource: SurfaceResourceId,
        form: FormId,
        phase: MaterialPhase,
    },
    UnknownCompositionMaterial {
        resource: SurfaceResourceId,
        material: MaterialId,
    },
    InvalidPhaseState {
        resource: SurfaceResourceId,
        error: MaterialPhaseStateError,
    },
    InvalidParticleSizeState {
        resource: SurfaceResourceId,
        error: ParticleSizeStateError,
    },
    ProvenanceAfterSurfaceCreation {
        resource: SurfaceResourceId,
        latest_created_at: SimulationTick,
        surface_created_at: SimulationTick,
    },
    UnreachableAmbientAge {
        resource: SurfaceResourceId,
    },
}

impl Display for SurfaceResourceValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidLocationIndex => formatter.write_str(
                "surface resource position index does not match available parcel custody",
            ),
            Self::EmbeddedInSolidWorld { resource, voxel } => write!(
                formatter,
                "available surface resource {} is embedded in solid generated terrain at ({},{},{})",
                resource.value(),
                voxel.x(),
                voxel.y(),
                voxel.z()
            ),
            Self::ZeroNextResourceId => {
                formatter.write_str("surface next resource id must be nonzero")
            }
            Self::NextIdNotAfterExisting { next, highest } => write!(
                formatter,
                "surface next resource id {next} does not follow highest existing resource {}",
                highest.value()
            ),
            Self::ZeroResourceId => formatter.write_str("surface resource id must be nonzero"),
            Self::IdMismatch { key, record } => write!(
                formatter,
                "surface resource map key {} does not match record id {}",
                key.value(),
                record.value()
            ),
            Self::ZeroInitialMass { resource } => write!(
                formatter,
                "surface resource {} has zero initial mass",
                resource.value()
            ),
            Self::RemainingMassExceedsInitial { resource } => write!(
                formatter,
                "surface resource {} remaining mass exceeds its initial mass",
                resource.value()
            ),
            Self::AvailableWithoutMass { resource } => write!(
                formatter,
                "available surface resource {} has no remaining mass",
                resource.value()
            ),
            Self::DepletedWithRemainingMass { resource } => write!(
                formatter,
                "depleted surface resource {} still has remaining mass",
                resource.value()
            ),
            Self::AvailableHasDepletionTick { resource } => write!(
                formatter,
                "available surface resource {} already records a depletion tick",
                resource.value()
            ),
            Self::DepletedMissingDepletionTick { resource } => write!(
                formatter,
                "depleted surface resource {} has no depletion tick",
                resource.value()
            ),
            Self::CreatedInFuture { resource } => write!(
                formatter,
                "surface resource {} was created after the current simulation tick",
                resource.value()
            ),
            Self::DepletionBeforeCreation { resource } => write!(
                formatter,
                "surface resource {} depletion predates creation",
                resource.value()
            ),
            Self::DepletedInFuture { resource } => write!(
                formatter,
                "surface resource {} depletion tick is in the future",
                resource.value()
            ),
            Self::PickupDelayTickMismatch { resource } => write!(
                formatter,
                "surface resource {} pickup-delay control tick does not belong to its surface-creation simulation tick",
                resource.value()
            ),
            Self::PickupDelayStartedInFuture { resource } => write!(
                formatter,
                "surface resource {} pickup-delay control tick is in the future",
                resource.value()
            ),
            Self::PickupDelayScheduleOverflow { resource } => write!(
                formatter,
                "surface resource {} pickup delay exceeds authoritative control time",
                resource.value()
            ),
            Self::InvalidComposition { resource, error } => write!(
                formatter,
                "surface resource {} has invalid composition: {error}",
                resource.value()
            ),
            Self::CompositionMissingHost { resource, host } => write!(
                formatter,
                "surface resource {} composition omits host material {}",
                resource.value(),
                host.value()
            ),
            Self::UnknownMaterial { resource, material } => write!(
                formatter,
                "surface resource {} references unknown material {}",
                resource.value(),
                material.value()
            ),
            Self::UnknownForm { resource, form } => write!(
                formatter,
                "surface resource {} references unknown form {}",
                resource.value(),
                form.value()
            ),
            Self::UnsupportedCommodity {
                resource,
                commodity,
            } => write!(
                formatter,
                "surface resource {} references unsupported commodity material {} form {}",
                resource.value(),
                commodity.material().value(),
                commodity.form().value()
            ),
            Self::UnsupportedPhase {
                resource,
                form,
                phase: _,
            } => write!(
                formatter,
                "surface resource {} form {} is not solid",
                resource.value(),
                form.value()
            ),
            Self::UnknownCompositionMaterial { resource, material } => write!(
                formatter,
                "surface resource {} composition references unknown material {}",
                resource.value(),
                material.value()
            ),
            Self::InvalidPhaseState { resource, error } => write!(
                formatter,
                "surface resource {} has invalid material phase state: {error}",
                resource.value()
            ),
            Self::InvalidParticleSizeState { resource, error } => write!(
                formatter,
                "surface resource {} has invalid particle-size state: {error}",
                resource.value()
            ),
            Self::ProvenanceAfterSurfaceCreation {
                resource,
                latest_created_at,
                surface_created_at,
            } => write!(
                formatter,
                "surface resource {} contains matter created at tick {} after its surface custody began at tick {}",
                resource.value(),
                latest_created_at.value(),
                surface_created_at.value()
            ),
            Self::UnreachableAmbientAge { resource } => write!(
                formatter,
                "surface resource {} has physically unreachable accumulated ambient exposure",
                resource.value()
            ),
        }
    }
}

impl Error for SurfaceResourceValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidComposition { error, .. } => Some(error),
            Self::InvalidPhaseState { error, .. } => Some(error),
            Self::InvalidParticleSizeState { error, .. } => Some(error),
            Self::InvalidLocationIndex
            | Self::EmbeddedInSolidWorld { .. }
            | Self::ZeroNextResourceId
            | Self::NextIdNotAfterExisting { .. }
            | Self::ZeroResourceId
            | Self::IdMismatch { .. }
            | Self::ZeroInitialMass { .. }
            | Self::RemainingMassExceedsInitial { .. }
            | Self::AvailableWithoutMass { .. }
            | Self::DepletedWithRemainingMass { .. }
            | Self::AvailableHasDepletionTick { .. }
            | Self::DepletedMissingDepletionTick { .. }
            | Self::CreatedInFuture { .. }
            | Self::DepletionBeforeCreation { .. }
            | Self::DepletedInFuture { .. }
            | Self::PickupDelayTickMismatch { .. }
            | Self::PickupDelayStartedInFuture { .. }
            | Self::PickupDelayScheduleOverflow { .. }
            | Self::CompositionMissingHost { .. }
            | Self::UnknownMaterial { .. }
            | Self::UnknownForm { .. }
            | Self::UnsupportedCommodity { .. }
            | Self::UnsupportedPhase { .. }
            | Self::UnknownCompositionMaterial { .. }
            | Self::ProvenanceAfterSurfaceCreation { .. }
            | Self::UnreachableAmbientAge { .. } => None,
        }
    }
}

fn validate_material_profile(
    materials: &MaterialRegistry,
    resource: SurfaceResourceId,
    record: &SurfaceResourceRecord,
) -> Result<(), SurfaceResourceValidationError> {
    let commodity = record.commodity();
    if materials.get_material(commodity.material()).is_none() {
        return Err(SurfaceResourceValidationError::UnknownMaterial {
            resource,
            material: commodity.material(),
        });
    }
    let Some(form) = materials.get_form(commodity.form()) else {
        return Err(SurfaceResourceValidationError::UnknownForm {
            resource,
            form: commodity.form(),
        });
    };
    if !materials.has_commodity(commodity) {
        return Err(SurfaceResourceValidationError::UnsupportedCommodity {
            resource,
            commodity,
        });
    }
    if form.phase() != MaterialPhase::Solid {
        return Err(SurfaceResourceValidationError::UnsupportedPhase {
            resource,
            form: commodity.form(),
            phase: form.phase(),
        });
    }
    for component in record.composition().components() {
        if materials.get_material(component.material()).is_none() {
            return Err(SurfaceResourceValidationError::UnknownCompositionMaterial {
                resource,
                material: component.material(),
            });
        }
    }
    validate_material_phase_state(
        materials,
        commodity,
        record.composition(),
        record.temperature(),
    )
    .map_err(|error| SurfaceResourceValidationError::InvalidPhaseState { resource, error })?;
    validate_material_particle_size_state(
        materials,
        commodity,
        record.material_profile().particle_size_distribution(),
    )
    .map_err(|error| SurfaceResourceValidationError::InvalidParticleSizeState { resource, error })
}

pub(in crate::surface) fn validate_surface_resource_record(
    materials: &MaterialRegistry,
    key: SurfaceResourceId,
    record: &SurfaceResourceRecord,
    current: SimulationTick,
    current_control: ControlTick,
) -> Result<(), SurfaceResourceValidationError> {
    if key.value() == 0 || record.id.value() == 0 {
        return Err(SurfaceResourceValidationError::ZeroResourceId);
    }
    if key != record.id {
        return Err(SurfaceResourceValidationError::IdMismatch {
            key,
            record: record.id,
        });
    }
    if record.initial_mass().is_zero() {
        return Err(SurfaceResourceValidationError::ZeroInitialMass { resource: key });
    }
    if record.remaining_mass() > record.initial_mass() {
        return Err(SurfaceResourceValidationError::RemainingMassExceedsInitial { resource: key });
    }
    match record.lifecycle {
        SurfaceResourceLifecycle::Available if record.remaining_mass.is_zero() => {
            return Err(SurfaceResourceValidationError::AvailableWithoutMass { resource: key });
        }
        SurfaceResourceLifecycle::Depleted if !record.remaining_mass.is_zero() => {
            return Err(SurfaceResourceValidationError::DepletedWithRemainingMass {
                resource: key,
            });
        }
        SurfaceResourceLifecycle::Available | SurfaceResourceLifecycle::Depleted => {}
    }
    if record.created_at > current {
        return Err(SurfaceResourceValidationError::CreatedInFuture { resource: key });
    }
    if let Some(started_at) = record.automatic_pickup_delay_started_at() {
        if started_at.simulation_tick() != record.created_at() {
            return Err(SurfaceResourceValidationError::PickupDelayTickMismatch { resource: key });
        }
        if started_at > current_control {
            return Err(SurfaceResourceValidationError::PickupDelayStartedInFuture {
                resource: key,
            });
        }
        if started_at
            .checked_add_steps(PLAYER_MATERIAL_DROP_PICKUP_DELAY_CONTROL_TICKS)
            .is_none()
        {
            return Err(
                SurfaceResourceValidationError::PickupDelayScheduleOverflow { resource: key },
            );
        }
    }
    match (record.lifecycle, record.depleted_at) {
        (SurfaceResourceLifecycle::Available, Some(_)) => {
            return Err(SurfaceResourceValidationError::AvailableHasDepletionTick {
                resource: key,
            });
        }
        (SurfaceResourceLifecycle::Depleted, None) => {
            return Err(
                SurfaceResourceValidationError::DepletedMissingDepletionTick { resource: key },
            );
        }
        (SurfaceResourceLifecycle::Depleted, Some(at)) if at < record.created_at => {
            return Err(SurfaceResourceValidationError::DepletionBeforeCreation { resource: key });
        }
        (SurfaceResourceLifecycle::Depleted, Some(at)) if at > current => {
            return Err(SurfaceResourceValidationError::DepletedInFuture { resource: key });
        }
        _ => {}
    }
    let latest_created_at = record.material_provenance().latest_created_at();
    if latest_created_at > record.created_at() {
        return Err(
            SurfaceResourceValidationError::ProvenanceAfterSurfaceCreation {
                resource: key,
                latest_created_at,
                surface_created_at: record.created_at(),
            },
        );
    }
    if !crate::inventory::MaterialStorageHistory::with_ambient_age_parts(
        record.ambient_age_parts_at_created_at(),
        record.created_at(),
    )
    .has_reachable_accumulated_age()
    {
        return Err(SurfaceResourceValidationError::UnreachableAmbientAge { resource: key });
    }
    record.composition().validate().map_err(|error| {
        SurfaceResourceValidationError::InvalidComposition {
            resource: key,
            error,
        }
    })?;
    if record
        .composition()
        .parts_per_million(record.commodity().material())
        == 0
    {
        return Err(SurfaceResourceValidationError::CompositionMissingHost {
            resource: key,
            host: record.commodity().material(),
        });
    }
    validate_material_profile(materials, key, record)
}

/// Shared physical placement rule for both new surface custody and decoded
/// persistent custody. A missing world column is not yet solid authority.
pub(in crate::surface) fn validate_surface_resource_world_position(
    world_registry: &WorldRegistry,
    world: &WorldState,
    record: &SurfaceResourceRecord,
) -> Result<(), SurfaceResourceValidationError> {
    if record.lifecycle() == SurfaceResourceLifecycle::Available
        && world
            .block_at(record.position())
            .and_then(|block| world_registry.block_for_state(block))
            .is_some_and(|block| block.collision() == BlockCollisionShape::FullCube)
    {
        return Err(SurfaceResourceValidationError::EmbeddedInSolidWorld {
            resource: record.id(),
            voxel: record.position(),
        });
    }
    Ok(())
}

pub(crate) fn validate_loaded_surface_resources(
    materials: &MaterialRegistry,
    state: &SurfaceResourceState,
    current: SimulationTick,
    current_control: ControlTick,
    world_registry: &WorldRegistry,
    world: &WorldState,
) -> Result<(), SurfaceResourceValidationError> {
    if state.next_resource_id == 0 {
        return Err(SurfaceResourceValidationError::ZeroNextResourceId);
    }
    if let Some(highest) = state.resources.keys().next_back().copied()
        && state.next_resource_id <= highest.value()
    {
        return Err(SurfaceResourceValidationError::NextIdNotAfterExisting {
            next: state.next_resource_id,
            highest,
        });
    }
    for (key, record) in &state.resources {
        validate_surface_resource_record(materials, *key, record, current, current_control)?;
        validate_surface_resource_world_position(world_registry, world, record)?;
    }
    if !state.has_valid_position_index() {
        return Err(SurfaceResourceValidationError::InvalidLocationIndex);
    }
    Ok(())
}
