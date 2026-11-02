//! Ground-storage raid admission and exact finite theft on a scout work slot.

use super::*;

/// An industrial raid needs both real capacity and a resident fit to work.
fn planned_ground_raid(
    registries: &Registries,
    state: &AppState,
    from: VoxelCoord,
    at: SimulationTick,
    forage_ready: bool,
    optional_work_blocked: bool,
) -> Option<(
    crate::inventory::StockpileId,
    ScavengedGroundStockpileRequest,
)> {
    let colony = state.kobold_state().colony.as_ref()?;
    let space = den_cache_space(state, colony.route[0]);
    if colony.mobilized_at.is_none()
        || colony.scout_step == 0
        || colony.returning
        || colony.scavenged_drop.is_some()
        || colony.hungry
        || optional_work_blocked
        || space.is_zero()
        || has_local_scout_interest(registries, state, from, space, forage_ready, at, false)
    {
        return None;
    }
    let (stockpile, lot) = local_unenclosed_stockpile_target(registries, state, from, at)?;
    Some((
        stockpile,
        ScavengedGroundStockpileRequest::new(
            stockpile,
            lot,
            from,
            at,
            colony.salvage_batch().min(space),
            SCOUT_SAFE_SCAVENGE_TEMPERATURE,
        ),
    ))
}

/// One canonical extraction occupies the scout's work slot and settles
/// finite stockpile custody, never material created from a target sighting.
pub(super) fn execute_ground_raid(
    registries: &Registries,
    state: &mut AppState,
    from: VoxelCoord,
    at: SimulationTick,
    forage_ready: bool,
    blocked: bool,
) -> Option<(crate::inventory::StockpileId, Mass)> {
    let (stockpile, request) =
        planned_ground_raid(registries, state, from, at, forage_ready, blocked)?;
    let (_, stolen) = scavenge_unenclosed_ground_stockpile_material(registries, state, request)?;
    state
        .kobold_state_mut()
        .colony
        .as_mut()
        .expect("ground raid retains its resident scout")
        .next_step_at = at
        .checked_add_span(TickSpan::new(SCOUT_STEP_TICKS))
        .expect("bounded ground raid cannot overflow its scout clock");
    Some((stockpile, stolen))
}
