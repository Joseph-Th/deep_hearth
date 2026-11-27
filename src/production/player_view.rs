//! Actor-visible production schedules at currently accessible output custody.

use std::collections::BTreeMap;

use crate::core::state::AppState;

use super::{ProcessId, ProductionJobId, ProductionOccupancyRelease};

/// Read-only schedule for one job retaining output at the player's exact locality.
/// No remote endpoint identities, input traces, or global job enumeration are exposed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerProductionJobProjection {
    pub job: ProductionJobId,
    pub process: ProcessId,
    pub release: ProductionOccupancyRelease,
}

/// Observes jobs through carried and exact-local output reservations, including suspended
/// jobs. A job routing several streams to local custody appears once in stable ID order.
pub fn project_player_local_production_jobs(
    state: &AppState,
) -> Vec<PlayerProductionJobProjection> {
    if state.player_observation_position().is_none() {
        return Vec::new();
    }
    let carried = state
        .logistics()
        .player()
        .expect("live observation requires player logistics")
        .carried_stockpile();
    let local_outputs = std::iter::once(carried).chain(
        state
            .available_local_ground_stockpiles()
            .map(|record| record.id()),
    );
    local_outputs
        .flat_map(|stockpile| state.production().retained_output_jobs(stockpile))
        .map(|job| {
            (
                job.id(),
                PlayerProductionJobProjection {
                    job: job.id(),
                    process: job.process(),
                    release: job.occupancy_release(),
                },
            )
        })
        .collect::<BTreeMap<_, _>>()
        .into_values()
        .collect()
}
