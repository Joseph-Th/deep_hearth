//! Trusted-load validation for finite loose surface-resource state.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::time::{ControlTick, SimulationTick};
use crate::material::{
    CommodityKey, CompositionError, FormId, MaterialId, MaterialPhase, MaterialPhaseStateError,
    MaterialRegistry, ParticleSizeStateError, validate_material_particle_size_state,
    validate_material_phase_state,
};
use crate::spatial::VoxelCoord;
use crate::structural::StructureState;
use crate::surface::material_drop::PLAYER_MATERIAL_DROP_PICKUP_DELAY_CONTROL_TICKS;
use crate::world::{BlockCollisionShape, ChunkCoord, WorldRegistry, WorldState};

use super::{
    SurfaceResourceId, SurfaceResourceLifecycle, SurfaceResourceRecord, SurfaceResourceState,
};

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

fn validate_resource_identity_and_mass(
    key: SurfaceResourceId,
    record: &SurfaceResourceRecord,
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
    Ok(())
}

/// Perennial root, fruit and ripening clocks form one finite material budget.
fn validate_perennial_budget(
    key: SurfaceResourceId,
    record: &SurfaceResourceRecord,
    current: SimulationTick,
) -> Result<(), SurfaceResourceValidationError> {
    if let Some(plant) = record.perennial {
        let original = plant.original_fruit.milligrams();
        let expected_total = original.checked_add(original / 2);
        if record.kind() != super::SurfaceResourceKind::Opportunity
            || record.commodity().form() != crate::content::FORM_FOOD
            || !matches!(
                record.commodity().material(),
                crate::content::MATERIAL_BERRIES | crate::content::MATERIAL_WILD_NUTS
            )
            || record.created_at() != SimulationTick::ZERO
            || original == 0
            || expected_total != Some(record.initial_mass().milligrams())
            || plant.root_reserve > record.initial_mass()
            || record.remaining_mass().milligrams() > original
            || record
                .remaining_mass()
                .milligrams()
                .checked_add(plant.root_reserve.milligrams())
                .is_none_or(|sum| sum > record.initial_mass().milligrams())
            || plant.ripened_at < record.created_at()
            || plant.ripened_at > current
            || !plant
                .ripened_at
                .value()
                .is_multiple_of(crate::world::WEATHER_FRONT_TICKS)
            || record
                .depleted_at()
                .is_some_and(|depleted| plant.ripened_at > depleted)
            || record.ambient_age_parts_at_created_at() != 0
        {
            return Err(SurfaceResourceValidationError::InvalidPerennialBudget { resource: key });
        }
    }
    Ok(())
}

fn validate_resource_lifecycle_and_clocks(
    key: SurfaceResourceId,
    record: &SurfaceResourceRecord,
    current: SimulationTick,
    current_control: ControlTick,
) -> Result<(), SurfaceResourceValidationError> {
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
    Ok(())
}

fn validate_resource_material_history(
    materials: &MaterialRegistry,
    key: SurfaceResourceId,
    record: &SurfaceResourceRecord,
) -> Result<(), SurfaceResourceValidationError> {
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

/// Validates identity, finite biomass, lifecycle, time and physical composition in stable
/// rejection order. Runtime creation and trusted-load replay share this exact contract.
pub(in crate::surface) fn validate_surface_resource_record(
    materials: &MaterialRegistry,
    key: SurfaceResourceId,
    record: &SurfaceResourceRecord,
    current: SimulationTick,
    current_control: ControlTick,
) -> Result<(), SurfaceResourceValidationError> {
    validate_resource_identity_and_mass(key, record)?;
    validate_perennial_budget(key, record, current)?;
    validate_resource_lifecycle_and_clocks(key, record, current, current_control)?;
    validate_resource_material_history(materials, key, record)
}

/// Shared physical world obstruction for new, relocated, and decoded loose custody.
/// Tree trunks occupy cells whose underlying terrain block is still air.
/// A missing world column is not yet solid authority.
pub(in crate::surface) fn surface_position_has_solid_world(
    world_registry: &WorldRegistry,
    world: &WorldState,
    position: VoxelCoord,
) -> bool {
    surface_position_has_solid_world_after_felling(world_registry, world, position, None)
}

fn surface_position_has_solid_world_after_felling(
    world_registry: &WorldRegistry,
    world: &WorldState,
    position: VoxelCoord,
    felled_root: Option<VoxelCoord>,
) -> bool {
    // A validated atomic felling removes only the named root before its log
    // parcel lands. The adjacent upper trunk and all other trees remain solid.
    if felled_root == Some(position) && world.tree_at(position).is_some() {
        return world
            .block_at(position)
            .and_then(|block| world_registry.block_for_state(block))
            .is_some_and(|block| block.collision() == BlockCollisionShape::FullCube);
    }
    world.full_cell_collision_at(world_registry, position) == Some(true)
}

pub(in crate::surface) fn validate_surface_resource_physical_position(
    world_registry: &WorldRegistry,
    world: &WorldState,
    structures: &StructureState,
    record: &SurfaceResourceRecord,
    felled_root: Option<VoxelCoord>,
) -> Result<(), SurfaceResourceValidationError> {
    if (record.lifecycle() == SurfaceResourceLifecycle::Available
        || record.rooted_biomass_occupies_site())
        && surface_position_has_solid_world_after_felling(
            world_registry,
            world,
            record.position(),
            felled_root,
        )
    {
        return Err(SurfaceResourceValidationError::EmbeddedInSolidWorld {
            resource: record.id(),
            voxel: record.position(),
        });
    }
    if (record.lifecycle() == SurfaceResourceLifecycle::Available
        || record.rooted_biomass_occupies_site())
        && structures.blocks_full_cell(record.position())
    {
        return Err(SurfaceResourceValidationError::EmbeddedInSolidStructure {
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
    structures: &StructureState,
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
    // The world generator admits at most one perennial patch per selected
    // two-by-two district. Candidate provenance is not sufficient by itself:
    // two individually seed-plausible alternate sites must not both be loaded.
    let mut perennial_by_chunk = BTreeMap::<ChunkCoord, SurfaceResourceId>::new();
    for (key, record) in &state.resources {
        validate_surface_resource_record(materials, *key, record, current, current_control)?;
        if let Some(plant) = record.perennial {
            // Natural biomass enters custody only with the terrain chunk that
            // generated it. A seed-plausible site in an unexplored chunk is
            // not an actual plant and must not become trusted on save load.
            if !world.has_generated_terrain_at(record.position()) {
                return Err(SurfaceResourceValidationError::PerennialSiteNotGenerated {
                    resource: *key,
                    position: record.position(),
                });
            }
            if !world.generation_profile().is_some_and(|profile| {
                crate::world::seeded_perennial_matches(
                    profile,
                    record.position(),
                    record.commodity(),
                    plant.original_fruit,
                )
            }) {
                return Err(SurfaceResourceValidationError::InvalidPerennialBudget {
                    resource: *key,
                });
            }
            let chunk = ChunkCoord::containing(record.position());
            if let Some(previous) = perennial_by_chunk.insert(chunk, *key) {
                return Err(SurfaceResourceValidationError::DuplicateSeededPerennial {
                    resource: *key,
                    previous,
                    chunk,
                });
            }
        }
        validate_surface_resource_physical_position(
            world_registry,
            world,
            structures,
            record,
            None,
        )?;
    }
    if !state.has_valid_derived_indexes() {
        return Err(SurfaceResourceValidationError::InvalidLocationIndex);
    }
    Ok(())
}

#[cfg(test)]
#[path = "validation_perennial_tests.rs"]
mod perennial_tests;
