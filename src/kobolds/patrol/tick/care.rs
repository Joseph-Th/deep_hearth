//! Due scout survival care and alarm relay.
//!
//! Care settles before movement and cargo, so later work-slot decisions cannot
//! erase a real drink, meal, disposal, or witnessed source event.

use super::*;
use crate::core::quantity::Volume;
use crate::fluid::FluidStoreId;

pub(super) struct ScoutVisitCare {
    pub(super) drank_water: Option<(FluidStoreId, Volume)>,
    pub(super) discarded_food: Option<SurfaceResourceId>,
    pub(super) cleared_cache: Option<SurfaceResourceId>,
    pub(super) observed_care: KoboldScoutEvent,
    pub(super) heard_junk_alarm: bool,
    pub(super) alarm_pulses: Vec<KoboldAlarmPulse>,
    pub(super) water_deprived: bool,
}

pub(super) fn settle_scout_visit_care(
    registries: &Registries,
    state: &mut AppState,
    from: VoxelCoord,
    at: SimulationTick,
    observer: Option<VoxelCoord>,
    pressured: bool,
) -> ScoutVisitCare {
    let drank_water = (!pressured)
        .then(|| crate::kobolds::hydration::drink_on_scout_visit(registries, state, from, at))
        .flatten();
    // Den sources lose matter only through their actual owner operations.
    let (consumed_food, discarded_food, cleared_cache) =
        apply_scheduled_den_care(registries, state, from, at, pressured);
    let water_deprived = crate::kobolds::hydration::prolonged_water_deprivation(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap_or_else(|| unreachable!("active scout exists")),
        at,
    );
    let (heard_junk_alarm, alarm_pulses) = apply_watchman_tick(registries, state, at);
    // Preserve the witnessed source even if the scout later moves out of sight.
    let care_visible = observer.is_some_and(|player| {
        crate::world::player_can_see_local_cell_at(registries, state, player, from, at)
    });
    // Discard and cache clearing release matter into a neighboring cell. Seeing
    // care at the den cannot reveal the ID of a parcel now outside sight.
    // Read the moved record, since oversized cargo may have a new split ID.
    let released_parcel_visible = |id: &SurfaceResourceId| {
        care_visible
            && observer.is_some_and(|player| {
                state.surface().get(*id).is_some_and(|record| {
                    crate::world::player_can_see_local_cell_at(
                        registries,
                        state,
                        player,
                        record.position(),
                        at,
                    )
                })
            })
    };
    let observed_care = KoboldScoutEvent {
        consumed_food: consumed_food.filter(|_| care_visible),
        drank_water: drank_water.filter(|_| care_visible),
        discarded_food: discarded_food.filter(released_parcel_visible),
        cleared_cache: cleared_cache.filter(released_parcel_visible),
        ..KoboldScoutEvent::quiet_at(from)
    };
    ScoutVisitCare {
        drank_water,
        discarded_food,
        cleared_cache,
        observed_care,
        heard_junk_alarm,
        alarm_pulses,
        water_deprived,
    }
}
