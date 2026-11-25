//! Bounded player heat exposure from actually dissipated, physically located energy.

use crate::core::quantity::Energy;
use crate::core::state::AppState;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

use super::definitions::EnergyStoreDefinitionId;
use super::passive_dissipation::PassiveEnergyDissipationPlan;
use super::{EnergyStoreId, EnergyStoreRecord};

/// A currently radiating fire within the admitted player's physical heat radius.
/// This is observation only; adjacent sources cannot be operated as local stores.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NearbyWarmthSource {
    store: EnergyStoreId,
    position: VoxelCoord,
    definition: EnergyStoreDefinitionId,
    stored: Energy,
}

impl NearbyWarmthSource {
    pub const fn store(self) -> EnergyStoreId {
        self.store
    }
    pub const fn position(self) -> VoxelCoord {
        self.position
    }
    pub const fn definition(self) -> EnergyStoreDefinitionId {
        self.definition
    }
    pub const fn stored(self) -> Energy {
        self.stored
    }
}

/// The exact heat-source eligibility shared by slow-tick physiology and player feedback.
/// Keeping this in the energy owner prevents the text client from advertising heat
/// from an empty, insulated, unlocated or non-dissipating store.
fn radiating_sources<'a>(
    registries: &'a Registries,
    state: &'a AppState,
) -> impl Iterator<Item = (VoxelCoord, &'a EnergyStoreRecord, u64)> + 'a {
    state
        .player_observation_position()
        .into_iter()
        .flat_map(move |player| {
            state
                .logistics()
                .energy_store_locations_near(player, 1)
                .into_iter()
                .filter_map(move |(id, source)| {
                    let distance = player.x().abs_diff(source.x())
                        + player.y().abs_diff(source.y())
                        + player.z().abs_diff(source.z());
                    if distance > 1 {
                        return None;
                    }
                    let store = state.energy().get_store(id).unwrap_or_else(|| {
                        panic!(
                            "runtime invariant broken: located energy store {} disappeared",
                            id.value()
                        )
                    });
                    let definition = registries
                        .energy()
                        .get_store(store.definition())
                        .unwrap_or_else(|| {
                            panic!("runtime invariant broken: energy definition disappeared")
                        });
                    if !definition.is_local_warmth_source()
                        || store.stored().is_zero()
                        || registries
                            .energy()
                            .passive_dissipation_per_tick(definition.id())
                            .is_zero()
                        || !crate::world::has_clear_two_step_sightline(
                            registries, state, player, source,
                        )
                    {
                        return None;
                    }
                    Some((source, store, distance))
                })
        })
}

/// Reports the same currently effective nearby emitters used by thermal physiology.
/// A face-adjacent fire warms the player but does not become accessible for fueling.
#[must_use]
pub fn project_player_nearby_warmth_sources(
    registries: &Registries,
    state: &AppState,
) -> Vec<NearbyWarmthSource> {
    radiating_sources(registries, state)
        .map(|(position, store, _)| NearbyWarmthSource {
            store: store.id(),
            position,
            definition: store.definition(),
            stored: store.stored(),
        })
        .collect()
}

/// A fire heats its own cell and the directly adjoining air, not a remotely
/// queried store. The first face-adjacent ring receives half the source's
/// dissipated heat only across a clear route; diagonal cells, obstructions and
/// missing terrain on a locally generated route receive none.
/// This is a read-only exposure projection, not a second energy withdrawal.
pub(crate) fn assess_player_local_warmth(
    registries: &Registries,
    state: &AppState,
    passive: &PassiveEnergyDissipationPlan,
    maximum_relevant: Energy,
) -> Energy {
    if maximum_relevant.is_zero() {
        return Energy::ZERO;
    }
    let mut warmth = Energy::ZERO;
    // Stable store-ID order makes overlapping heat sources deterministic.
    for (_, store, distance) in radiating_sources(registries, state) {
        let dissipated = passive.dissipated_for_store(store.id());
        let contribution = if distance == 0 {
            dissipated
        } else {
            Energy::from_nanojoules(dissipated.nanojoules() / 2)
        };
        warmth = cap_warmth(warmth, contribution, maximum_relevant);
        if warmth == maximum_relevant {
            break;
        }
    }
    warmth
}

fn cap_warmth(total: Energy, contribution: Energy, maximum: Energy) -> Energy {
    if total >= maximum {
        return maximum;
    }
    let remaining = maximum
        .checked_sub(total)
        .expect("sub-maximum warmth has representable headroom");
    total
        .checked_add(contribution.min(remaining))
        .expect("capped warmth fits represented energy")
}

#[cfg(test)]
#[path = "local_warmth_tests.rs"]
mod tests;
