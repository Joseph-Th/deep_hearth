//! Deterministic passive loss from finite stores into unmodeled environmental energy domains.

use std::cmp::min;

use crate::core::quantity::Energy;
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::registry::Registries;

use super::definitions::EnergyStoreDefinition;
use super::integration::{PowerRemainder, integrate_power};
use super::state::EnergyStoreId;

fn passive_dissipation_per_tick(
    registries: &Registries,
    definition: &EnergyStoreDefinition,
) -> Energy {
    let power = definition.passive_dissipation_power();
    if power.is_zero() {
        return Energy::ZERO;
    }
    let integration = integrate_power(
        power,
        TickSpan::new(1),
        registries.core().physical_tick_duration(),
        PowerRemainder::ZERO,
    )
    .unwrap_or_else(|error| {
        panic!(
            "validated passive dissipation for energy store definition {} failed at runtime: {error}",
            definition.id().value()
        )
    });
    assert_eq!(
        integration.remainder(),
        PowerRemainder::ZERO,
        "validated passive dissipation must remain exact at runtime"
    );
    integration.energy()
}

/// Projects the greatest stored energy that can remain after an exact number of unavoidable
/// passive-loss ticks. The result is deterministic because registry validation requires passive
/// loss to integrate to whole nanojoules on every authoritative tick.
pub(crate) fn project_stored_energy_after_passive_dissipation(
    registries: &Registries,
    definition: &EnergyStoreDefinition,
    stored: Energy,
    ticks: TickSpan,
) -> Energy {
    if stored.is_zero() || ticks.is_zero() {
        return stored;
    }
    let per_tick = passive_dissipation_per_tick(registries, definition);
    if per_tick.is_zero() {
        return stored;
    }
    let stored_nj = stored.nanojoules();
    let per_tick_nj = per_tick.nanojoules();
    let tick_count = u128::from(ticks.value());
    let ticks_until_empty = stored_nj.div_ceil(per_tick_nj);
    if tick_count >= ticks_until_empty {
        return Energy::ZERO;
    }
    let dissipated = per_tick_nj
        .checked_mul(tick_count)
        .unwrap_or_else(|| unreachable!("sub-empty passive-loss projection fits u128"));
    Energy::from_nanojoules(
        stored_nj
            .checked_sub(dissipated)
            .unwrap_or_else(|| unreachable!("sub-empty passive-loss projection cannot underflow")),
    )
}

/// Exact number of authoritative passive-loss ticks required to empty `stored` energy.
///
/// Returns zero ticks for an empty store. Returns `None` when the definition has no passive-loss
/// route or when the required duration exceeds the represented [`TickSpan`] range.
#[must_use]
pub fn passive_dissipation_ticks_until_empty(
    registries: &Registries,
    definition: &EnergyStoreDefinition,
    stored: Energy,
) -> Option<TickSpan> {
    if stored.is_zero() {
        return Some(TickSpan::new(0));
    }
    let per_tick = passive_dissipation_per_tick(registries, definition);
    if per_tick.is_zero() {
        return None;
    }
    let ticks = stored.nanojoules().div_ceil(per_tick.nanojoules());
    u64::try_from(ticks).ok().map(TickSpan::new)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PassiveEnergyDissipationEntry {
    store: EnergyStoreId,
    stored_before: Energy,
    dissipated: Energy,
}

/// Pre-tick passive-loss decisions for every finite store with authored dissipation.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct PassiveEnergyDissipationPlan {
    entries: Vec<PassiveEnergyDissipationEntry>,
}

impl PassiveEnergyDissipationPlan {
    pub(crate) fn energy_revision_steps(&self) -> u64 {
        u64::from(!self.entries.is_empty())
    }
}

/// Decides passive losses against the pre-tick energy snapshot without mutation.
pub(crate) fn decide_passive_energy_dissipation(
    registries: &Registries,
    state: &AppState,
) -> PassiveEnergyDissipationPlan {
    let mut entries = Vec::new();
    for record in state.energy().stores() {
        if record.stored().is_zero() {
            continue;
        }
        let definition = registries
            .energy()
            .get_store(record.definition())
            .unwrap_or_else(|| {
                panic!(
                    "runtime invariant broken: energy store {} references missing definition {}",
                    record.id().value(),
                    record.definition().value()
                )
            });
        if definition.passive_dissipation_power().is_zero() {
            continue;
        }
        let dissipated = min(
            record.stored(),
            passive_dissipation_per_tick(registries, definition),
        );
        if dissipated.is_zero() {
            continue;
        }
        entries.push(PassiveEnergyDissipationEntry {
            store: record.id(),
            stored_before: record.stored(),
            dissipated,
        });
    }
    PassiveEnergyDissipationPlan { entries }
}

/// Applies pre-tick passive losses after other same-tick energy ingress has committed.
///
/// Earlier canonical tick phases may add to a store but may not consume energy that was present in
/// the pre-tick snapshot. This keeps newly captured energy observable until the following tick.
pub(crate) fn apply_passive_energy_dissipation(
    state: &mut AppState,
    plan: PassiveEnergyDissipationPlan,
) {
    if plan.entries.is_empty() {
        return;
    }
    for entry in &plan.entries {
        let stored = state
            .energy()
            .get_store(entry.store)
            .unwrap_or_else(|| {
                panic!(
                    "runtime invariant broken: passively dissipating energy store {} disappeared",
                    entry.store.value()
                )
            })
            .stored();
        assert!(
            stored >= entry.stored_before,
            "same-tick phases consumed pre-tick energy before passive dissipation"
        );
    }

    let current_revision = state.energy().revision();
    let next_revision = current_revision
        .checked_add(1)
        .unwrap_or_else(|| panic!("prevalidated passive energy revision exhausted"));
    let energy = state.energy_state_mut();
    for entry in plan.entries {
        energy.subtract_stored_energy(entry.store, entry.dissipated);
    }
    energy.apply_revision(next_revision);
}

#[cfg(test)]
#[path = "passive_dissipation_tests.rs"]
mod tests;
