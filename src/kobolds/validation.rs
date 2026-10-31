//! Trusted-load validation of kobold occupancy, custody, history, and schedules.

use crate::content::FLUID_WATER;
use crate::core::state::AppState;
use crate::core::time::ControlTick;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

use super::{
    FIRST_SCOUT_TICK, FULL_BALANCE_PPM, KoboldColony, KoboldState, SCOUT_DRINK_INTERVAL_TICKS,
    SCOUT_FORAGE_INTERVAL_TICKS, SCOUT_MEAL_INTERVAL_TICKS, SCOUT_MEAL_MASS,
    SCOUT_SAFE_SCAVENGE_TEMPERATURE, SCOUT_STEP_TICKS, SCOUT_WARY_INTERVAL_TICKS,
    scavenging::is_portable_drop, scrap_tools, temple_archivist, temple_guard,
};

/// An opening resident's body cannot coexist with full-cell constructed
/// matter in either world mode. Terrain and vertical-world limits apply only
/// in the resident's generated column, not when an unrelated distant chunk
/// establishes the world's generation profile. Ambient equipment, energy/fluid
/// stores and unenclosed stockpiles remain shareable locality.
fn validate_opening_resident_infrastructure(
    registries: &Registries,
    state: &AppState,
    colony: &KoboldColony,
) -> Result<(), &'static str> {
    for resident in [colony.scout_position(), colony.watch_position()] {
        if resident.y().checked_sub(1).is_some_and(|floor_y| {
            crate::world::full_block_excavation_reserved_at(
                state,
                VoxelCoord::new(resident.x(), floor_y, resident.z()),
            )
        }) {
            return Err("opening kobold occupies an active excavation's reserved floor");
        }
        let head_y = resident
            .y()
            .checked_add(1)
            .ok_or("opening resident body extends outside representable coordinates")?;
        for cell in [
            resident,
            VoxelCoord::new(resident.x(), head_y, resident.z()),
        ] {
            let generated = state.world().has_generated_terrain_at(cell);
            if generated
                && !(crate::world::WORLD_MIN_Y..crate::world::WORLD_MAX_Y_EXCLUSIVE)
                    .contains(&cell.y())
            {
                return Err("opening kobold body extends outside the generated vertical world");
            }
            if generated
                && state.world().block_at(cell).is_none_or(|block| {
                    registries
                        .world()
                        .block_for_state(block)
                        .is_none_or(|definition| {
                            definition.collision() == crate::world::BlockCollisionShape::FullCube
                        })
                })
            {
                return Err("generated terrain intersects an opening kobold resident");
            }
            if crate::kobolds::temple_caste_occupies_voxel(registries, state, cell) {
                return Err("temple caste intersects an opening kobold resident");
            }
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
        || !(next - FIRST_SCOUT_TICK).is_multiple_of(SCOUT_MEAL_INTERVAL_TICKS)
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

fn validate_consumed_fluids(
    registries: &Registries,
    state: &KoboldState,
) -> Result<(), &'static str> {
    if state.consumed_fluids.iter().any(|(fluid, volume)| {
        volume.is_zero()
            || *fluid != FLUID_WATER
            || registries.fluid().get_fluid(*fluid).is_none()
            || !volume
                .microliters()
                .is_multiple_of(u128::from(super::SCOUT_DRINK_VOLUME.microliters()))
    }) {
        return Err("kobold consumed water ledger is invalid");
    }
    Ok(())
}

fn validate_water_schedule(colony: &KoboldColony, state: &AppState) -> Result<(), &'static str> {
    let current = state.tick().value();
    let first_drink = FIRST_SCOUT_TICK + SCOUT_DRINK_INTERVAL_TICKS;
    let ledger_present = !state.kobold_state().consumed_fluids.is_empty();
    let valid_history = match (colony.last_drink_at, colony.last_drink_interval_ticks) {
        (None, None) => !ledger_present && colony.next_drink_at.value() == first_drink,
        (Some(last), Some(interval)) => {
            ledger_present
                && last.value() >= first_drink
                && last.value() <= current
                && (last.value() - FIRST_SCOUT_TICK).is_multiple_of(SCOUT_STEP_TICKS)
                && [
                    SCOUT_DRINK_INTERVAL_TICKS / 2,
                    SCOUT_DRINK_INTERVAL_TICKS * 3 / 4,
                    SCOUT_DRINK_INTERVAL_TICKS,
                    SCOUT_DRINK_INTERVAL_TICKS * 3 / 2,
                ]
                .contains(&interval)
                && last.checked_add_span(crate::core::time::TickSpan::new(interval))
                    == Some(colony.next_drink_at)
        }
        _ => false,
    };
    if colony.next_drink_at.value() < first_drink
        || !(colony.next_drink_at.value() - FIRST_SCOUT_TICK).is_multiple_of(SCOUT_STEP_TICKS)
        || (current < first_drink && colony.next_drink_at.value() != first_drink)
        || (current >= first_drink
            && colony.next_drink_at.value()
                > current.saturating_add(SCOUT_DRINK_INTERVAL_TICKS * 3 / 2))
        || !valid_history
    {
        return Err("kobold water need is not aligned with real scout visits");
    }
    Ok(())
}

