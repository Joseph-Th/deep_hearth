//! Spatial coherence for production endpoints whose world locations are already authoritative.

use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::energy::EnergyStoreId;
use crate::equipment::EquipmentId;
use crate::inventory::StockpileId;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

use super::{ProcessResolution, ProductionJobId, ProductionJobRecord};

/// One production endpoint that can already have an explicit logistics-owned world location.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductionSiteEndpoint {
    Source(StockpileId),
    Destination(StockpileId),
    Equipment(EquipmentId),
    EnergyStore(EnergyStoreId),
}

impl Display for ProductionSiteEndpoint {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Source(stockpile) => write!(formatter, "source stockpile {}", stockpile.value()),
            Self::Destination(stockpile) => {
                write!(formatter, "destination stockpile {}", stockpile.value())
            }
            Self::Equipment(equipment) => write!(formatter, "equipment {}", equipment.value()),
            Self::EnergyStore(store) => write!(formatter, "energy store {}", store.value()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProductionSiteMismatch {
    pub(crate) first: ProductionSiteEndpoint,
    pub(crate) first_position: VoxelCoord,
    pub(crate) second: ProductionSiteEndpoint,
    pub(crate) second_position: VoxelCoord,
}

fn bind_known_endpoint(
    anchor: &mut Option<(ProductionSiteEndpoint, VoxelCoord)>,
    endpoint: ProductionSiteEndpoint,
    position: Option<VoxelCoord>,
) -> Result<(), ProductionSiteMismatch> {
    let Some(position) = position else {
        return Ok(());
    };
    let Some((first, first_position)) = *anchor else {
        *anchor = Some((endpoint, position));
        return Ok(());
    };
    if position != first_position {
        return Err(ProductionSiteMismatch {
            first,
            first_position,
            second: endpoint,
            second_position: position,
        });
    }
    Ok(())
}

pub(crate) fn validate_process_start_site(
    state: &AppState,
    resolution: &ProcessResolution,
    source: StockpileId,
    destinations: impl IntoIterator<Item = StockpileId>,
) -> Result<(), ProductionSiteMismatch> {
    let mut anchor = None;
    bind_known_endpoint(
        &mut anchor,
        ProductionSiteEndpoint::Source(source),
        state.logistics().stockpile_position(source),
    )?;
    for destination in destinations {
        bind_known_endpoint(
            &mut anchor,
            ProductionSiteEndpoint::Destination(destination),
            state.logistics().stockpile_position(destination),
        )?;
    }
    if let Some(provider) = resolution.equipment_input() {
        bind_known_endpoint(
            &mut anchor,
            ProductionSiteEndpoint::Equipment(provider.equipment()),
            state.logistics().equipment_position(provider.equipment()),
        )?;
    }
    if let Some(energy) = resolution.energy_input() {
        bind_known_endpoint(
            &mut anchor,
            ProductionSiteEndpoint::EnergyStore(energy.source()),
            state.logistics().energy_store_position(energy.source()),
        )?;
    }
    if let Some(sink) = resolution.energy_sink() {
        let store = sink.trace().destination();
        bind_known_endpoint(
            &mut anchor,
            ProductionSiteEndpoint::EnergyStore(store),
            state.logistics().energy_store_position(store),
        )?;
    }
    Ok(())
}

/// Replays the spatial obligations that remain durable after production admission.
///
/// Suspension stops active time but does not dissolve the job's retained equipment, released-energy
/// sink, or output destinations. Those endpoints must therefore remain co-located while suspended
/// as well as while running, otherwise a loaded job could later resume physically remote work.
fn validate_retained_job_site_with_stockpile_override(
    state: &AppState,
    job: &ProductionJobRecord,
    stockpile_override: Option<(StockpileId, VoxelCoord)>,
) -> Result<(), ProductionSiteMismatch> {
    let stockpile_position = |stockpile| {
        stockpile_override
            .filter(|(overridden, _)| *overridden == stockpile)
            .map(|(_, position)| position)
            .or_else(|| state.logistics().stockpile_position(stockpile))
    };
    let mut anchor = None;
    if let Some(provider) = job.equipment_provider() {
        bind_known_endpoint(
            &mut anchor,
            ProductionSiteEndpoint::Equipment(provider.equipment()),
            state.logistics().equipment_position(provider.equipment()),
        )?;
    }
    if let Some(released) = job.released_energy() {
        let store = released.destination();
        bind_known_endpoint(
            &mut anchor,
            ProductionSiteEndpoint::EnergyStore(store),
            state.logistics().energy_store_position(store),
        )?;
    }
    for stream in job.output_streams() {
        let destination = stream.destination();
        bind_known_endpoint(
            &mut anchor,
            ProductionSiteEndpoint::Destination(destination),
            stockpile_position(destination),
        )?;
    }
    Ok(())
}

pub(crate) fn validate_retained_job_site(
    state: &AppState,
    job: &ProductionJobRecord,
) -> Result<(), ProductionSiteMismatch> {
    validate_retained_job_site_with_stockpile_override(state, job, None)
}

/// Only loose, stationary, unmounted storage depends on the collision cell
/// below its exact location. Carried custody and explicit support have their
/// own owners.
pub(crate) fn loose_output_destination_position(
    state: &AppState,
    destination: StockpileId,
) -> Option<VoxelCoord> {
    let pile = state.inventory().get_stockpile(destination)?;
    if pile.enclosure().is_some() || pile.supported_by().is_some() {
        return None;
    }
    state.logistics().stationary_stockpile_position(destination)
}

/// Abstract, ungenerated locations have no physical floor authority yet.
/// Once terrain exists, a reserved output needs actual fixed voxel support,
/// not the player's broader continuous-body footprint or a moving resident.
pub(crate) fn unsupported_output_destination(
    registries: &Registries,
    state: &AppState,
    destination: StockpileId,
) -> Option<VoxelCoord> {
    let position = loose_output_destination_position(state, destination)?;
    generated_output_position_lacks_footing(registries, state, position).then_some(position)
}

/// Evaluate the ground fallback independently of a current support assignment,
/// so unmounting a suspended output cache cannot invalidate its reservation.
pub(crate) fn generated_output_position_lacks_footing(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
) -> bool {
    if !state.world().has_generated_terrain_at(position) {
        return false;
    }
    let supported = position.y().checked_sub(1).is_some_and(|y| {
        let floor = VoxelCoord::new(position.x(), y, position.z());
        (crate::world::WORLD_MIN_Y..crate::world::WORLD_MAX_Y_EXCLUSIVE).contains(&floor.y())
            && !crate::world::fixed_floor_reserved_for_removal(state, floor)
            && (state
                .world()
                .full_cell_collision_at(registries.world(), floor)
                == Some(true)
                || state.structures().blocks_full_cell(floor))
    });
    !supported
}

/// Returns the first durable production job whose retained site would be split if the player's
/// carried stockpile moved to `destination`.
pub(crate) fn player_move_production_site_conflict(
    state: &AppState,
    carried: StockpileId,
    destination: VoxelCoord,
) -> Option<ProductionJobId> {
    state
        .production()
        .retained_output_jobs(carried)
        .find_map(|job| {
            validate_retained_job_site_with_stockpile_override(
                state,
                job,
                Some((carried, destination)),
            )
            .is_err()
            .then_some(job.id())
        })
}
