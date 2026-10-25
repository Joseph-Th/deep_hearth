//! Trusted-load validation of kobold occupancy, custody, history, and schedules.

use crate::core::state::AppState;
use crate::core::time::ControlTick;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

use super::{
    FIRST_SCOUT_TICK, FULL_BALANCE_PPM, KoboldColony, KoboldState, SCOUT_FORAGE_INTERVAL_TICKS,
    SCOUT_MEAL_INTERVAL_TICKS, SCOUT_MEAL_MASS, SCOUT_STEP_TICKS, SCOUT_WARY_INTERVAL_TICKS,
    scavenging::is_portable_drop, scrap_tools, temple_archivist, temple_guard,
};

/// An opening resident's body cannot coexist with constructed matter in a
/// generated world. Ambient equipment, energy/fluid stores and unenclosed
/// stockpiles remain shareable locality rather than full-cube collision.
fn validate_opening_resident_infrastructure(
    state: &AppState,
    colony: &KoboldColony,
) -> Result<(), &'static str> {
    if state.world().generation_profile().is_none() {
        return Ok(());
    }
    for resident in [colony.scout_position(), colony.watch_position()] {
        let head_y = resident
            .y()
            .checked_add(1)
            .ok_or("opening resident body extends outside representable coordinates")?;
        for cell in [
            resident,
            VoxelCoord::new(resident.x(), head_y, resident.z()),
        ] {
            let enclosed = state.logistics().stockpiles_at(cell).any(|id| {
                state
                    .inventory()
                    .get_stockpile(id)
                    .is_some_and(|stockpile| stockpile.enclosure().is_some())
            });
            if enclosed || state.structures().blocks_full_cell(cell) {
                return Err("constructed infrastructure intersects an opening kobold resident");
            }
        }
    }
    Ok(())
}

fn validate_meal_schedule(colony: &KoboldColony, current: u64) -> Result<(), &'static str> {
    let next = colony.next_meal_at.value();
    let first = FIRST_SCOUT_TICK + SCOUT_MEAL_INTERVAL_TICKS;
    if next < first
        || (next - FIRST_SCOUT_TICK) % SCOUT_MEAL_INTERVAL_TICKS != 0
        || next <= current
        || (current < first && next != first)
        || (current >= first && next - current > SCOUT_MEAL_INTERVAL_TICKS)
    {
        return Err("kobold meal schedule is not aligned with authoritative time");
    }
    if colony.hungry && current < first {
        return Err("kobold hunger cannot precede its first meal opportunity");
    }
    if colony.partial_meal >= SCOUT_MEAL_MASS || (!colony.hungry && !colony.partial_meal.is_zero())
    {
        return Err("kobold partial meal is not a valid outstanding ration");
    }
    Ok(())
}

fn validate_consumed_matter(
    registries: &Registries,
    state: &KoboldState,
) -> Result<(), &'static str> {
    if state.consumed_matter.values().any(|mass| mass.is_zero())
        || state
            .consumed_matter
            .keys()
            .any(|material| registries.materials().get_material(*material).is_none())
    {
        return Err("kobold consumption ledger has invalid materials or zero mass");
    }
    Ok(())
}

fn validate_contact_history(
    colony: &KoboldColony,
    current: ControlTick,
    combat: &crate::combat::CombatState,
) -> Result<(), &'static str> {
    if colony.scout_balance_ppm > FULL_BALANCE_PPM || colony.watch_balance_ppm > FULL_BALANCE_PPM {
        return Err("kobold balance exceeds stable footing");
    }
    if (colony.scout_balance_ppm < FULL_BALANCE_PPM && colony.scout_shoved_at.is_none())
        || (colony.watch_balance_ppm < FULL_BALANCE_PPM && colony.watch_shoved_at.is_none())
    {
        return Err("kobold lost balance without recorded local impact");
    }
    if [colony.scout_shoved_at, colony.watch_shoved_at]
        .into_iter()
        .flatten()
        .any(|at| at > current)
    {
        return Err("kobold contact record originates in the future");
    }
    if [colony.scout_shoved_at, colony.watch_shoved_at]
        .into_iter()
        .flatten()
        .any(|at| !combat.admits_contact_at(at))
    {
        return Err("kobold impact record lacks authoritative player combat history");
    }
    Ok(())
}

