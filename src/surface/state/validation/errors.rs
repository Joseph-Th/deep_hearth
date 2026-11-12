//! Typed rejection reasons for finite surface-resource validation.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::time::SimulationTick;
use crate::material::{
    CommodityKey, CompositionError, FormId, MaterialId, MaterialPhase, MaterialPhaseStateError,
    ParticleSizeStateError,
};
use crate::spatial::VoxelCoord;
use crate::world::ChunkCoord;

use super::super::SurfaceResourceId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SurfaceResourceValidationError {
    InvalidLocationIndex,
    InvalidPerennialBudget {
        resource: SurfaceResourceId,
    },
    PerennialSiteNotGenerated {
        resource: SurfaceResourceId,
        position: VoxelCoord,
    },
    DuplicateSeededPerennial {
        resource: SurfaceResourceId,
        previous: SurfaceResourceId,
        chunk: ChunkCoord,
    },
    EmbeddedInSolidWorld {
        resource: SurfaceResourceId,
        voxel: VoxelCoord,
    },
    EmbeddedInSolidStructure {
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
            Self::InvalidPerennialBudget { resource } => write!(
                formatter,
                "perennial {} has invalid finite root, crop age or initial biomass budget",
                resource.value()
            ),
            Self::PerennialSiteNotGenerated { resource, position } => write!(
                formatter,
                "perennial {} claims a site ({},{},{}) whose terrain has not been generated",
                resource.value(),
                position.x(),
                position.y(),
                position.z()
            ),
            Self::DuplicateSeededPerennial {
                resource,
                previous,
                chunk,
            } => write!(
                formatter,
                "perennial {} duplicates seeded patch {} in chunk ({},{})",
                resource.value(),
                previous.value(),
                chunk.x(),
                chunk.z()
            ),
            Self::InvalidLocationIndex => formatter.write_str(
                "surface resource position index does not match available parcel custody",
            ),
            Self::EmbeddedInSolidWorld { resource, voxel } => write!(
                formatter,
                "surface resource {} has physical biomass embedded in solid generated terrain at ({},{},{})",
                resource.value(),
                voxel.x(),
                voxel.y(),
                voxel.z()
            ),
            Self::EmbeddedInSolidStructure { resource, voxel } => write!(
                formatter,
                "surface resource {} has physical biomass embedded in a full-cell structural member at ({},{},{})",
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
            | Self::InvalidPerennialBudget { .. }
            | Self::PerennialSiteNotGenerated { .. }
            | Self::DuplicateSeededPerennial { .. }
            | Self::EmbeddedInSolidWorld { .. }
            | Self::EmbeddedInSolidStructure { .. }
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
