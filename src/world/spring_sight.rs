//! Discover nearby located fluid without exposing the hidden regional water ledger.

use crate::core::state::AppState;
use crate::fluid::FluidStoreId;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::survival::Vitality;

/// An actual visible source, not its quantity, temperature, or an inferred
/// remote aquifer. Drinking still requires ordinary exact-local access.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisibleWaterSource {
    store: FluidStoreId,
    position: VoxelCoord,
}

impl VisibleWaterSource {
    #[must_use]
    pub const fn store(self) -> FluidStoreId {
        self.store
    }

    #[must_use]
    pub const fn position(self) -> VoxelCoord {
        self.position
    }
}

#[must_use]
pub fn available_visible_generated_water(
    registries: &Registries,
    state: &AppState,
) -> Vec<VisibleWaterSource> {
    let Some(player) = state.logistics().player() else {
        return Vec::new();
    };
    if state
        .survival()
        .player()
        .is_none_or(|survival| survival.vitality() == Vitality::ZERO)
    {
        return Vec::new();
    }
    let from = player.position();
    if !state.world().has_generated_terrain_at(from) {
        return Vec::new();
    }
    let mut nearby = Vec::new();
    for dx in -2_i64..=2 {
        let remaining = 2 - dx.abs();
        for dy in -remaining..=remaining {
            let last = remaining - dy.abs();
            for dz in -last..=last {
                let (Some(x), Some(y), Some(z)) = (
                    from.x().checked_add(dx),
                    from.y().checked_add(dy),
                    from.z().checked_add(dz),
                ) else {
                    continue;
                };
                let to = VoxelCoord::new(x, y, z);
                if !super::has_clear_two_step_sightline(registries, state, from, to)
                    || (super::assess_generated_skylight(registries, state, to) == 0
                        && crate::energy::assess_local_illumination(registries, state, to).level()
                            == 0)
                {
                    continue;
                }
                for store in state.logistics().fluid_stores_at(to) {
                    if state
                        .fluid()
                        .get_store(store)
                        .and_then(|record| record.contents())
                        .is_some_and(|contents| {
                            registries.survival().get_drink(contents.fluid()).is_some()
                        })
                    {
                        nearby.push(VisibleWaterSource {
                            store,
                            position: to,
                        });
                    }
                }
            }
        }
    }
    nearby.sort_unstable_by_key(|source| source.store);
    nearby
}
