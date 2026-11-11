//! Sparse regrowth of previously stripped perennial berry and nut patches.
//!
//! Harvesting exhausts the current fruit; after a growing interval, the
//! rooted plant converts some of its *existing* finite underground biomass
//! into another fresh, locally gatherable crop. This is transfer, not a
//! photosynthetic source or loot respawn.

use crate::content::BLOCK_STATE_STONE;
use crate::core::arithmetic::checked_mul_div_with_remainder;
use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::{CalendarDefinition, Season, SimulationTick};
use crate::registry::Registries;
use crate::surface::SurfaceResourceLifecycle;

use super::generation::{HabitatKind, habitat_at};
use super::sky::{daylight_exposure_after_canopy, has_open_sky, living_canopy_shade_at};
use super::weather::weather_at;

mod moisture;
#[cfg(test)]
use moisture::{BASIN_ROOT_REACH_VOXELS, BASIN_ROOT_VERTICAL_REACH_VOXELS};
use moisture::{RootMoisture, RootMoistureSurvey};

/// One check per regional front, not one vegetation scan per physics step.
const FIELD_CHECK_TICKS: u64 = super::WEATHER_FRONT_TICKS;
/// Unpicked crop material eventually returns to the plant's finite nutrient
/// reserve, rather than staying fresh indefinitely or vanishing from custody.
pub(super) const FRUIT_SENESCENCE_TICKS: u64 = 2_400;
const _: () = assert!(
    FRUIT_SENESCENCE_TICKS.is_multiple_of(FIELD_CHECK_TICKS),
    "first crop senescence must fall on an authoritative vegetation front"
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NaturalForageCycleError {
    RevisionExhausted,
}
/// Fraction of the otherwise available fruit biomass that one rooted shrub
/// may transfer at this weather front. These ratios never consume fluid or
/// add material: the source is a fixed, previously present underground reserve.
fn moisture_adjusted_crop(open_crop: u64, moisture: RootMoisture) -> u64 {
    match moisture {
        RootMoisture::NoBasin => open_crop,
        RootMoisture::Basin { fraction_ppm } => {
            // Empty natural water custody provides no additional moisture,
            // just like having no basin. Retained water can increase the
            // otherwise available fruit by up to 25% without being consumed.
            let multiplier_ppm = 1_000_000 + u64::from(fraction_ppm) / 4;
            u64::try_from(u128::from(open_crop) * u128::from(multiplier_ppm) / 1_000_000)
                .expect("a quarter-crop with at most 25% moisture gain fits u64")
        }
    }
}

/// Ripening transfers existing biomass only when light supports a whole-milligram
/// crop. Dense overlapping crowns and fractional yields retain the root reserve.
fn ripened_crop_for_calendar(
    calendar: CalendarDefinition,
    root_reserve: Mass,
    original_fruit: Mass,
    moisture: RootMoisture,
    shade: u8,
) -> Mass {
    let daylight = daylight_exposure_after_canopy(calendar, shade);
    if daylight == 0 || root_reserve.is_zero() {
        return Mass::ZERO;
    }
    let open_batch = (original_fruit.milligrams() / 4).max(1);
    let watered_batch = moisture_adjusted_crop(open_batch, moisture);
    let (lit_batch, _) = checked_mul_div_with_remainder(
        u128::from(watered_batch),
        daylight,
        daylight_exposure_after_canopy(calendar, 0),
        0,
    )
    .expect("a bounded crop scaled by available daylight fits u128");
    let lit_batch =
        u64::try_from(lit_batch).expect("daylight cannot increase a moisture-adjusted crop");
    Mass::from_milligrams(root_reserve.milligrams().min(lit_batch))
}

/// Existing vegetation unit tests evaluate the authored day; runtime decisions
/// always provide their actual calendar to the same crop formula.
#[cfg(test)]
fn ripened_crop(
    root_reserve: Mass,
    original_fruit: Mass,
    moisture: RootMoisture,
    shade: u8,
) -> Mass {
    ripened_crop_for_calendar(
        crate::content::build_registries().core().calendar(),
        root_reserve,
        original_fruit,
        moisture,
        shade,
    )
}

/// A stripped plant needs real elapsed growth time; drought-tolerant scrub
/// has a longer cycle than the sheltered, moist grove.
fn recovery_ticks(habitat: HabitatKind) -> u64 {
    match habitat {
        HabitatKind::DampGrove => 800,
        HabitatKind::MixedWoodland => 1_200,
        HabitatKind::DryScrub => 2_400,
    }
}

pub(crate) fn apply_natural_forage_cycle(
    registries: &Registries,
    state: &mut AppState,
    at: SimulationTick,
) -> Result<(), NaturalForageCycleError> {
    if !at.value().is_multiple_of(FIELD_CHECK_TICKS) {
        return Ok(());
    }
    // Growth and senescence are incidental. A scheduled harvest owns its future
    // surface revision even when it completes after this weather front. Defer
    // vegetation changes rather than consuming that promised mutation space.
    // With no outstanding reservation, a genuinely exhausted revision still
    // reports an error instead of silently suppressing an otherwise due cycle.
    if state
        .checked_future_surface_revision_demand()
        .is_some_and(|future| future != 0)
        && !state.can_spend_surface_revisions(1)
    {
        return Ok(());
    }
    let growing_season = matches!(
        registries.core().calendar().season_at(at),
        Season::Spring | Season::Summer
    );
    let Some(profile) = state.world().generation_profile() else {
        return Ok(());
    };
    // Only survey springs if a depleted shrub actually reaches a viable
    // ripening decision. Senescence, winter, and barren districts need no
    // moisture scan or allocation at this weather front.
    let moisture = std::cell::OnceCell::new();
    let changes = state
        .surface()
        .perennial_resources()
        .filter_map(|source| {
            let mut plant = source.perennial?;
            if source.lifecycle() == SurfaceResourceLifecycle::Available {
                let being_gathered = state
                    .player_work()
                    .active()
                    .and_then(|work| work.surface_gathering())
                    .is_some_and(|work| work.resource() == source.id());
                if being_gathered
                    || at.value().saturating_sub(plant.ripened_at.value()) < FRUIT_SENESCENCE_TICKS
                {
                    return None;
                }
                plant.root_reserve = plant.root_reserve.checked_add(source.remaining_mass())?;
                return Some((
                    source.id(),
                    plant,
                    Mass::ZERO,
                    SurfaceResourceLifecycle::Depleted,
                    Some(at),
                ));
            }
            if source.lifecycle() != SurfaceResourceLifecycle::Depleted
                || !growing_season
                || plant.root_reserve.is_zero()
                || !state.world().has_generated_terrain_at(source.position())
            {
                return None;
            }
            // Age and temperature are cheap, independent admission facts.
            // A depleted plant may spend many fronts recovering; do not trace
            // an entire sky column and inspect constructed custody until it
            // could actually ripen on this front.
            let habitat = habitat_at(profile.seed(), source.position().x(), source.position().z());
            if at.value().checked_sub(source.depleted_at()?.value())? < recovery_ticks(habitat) {
                return None;
            }
            let weather = weather_at(registries, profile.seed().value(), source.position(), at);
            // Frozen roots do not fruit; prolonged scorching also arrests
            // this temperate shrub. The actual local weather controls yield.
            if !(283_150..=303_150).contains(&weather.ambient_temperature().millikelvin()) {
                return None;
            }
            let floor = source.position().y().checked_sub(1)?;
            if state.world().block_at(crate::spatial::VoxelCoord::new(
                source.position().x(),
                floor,
                source.position().z(),
            )) != Some(BLOCK_STATE_STONE)
            {
                // Mining the rooted stone footing is a durable way to kill
                // the patch's recovery without erasing its old matter. Check
                // this cheap world cell before tracing a full sky column.
                return None;
            }
            if state.structures().blocks_full_cell(source.position())
                // An enclosure is inventory-owned physical volume, not a structural
                // member. It cannot hide a rooted patch and still admit fresh fruit.
                || state.has_enclosed_stockpile_at(source.position())
                || !has_open_sky(registries, state, source.position())
            {
                return None;
            }
            // The physical crown of an older neighboring tree intercepts
            // direct sunlight from the understory shrub. Smaller crops
            // leave their unspent mass in its actual root reserve; no water,
            // organic matter or food is silently removed or manufactured.
            let shade = living_canopy_shade_at(registries, state, source.position(), at);
            if daylight_exposure_after_canopy(registries.core().calendar(), shade) == 0 {
                return None;
            }
            let moisture = moisture.get_or_init(|| RootMoistureSurvey::observe(state));
            let batch = ripened_crop_for_calendar(
                registries.core().calendar(),
                plant.root_reserve,
                plant.original_fruit,
                moisture.at(source.position()),
                shade,
            );
            if batch.is_zero() {
                return None;
            }
            plant.root_reserve = plant.root_reserve.checked_sub(batch)?;
            plant.ripened_at = at;
            Some((
                source.id(),
                plant,
                batch,
                SurfaceResourceLifecycle::Available,
                None,
            ))
        })
        .collect::<Vec<_>>();
    if !changes.is_empty() {
        let next_revision = state
            .surface()
            .revision()
            .checked_add(1)
            .ok_or(NaturalForageCycleError::RevisionExhausted)?;
        state
            .surface_state_mut()
            .apply_perennial_cycle(changes, next_revision);
    }
    Ok(())
}

#[cfg(test)]
#[path = "forage_cycle_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "forage_moisture_tests.rs"]
mod moisture_tests;

#[cfg(test)]
#[path = "forage_daylight_tests.rs"]
mod daylight_tests;
