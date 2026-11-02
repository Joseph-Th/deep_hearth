//! Bounded player heat exposure from actually dissipated, physically located energy.

use crate::core::quantity::Energy;
use crate::core::state::AppState;
use crate::registry::Registries;

use super::passive_dissipation::PassiveEnergyDissipationPlan;

/// A fire heats its own cell and the directly adjoining air, not a remotely
/// queried store. The first face-adjacent ring receives half the source's
/// dissipated heat; diagonal cells and more distant locations receive none.
/// This is a read-only exposure projection, not a second energy withdrawal.
pub(crate) fn assess_player_local_warmth(
    registries: &Registries,
    state: &AppState,
    passive: &PassiveEnergyDissipationPlan,
    maximum_relevant: Energy,
) -> Energy {
    let Some(position) = state.player_observation_position() else {
        return Energy::ZERO;
    };
    if maximum_relevant.is_zero() {
        return Energy::ZERO;
    }
    let mut warmth = Energy::ZERO;
    // Location index limits the search to the immediately adjoining cells.
    // Stable store-ID order makes overlapping heat sources deterministic.
    for (id, source) in state.logistics().energy_store_locations_near(position, 1) {
        let distance = position.x().abs_diff(source.x())
            + position.y().abs_diff(source.y())
            + position.z().abs_diff(source.z());
        if distance > 1 {
            continue;
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
            .unwrap_or_else(|| panic!("runtime invariant broken: energy definition disappeared"));
        if !definition.is_local_warmth_source() {
            continue;
        }
        // An emitter encased in an opaque cell must not radiate through it.
        if !super::illumination::voxel_transmits_light(registries, state, source) {
            continue;
        }
        let dissipated = passive.dissipated_for_store(id);
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
