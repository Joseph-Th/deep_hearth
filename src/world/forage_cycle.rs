//! Sparse regrowth of previously stripped perennial berry and nut patches.
//!
//! Harvesting exhausts the current fruit; after a growing interval, the
//! rooted plant converts some of its *existing* finite underground biomass
//! into another fresh, locally gatherable crop. This is transfer, not a
//! photosynthetic source or loot respawn.

use crate::content::{BLOCK_STATE_STONE, FLUID_WATER};
use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::{Season, SimulationTick};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::SurfaceResourceLifecycle;

use super::generation::{HabitatKind, habitat_at};
use super::sky::{has_open_sky, living_canopy_shade_at};
use super::weather::weather_at;

/// One check per regional front, not one vegetation scan per physics step.
const FIELD_CHECK_TICKS: u64 = super::WEATHER_FRONT_TICKS;
/// Unpicked crop material eventually returns to the plant's finite nutrient
/// reserve, rather than staying fresh indefinitely or vanishing from custody.
const FRUIT_SENESCENCE_TICKS: u64 = 2_400;
/// Shallow basin moisture influences nearby roots, not an entire region or a
/// distant aquifer. No permeability or nutrient transport model is claimed.
const BASIN_ROOT_REACH_VOXELS: u64 = 12;
const BASIN_ROOT_VERTICAL_REACH_VOXELS: u64 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RootMoisture {
    NoBasin,
    /// Finite natural source volume as a fraction of the original basin size.
    Basin {
        fraction_ppm: u32,
    },
}

/// A naturally generated source has distinct finite surface and subsurface
/// custody. A dry-looking pool with water still in its own shallow catchment
/// remains moist; an empty bucket elsewhere is not a natural aquifer.
fn root_moisture_at(state: &AppState, position: VoxelCoord) -> RootMoisture {
    let mut wettest: Option<u32> = None;
    for (id, soil) in state.fluid().spring_soil() {
        if position.x().abs_diff(soil.position.x()) > BASIN_ROOT_REACH_VOXELS
            || position.z().abs_diff(soil.position.z()) > BASIN_ROOT_REACH_VOXELS
            || position.y().abs_diff(soil.position.y()) > BASIN_ROOT_VERTICAL_REACH_VOXELS
            || state.logistics().fluid_store_position(id) != Some(soil.position)
        {
            continue;
        }
        let Some(pool) = state.fluid().get_store(id) else {
            continue;
        };
        let standing_water = pool
            .contents()
            .filter(|liquid| liquid.fluid() == FLUID_WATER);
        let retained = standing_water
            .map_or(0, |liquid| u128::from(liquid.volume().microliters()))
            .saturating_add(u128::from(soil.soaked.microliters()))
            .min(u128::from(pool.capacity().microliters()));
        let fraction_ppm =
            u32::try_from(retained * 1_000_000 / u128::from(pool.capacity().microliters()))
                .expect("bounded natural water ratio fits ppm");
        wettest = Some(wettest.map_or(fraction_ppm, |previous| previous.max(fraction_ppm)));
    }
    wettest.map_or(RootMoisture::NoBasin, |fraction_ppm| RootMoisture::Basin {
        fraction_ppm,
    })
}

/// Fraction of the otherwise available fruit biomass that one rooted shrub
/// may transfer at this weather front. These ratios never consume fluid or
/// add material: the source is a fixed, previously present underground reserve.
fn moisture_adjusted_crop(open_crop: u64, moisture: RootMoisture) -> u64 {
    match moisture {
        RootMoisture::NoBasin => open_crop,
        RootMoisture::Basin { fraction_ppm } => {
            // An exhausted basin supports half the normal fruit; a full
            // basin supplies 125%. Intermediate water depletion changes
            // yield proportionally, without consuming this fluid.
            let multiplier_ppm = 500_000 + u64::from(fraction_ppm) * 3 / 4;
            u64::try_from(u128::from(open_crop) * u128::from(multiplier_ppm) / 1_000_000)
                .unwrap_or(u64::MAX)
        }
    }
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
) {
    if !at.value().is_multiple_of(FIELD_CHECK_TICKS) {
        return;
    }
    let growing_season = matches!(
        registries.core().calendar().season_at(at),
        Season::Spring | Season::Summer
    );
    let Some(profile) = state.world().generation_profile() else {
        return;
    };
    let Some(next_revision) = state.surface().revision().checked_add(1) else {
        return;
    };
    let changes = state
        .surface()
        .resources()
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
                || state.structures().blocks_full_cell(source.position())
                || !has_open_sky(registries, state, source.position())
            {
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
                // the patch's recovery without erasing its old matter.
                return None;
            }
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
            // The physical crown of an older neighboring tree intercepts
            // direct sunlight from the understory shrub. Smaller crops
            // leave their unspent mass in its actual root reserve; no water,
            // organic matter or food is silently removed or manufactured.
            let shade = u64::from(living_canopy_shade_at(
                registries,
                state,
                source.position(),
                at,
            ));
            let open_batch = (plant.original_fruit.milligrams() / 4).max(1);
            let light_fraction = 8_u64.saturating_sub(shade.min(8));
            let watered_batch =
                moisture_adjusted_crop(open_batch, root_moisture_at(state, source.position()));
            let batch = Mass::from_milligrams(
                plant
                    .root_reserve
                    .milligrams()
                    .min((watered_batch * light_fraction / 8).max(1)),
            );
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
        state
            .surface_state_mut()
            .apply_perennial_cycle(changes, next_revision);
    }
}

#[cfg(test)]
#[path = "forage_cycle_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "forage_moisture_tests.rs"]
mod moisture_tests;
