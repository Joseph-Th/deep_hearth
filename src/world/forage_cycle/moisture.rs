//! Local natural-spring moisture observations for finite perennial regrowth.

use crate::content::FLUID_WATER;
use crate::core::state::AppState;
use crate::spatial::VoxelCoord;

/// Shallow basin moisture influences nearby roots, not an entire region or a
/// distant aquifer. No permeability or nutrient transport model is claimed.
pub(super) const BASIN_ROOT_REACH_VOXELS: u64 = 12;
pub(super) const BASIN_ROOT_VERTICAL_REACH_VOXELS: u64 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RootMoisture {
    NoBasin,
    /// Finite natural source volume as a fraction of the original basin size.
    Basin {
        fraction_ppm: u32,
    },
}

/// One read-only survey of current natural spring custody per vegetation front.
/// Sorting by X limits each plant's moisture lookup to nearby spring columns
/// without repeated global scans as the generated world grows.
/// No mutable cache survives the tick or the save/load boundary.
pub(super) struct RootMoistureSurvey {
    pub(super) springs: Vec<(VoxelCoord, u32)>,
}

impl RootMoistureSurvey {
    pub(super) fn observe(state: &AppState) -> Self {
        let mut springs = state
            .fluid()
            .spring_soil()
            .filter_map(|(id, soil)| {
                if state.logistics().fluid_store_position(id) != Some(soil.position) {
                    return None;
                }
                let pool = state.fluid().get_store(id)?;
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
                Some((soil.position, fraction_ppm))
            })
            .collect::<Vec<_>>();
        springs.sort_unstable_by_key(|(position, _)| position.x());
        Self { springs }
    }

    /// A dry-looking pool can still nourish roots through its finite soil
    /// custody. An unrelated nearby bucket does not count as an aquifer.
    pub(super) fn at(&self, position: VoxelCoord) -> RootMoisture {
        let first_x = position.x().saturating_sub(BASIN_ROOT_REACH_VOXELS as i64);
        let last_x = position.x().saturating_add(BASIN_ROOT_REACH_VOXELS as i64);
        let first = self
            .springs
            .partition_point(|(spring, _)| spring.x() < first_x);
        let wettest = self.springs[first..]
            .iter()
            .take_while(|(spring, _)| spring.x() <= last_x)
            .filter(|(spring, _)| {
                position.z().abs_diff(spring.z()) <= BASIN_ROOT_REACH_VOXELS
                    && position.y().abs_diff(spring.y()) <= BASIN_ROOT_VERTICAL_REACH_VOXELS
            })
            .map(|(_, fraction_ppm)| *fraction_ppm)
            .max();
        wettest.map_or(RootMoisture::NoBasin, |fraction_ppm| RootMoisture::Basin {
            fraction_ppm,
        })
    }
}