/// Full save-admission validation of persistent colony topology and patrol continuation.
pub(crate) fn validate_loaded_kobolds(
    registries: &Registries,
    state: &AppState,
) -> Result<(), &'static str> {
    validate_consumed_matter(registries, state.kobold_state())?;
    temple_guard::validate_temple_wardens(registries, state)?;
    temple_archivist::validate_archivists(registries, state)?;
    temple_guard::validate_temple_infrastructure(registries, state)?;
    let Some(colony) = state.kobold_state().colony.as_ref() else {
        return state
            .kobold_state()
            .consumed_matter
            .is_empty()
            .then_some(())
            .ok_or("uninhabited kobold state contains consumed matter");
    };
    validate_opening_resident_infrastructure(state, colony)?;
    validate_contact_history(colony, state.control_tick(), state.combat())?;
    if colony.scout_step > 3
        || (colony.scout_step == 3 && (colony.returning || colony.detour_goal.is_none()))
        || !colony.route[0].is_face_adjacent_to(colony.route[1])
        || !colony.route[1].is_face_adjacent_to(colony.route[2])
        || colony.route[0] == colony.route[2]
    {
        return Err("invalid kobold patrol route or cursor");
    }
    colony.validate_detour(state)?;
    if let Some(resource) = colony.scavenged_drop {
        let Some(drop) = state.surface().get(resource) else {
            return Err("kobold dragged parcel does not exist");
        };
        if colony.scout_step == 0
            || !colony.returning
            || !is_portable_drop(drop)
            || drop.position() != colony.scout_position()
        {
            return Err("kobold dragged parcel has invalid route custody");
        }
        // Depleted custody is legitimate when the player intercepted the parcel
        // since the last scout step; the next tick clears the stale carry reference.
    }
    if !state
        .logistics()
        .has_traversal_edge(colony.route[0], colony.route[1])
        || !state
            .logistics()
            .has_traversal_edge(colony.route[1], colony.route[2])
    {
        return Err("kobold patrol lacks authoritative wilderness traversal edges");
    }
    let current = state.tick().value();
    validate_meal_schedule(colony, current)?;
    colony.validate_mobilization(state.tick())?;
    scrap_tools::validate_pry_fitting(state, colony, state.tick())?;
    colony.validate_watchman(state.tick())?;
    colony.validate_tracks(state)?;
    colony.validate_watch_tracks(state)?;
    if let Some(wary_until) = colony.wary_until {
        let due = wary_until.value();
        if due < FIRST_SCOUT_TICK + SCOUT_WARY_INTERVAL_TICKS
            || (due - FIRST_SCOUT_TICK) % SCOUT_STEP_TICKS != 0
            || due > current.saturating_add(SCOUT_WARY_INTERVAL_TICKS)
        {
            return Err("kobold caution deadline is not aligned with authoritative time");
        }
    }
    if let Some(next_forage) = colony.next_forage_at {
        let earliest = FIRST_SCOUT_TICK + SCOUT_STEP_TICKS + colony.forage_interval();
        let forage_due = next_forage.value();
        if forage_due < earliest
            || (forage_due - FIRST_SCOUT_TICK) % SCOUT_STEP_TICKS != 0
            || forage_due > current.saturating_add(SCOUT_FORAGE_INTERVAL_TICKS)
        {
            return Err("kobold forage interval is not aligned with authoritative time");
        }
    }
    let due = colony.next_step_at.value();
    if due <= current {
        return Err("kobold scout step is overdue");
    }
    if due < FIRST_SCOUT_TICK
        || (due - FIRST_SCOUT_TICK) % SCOUT_STEP_TICKS != 0
        || (current < FIRST_SCOUT_TICK && due != FIRST_SCOUT_TICK)
        || (current >= FIRST_SCOUT_TICK && due - current > SCOUT_STEP_TICKS)
    {
        return Err("kobold patrol schedule is not aligned with authoritative time");
    }
    Ok(())
}