fn validate_caution_schedule(colony: &KoboldColony, current: u64) -> Result<(), &'static str> {
    if let Some(wary_until) = colony.wary_until {
        let due = wary_until.value();
        if due < FIRST_SCOUT_TICK + SCOUT_WARY_INTERVAL_TICKS
            || !(due - FIRST_SCOUT_TICK).is_multiple_of(SCOUT_STEP_TICKS)
            || due > current.saturating_add(SCOUT_WARY_INTERVAL_TICKS)
        {
            return Err("kobold caution deadline is not aligned with authoritative time");
        }
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
    validate_consumed_fluids(registries, state.kobold_state())?;
    temple_guard::validate_temple_wardens(registries, state)?;
    temple_archivist::validate_archivists(registries, state)?;
    temple_guard::validate_temple_infrastructure(registries, state)?;
    let Some(colony) = state.kobold_state().colony.as_ref() else {
        return (state.kobold_state().consumed_matter.is_empty()
            && state.kobold_state().consumed_fluids.is_empty())
        .then_some(())
        .ok_or("uninhabited kobold state contains consumed matter or water");
    };
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
    // A corrupted route cursor must be rejected before either resident's
    // position accessor can be called. Those accessors assume a valid route.
    validate_opening_resident_infrastructure(registries, state, colony)?;
    if let Some(resource) = colony.scavenged_drop {
        let Some(drop) = state.surface().get(resource) else {
            return Err("kobold dragged parcel does not exist");
        };
        if colony.scout_step == 0
            || !colony.returning
            || !is_portable_drop(drop)
            || drop.temperature() > SCOUT_SAFE_SCAVENGE_TEMPERATURE
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
    validate_water_schedule(colony, state)?;
    colony.validate_mobilization(state.tick())?;
    scrap_tools::validate_pry_fitting(state, colony, state.tick())?;
    colony.validate_watchman(state.tick())?;
    colony.validate_tracks(state)?;
    colony.validate_watch_tracks(state)?;
    validate_caution_schedule(colony, current)?;
    if let Some(next_forage) = colony.next_forage_at {
        let earliest = FIRST_SCOUT_TICK + SCOUT_STEP_TICKS + colony.forage_interval();
        let forage_due = next_forage.value();
        if forage_due < earliest
            || !(forage_due - FIRST_SCOUT_TICK).is_multiple_of(SCOUT_STEP_TICKS)
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
        || !(due - FIRST_SCOUT_TICK).is_multiple_of(SCOUT_STEP_TICKS)
        || (current < FIRST_SCOUT_TICK && due != FIRST_SCOUT_TICK)
        || (current >= FIRST_SCOUT_TICK && due - current > SCOUT_STEP_TICKS)
    {
        return Err("kobold patrol schedule is not aligned with authoritative time");
    }
    Ok(())
}
