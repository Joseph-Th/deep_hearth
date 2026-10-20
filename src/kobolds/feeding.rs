//! Sparse resident feeding from recoverable den custody into terminal matter accounting.

use crate::core::quantity::{AggregateMass, Mass};
use crate::core::state::AppState;
use crate::core::time::{SimulationTick, TickSpan};
use crate::inventory::AMBIENT_PRESERVATION_MULTIPLIER_PPM;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::{SurfaceResourceId, SurfaceResourceKind};
use crate::survival::{FoodFreshness, freshness_from_history};

use super::{SCOUT_MEAL_INTERVAL_TICKS, SCOUT_MEAL_MASS};

/// Advance the sparse feeding clock even when the scout is away from home or
/// frightened. Missing a meal does not synthesize resources or backlog work.
pub(super) fn apply_scheduled_den_meal(
    registries: &Registries,
    state: &mut AppState,
    from: VoxelCoord,
    at: SimulationTick,
    pressured: bool,
) -> Option<(SurfaceResourceId, Mass)> {
    let colony = state.kobold_state().colony.as_ref()?;
    if at < colony.next_meal_at {
        return None;
    }
    let at_den = colony.scout_step == 0;
    let eaten = (at_den && !pressured)
        .then(|| consume_den_meal(registries, state, from, at))
        .flatten();
    state
        .kobold_state_mut()
        .colony
        .as_mut()
        .expect("resident feeding requires existing colony")
        .next_meal_at = at
        .checked_add_span(TickSpan::new(SCOUT_MEAL_INTERVAL_TICKS))
        .expect("bounded simulation cannot overflow meal schedule");
    eaten
}

/// Deplete one fresh, physically local food parcel and retain the exact mass
/// in the colony's terminal consumption ledger. Both writes are prechecked.
pub(super) fn consume_den_meal(
    registries: &Registries,
    state: &mut AppState,
    den: VoxelCoord,
    at: SimulationTick,
) -> Option<(SurfaceResourceId, Mass)> {
    if !state.can_spend_surface_revisions(1) {
        return None;
    }
    let (source, material, mass, consumed_after) = state
        .surface()
        .available_resources_at(Some(den))
        .find_map(|resource| {
            if resource.kind() != SurfaceResourceKind::Drop {
                return None;
            }
            let food = registries.survival().get_food(resource.commodity())?;
            if resource.composition().pure_material() != Some(resource.commodity().material())
                || !food
                    .consumption_temperature()
                    .contains(resource.temperature())
                || !matches!(
                    freshness_from_history(
                        resource.ambient_storage_history_at(at)?,
                        AMBIENT_PRESERVATION_MULTIPLIER_PPM,
                        at,
                        food.shelf_life()
                    ),
                    Ok(FoodFreshness::Fresh { .. })
                )
            {
                return None;
            }
            let mass = resource.remaining_mass().min(SCOUT_MEAL_MASS);
            let material = resource.commodity().material();
            let consumed_after = state
                .kobold_state()
                .consumed_matter
                .get(&material)
                .copied()
                .unwrap_or(AggregateMass::ZERO)
                .checked_add(AggregateMass::from_mass(mass))?;
            Some((resource.id(), material, mass, consumed_after))
        })?;
    let next_revision = state
        .surface()
        .revision()
        .checked_add(1)
        .expect("feeding checked surface and future revision headroom");
    state
        .surface_state_mut()
        .apply_gathering(source, mass, at, next_revision);
    state
        .kobold_state_mut()
        .apply_meal(material, consumed_after);
    Some((source, mass))
}
