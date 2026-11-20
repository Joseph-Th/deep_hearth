//! Conserved seasonal regrowth of rooted shrubs and fungal colonies.
//!
//! Harvesting exhausts the current fruit; after a growing interval, an
//! established root or mycelium converts some of its *existing* finite
//! underground biomass into another fresh, locally gatherable crop. This is
//! transfer, not a photosynthetic source or loot respawn.

use crate::content::{BLOCK_STATE_STONE, MATERIAL_EDIBLE_FUNGI};
use crate::core::arithmetic::checked_mul_div_with_remainder;
use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::{CalendarDefinition, Season, SimulationTick};
use crate::registry::Registries;
use crate::surface::{SurfaceResourceLifecycle, SurfaceResourceRecord};

use super::generation::{HabitatKind, habitat_at};
use super::sky::{daylight_exposure_after_canopy, has_open_sky, living_canopy_shade_at};
use super::weather::weather_at;

mod moisture;
#[cfg(test)]
use moisture::{BASIN_ROOT_REACH_VOXELS, BASIN_ROOT_VERTICAL_REACH_VOXELS};
use moisture::{RootMoisture, RootMoistureSurvey};

/// One check per regional front, not one vegetation scan per physics step.
const FIELD_CHECK_TICKS: u64 = super::WEATHER_FRONT_TICKS;
/// A ripe wild crop remains available for two days, including before a player
/// discovers its terrain. Unpicked matter then returns to the finite roots.
pub(super) const FRUIT_SENESCENCE_TICKS: u64 = 48_000;
const _: () = assert!(
    FRUIT_SENESCENCE_TICKS.is_multiple_of(FIELD_CHECK_TICKS),
    "first crop senescence must fall on an authoritative vegetation front"
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NaturalForageCycleError {
    RevisionExhausted,
}

/// Optional vegetation changes yield to a scheduled player's surface mutation.
/// Capture this before due work settles, because the completion releases its
/// reservation and must not be undone by incidental senescence on that tick.
pub(crate) fn forage_cycle_deferred_for_surface_reservation(state: &AppState) -> bool {
    state
        .checked_future_surface_revision_demand()
        .is_some_and(|future| future != 0)
        && !state.can_spend_surface_revisions(1)
}

/// Fraction of otherwise available fruit biomass transferred from rooted
/// shrubs or mycelium at this front. These ratios never consume fluid or
/// add material: the source is a finite, previously present underground reserve.
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
fn ripened_crop(
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

/// Mycelium fruits in cool, moist seasons using only its stored organic
/// reserve. Canopy shade increases the yield, unlike leaf photosynthesis.
/// Neither shade nor nearby standing water contributes any new mass.
fn ripened_fungal_crop(
    root_reserve: Mass,
    original_fruit: Mass,
    moisture: RootMoisture,
    shade: u8,
) -> Mass {
    let wet_crop = moisture_adjusted_crop((original_fruit.milligrams() / 4).max(1), moisture);
    let shade_multiplier_ppm = match shade {
        0 => 600_000_u128,
        1..=3 => 1_000_000,
        _ => 1_250_000,
    };
    let fruit = u64::try_from(u128::from(wet_crop) * shade_multiplier_ppm / 1_000_000)
        .expect("bounded fungal yield fits u64");
    Mass::from_milligrams(root_reserve.milligrams().min(fruit))
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

/// Resolve the real ecological gates before surveying nearby finite water.
/// Mushrooms may fruit below a canopy or roof, but still require rooted
/// substrate, cool temperatures, an appropriate season and an intact site.
fn ripening_site_shade(
    registries: &Registries,
    state: &AppState,
    source: &SurfaceResourceRecord,
    seed: super::WorldGenerationSeed,
    season: Season,
    at: SimulationTick,
) -> Option<u8> {
    let fungal = source.commodity().material() == MATERIAL_EDIBLE_FUNGI;
    let season_allows = if fungal {
        matches!(season, Season::Spring | Season::Autumn)
    } else {
        matches!(season, Season::Spring | Season::Summer)
    };
    if source.lifecycle() != SurfaceResourceLifecycle::Depleted
        || !season_allows
        || !state.world().has_generated_terrain_at(source.position())
    {
        return None;
    }
    let habitat = habitat_at(seed, source.position().x(), source.position().z());
    let recovery = if fungal {
        match habitat {
            HabitatKind::DampGrove => 800,
            HabitatKind::MixedWoodland => 1_400,
            HabitatKind::DryScrub => 2_400,
        }
    } else {
        recovery_ticks(habitat)
    };
    if at.value().checked_sub(source.depleted_at()?.value())? < recovery {
        return None;
    }
    let temperature = weather_at(registries, seed.value(), source.position(), at)
        .ambient_temperature()
        .millikelvin();
    if !(if fungal {
        (280_150..=299_150).contains(&temperature)
    } else {
        (283_150..=303_150).contains(&temperature)
    }) {
        return None;
    }
    let at_floor = crate::spatial::VoxelCoord::new(
        source.position().x(),
        source.position().y().checked_sub(1)?,
        source.position().z(),
    );
    if state.world().block_at(at_floor) != Some(BLOCK_STATE_STONE)
        || state.structures().blocks_full_cell(source.position())
        || state.has_enclosed_stockpile_at(source.position())
        || (!fungal && !has_open_sky(registries, state, source.position()))
    {
        return None;
    }
    let shade = living_canopy_shade_at(registries, state, source.position(), at);
    (fungal || daylight_exposure_after_canopy(registries.core().calendar(), shade) > 0)
        .then_some(shade)
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
    if forage_cycle_deferred_for_surface_reservation(state) {
        return Ok(());
    }
    let season = registries.core().calendar().season_at(at);
    let Some(profile) = state.world().generation_profile() else {
        return Ok(());
    };
    // Only survey springs if a depleted rooted crop actually reaches a viable
    // ripening decision. Senescence, winter, and barren districts need no
    // moisture scan or allocation at this weather front.
    let moisture = std::cell::OnceCell::new();
    let changes = state
        .surface()
        .perennial_resources()
        .filter_map(|source| {
            let mut plant = source.perennial?;
            let fungal = source.commodity().material() == MATERIAL_EDIBLE_FUNGI;
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
            if plant.root_reserve.is_zero() {
                return None;
            }
            // Region, soil, weather, roof and real canopy constraints are
            // checked before any regional spring-moisture survey is allocated.
            let shade = ripening_site_shade(registries, state, source, profile.seed(), season, at)?;
            let moisture = moisture.get_or_init(|| RootMoistureSurvey::observe(state));
            let batch = if fungal {
                ripened_fungal_crop(
                    plant.root_reserve,
                    plant.original_fruit,
                    moisture.at(source.position()),
                    shade,
                )
            } else {
                ripened_crop(
                    registries.core().calendar(),
                    plant.root_reserve,
                    plant.original_fruit,
                    moisture.at(source.position()),
                    shade,
                )
            };
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

#[cfg(test)]
#[path = "fungal_cycle_tests.rs"]
mod fungal_tests;
