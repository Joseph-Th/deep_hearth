//! Spatial coherence for production endpoints whose world locations are already authoritative.

use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::energy::EnergyStoreId;
use crate::equipment::EquipmentId;
use crate::inventory::StockpileId;
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

/// Enumerate only endpoints retained for the lifetime of a job. Inputs already
/// consumed at admission no longer anchor the job to their original storage.
fn retained_endpoints(
    job: &ProductionJobRecord,
) -> impl Iterator<Item = ProductionSiteEndpoint> + '_ {
    job.equipment_provider()
        .map(|provider| ProductionSiteEndpoint::Equipment(provider.equipment()))
        .into_iter()
        .chain(
            job.released_energy()
                .map(|released| ProductionSiteEndpoint::EnergyStore(released.destination())),
        )
        .chain(
            job.output_streams()
                .iter()
                .map(|stream| ProductionSiteEndpoint::Destination(stream.destination())),
        )
}

impl ProductionSiteEndpoint {
    fn position(self, state: &AppState) -> Option<VoxelCoord> {
        match self {
            Self::Source(id) | Self::Destination(id) => state.logistics().stockpile_position(id),
            Self::Equipment(id) => state.logistics().equipment_position(id),
            Self::EnergyStore(id) => state.logistics().energy_store_position(id),
        }
    }

    fn is_carried(self, state: &AppState) -> bool {
        match self {
            Self::Source(id) | Self::Destination(id) => state
                .logistics()
                .player()
                .is_some_and(|player| player.carried_stockpile() == id),
            Self::Equipment(id) => state.logistics().is_equipment_carried(id),
            Self::EnergyStore(_) => false,
        }
    }
}

/// Suspension retains the same physical obligations as active processing.
pub(crate) fn validate_retained_job_site(
    state: &AppState,
    job: &ProductionJobRecord,
) -> Result<(), ProductionSiteMismatch> {
    let mut anchor = None;
    for endpoint in retained_endpoints(job) {
        bind_known_endpoint(&mut anchor, endpoint, endpoint.position(state))?;
    }
    Ok(())
}

/// A job whose retained endpoints include both carried custody and a located
/// stationary resource needs to keep the player at that shared site.
pub(crate) fn retained_player_anchor(
    state: &AppState,
    job: &ProductionJobRecord,
) -> Option<VoxelCoord> {
    let mut carried = false;
    let mut stationary = None;
    for endpoint in retained_endpoints(job) {
        if endpoint.is_carried(state) {
            carried = true;
        } else if let Some(position) = endpoint.position(state) {
            stationary = Some(position);
        }
    }
    if carried { stationary } else { None }
}

/// Find the lowest-ID job anchoring the player using existing occupancy indexes.
pub(crate) fn player_retained_production_anchor(
    state: &AppState,
) -> Option<(ProductionJobId, VoxelCoord)> {
    let carried = state.logistics().player()?.carried_stockpile();
    state
        .production()
        .retained_output_jobs(carried)
        .chain(
            state
                .logistics()
                .carried_equipment()
                .filter_map(|id| state.production().get_equipment_occupant(id)),
        )
        .filter_map(|job| retained_player_anchor(state, job).map(|position| (job.id(), position)))
        .min_by_key(|(job, _)| *job)
}
