//! Local natural-spring moisture observations for finite perennial regrowth.

use std::collections::BTreeMap;

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
/// Both horizontal axes are indexed: a long explored north-south strip must
/// not make every plant visit distant springs sharing its X coordinate.
/// No mutable cache survives the tick or the save/load boundary.
pub(super) struct RootMoistureSurvey {
    springs_by_x: BTreeMap<i64, BTreeMap<(i64, i64), u32>>,
}

impl RootMoistureSurvey {
    pub(super) fn from_springs(springs: impl IntoIterator<Item = (VoxelCoord, u32)>) -> Self {
        let mut springs_by_x = BTreeMap::<i64, BTreeMap<(i64, i64), u32>>::new();
        for (position, fraction_ppm) in springs {
            springs_by_x
                .entry(position.x())
                .or_default()
                .entry((position.z(), position.y()))
                .and_modify(|wettest| *wettest = (*wettest).max(fraction_ppm))
                .or_insert(fraction_ppm);
        }
        Self { springs_by_x }
    }

    pub(super) fn observe(state: &AppState) -> Self {
        let springs = state.fluid().spring_soil().filter_map(|(id, soil)| {
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
        });
        Self::from_springs(springs)
    }

    /// A dry-looking pool can still nourish roots through its finite soil
    /// custody. An unrelated nearby bucket does not count as an aquifer.
    pub(super) fn at(&self, position: VoxelCoord) -> RootMoisture {
        let first_x = position.x().saturating_sub(BASIN_ROOT_REACH_VOXELS as i64);
        let last_x = position.x().saturating_add(BASIN_ROOT_REACH_VOXELS as i64);
        let first_z = position.z().saturating_sub(BASIN_ROOT_REACH_VOXELS as i64);
        let last_z = position.z().saturating_add(BASIN_ROOT_REACH_VOXELS as i64);
        let horizontal_reach_squared = BASIN_ROOT_REACH_VOXELS * BASIN_ROOT_REACH_VOXELS;
        let mut wettest = None;
        for (&x, by_z) in self.springs_by_x.range(first_x..=last_x) {
            let dx = position.x().abs_diff(x);
            for (&(z, y), &fraction_ppm) in by_z.range((first_z, i64::MIN)..=(last_z, i64::MAX)) {
                let dz = position.z().abs_diff(z);
                if dx * dx + dz * dz <= horizontal_reach_squared
                    && position.y().abs_diff(y) <= BASIN_ROOT_VERTICAL_REACH_VOXELS
                {
                    wettest = Some(
                        wettest.map_or(fraction_ppm, |current: u32| current.max(fraction_ppm)),
                    );
                }
            }
        }
        wettest.map_or(RootMoisture::NoBasin, |fraction_ppm| RootMoisture::Basin {
            fraction_ppm,
        })
    }
}
