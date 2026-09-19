//! Completion-time validation and application of released energy.

use std::collections::BTreeMap;

use crate::core::quantity::Energy;
use crate::energy::state::{EnergyState, EnergyStoreId};

use super::ReleasedEnergyTrace;

pub(crate) fn apply_released_energy_outcomes(
    state: &mut EnergyState,
    expected_revision: u64,
    next_revision: u64,
    traces: &[ReleasedEnergyTrace],
) {
    assert_released_energy_outcomes_available(state, expected_revision, next_revision, traces);
    for trace in traces {
        state.add_stored_energy(trace.destination, trace.energy);
    }
    state.apply_revision(next_revision);
}

pub(crate) fn assert_released_energy_outcomes_available(
    state: &EnergyState,
    expected_revision: u64,
    next_revision: u64,
    traces: &[ReleasedEnergyTrace],
) {
    assert_eq!(
        state.revision(),
        expected_revision,
        "released-energy completion requires its planned energy revision"
    );
    assert_eq!(
        expected_revision.checked_add(1),
        Some(next_revision),
        "released-energy completion must advance the energy revision exactly once"
    );
    let mut additions = BTreeMap::<EnergyStoreId, Energy>::new();
    for trace in traces {
        let record = state.get_store(trace.destination).unwrap_or_else(|| {
            panic!(
                "released-energy destination {} disappeared before commit",
                trace.destination.value()
            )
        });
        assert_eq!(
            record.definition(),
            trace.definition,
            "released-energy destination definition changed before commit"
        );
        let total = additions.entry(trace.destination).or_insert(Energy::ZERO);
        *total = total
            .checked_add(trace.energy)
            .unwrap_or_else(|| panic!("released-energy batch overflowed"));
    }
    for (store, addition) in additions {
        let record = state
            .get_store(store)
            .unwrap_or_else(|| unreachable!("released-energy destination was prechecked"));
        record
            .stored()
            .checked_add(addition)
            .unwrap_or_else(|| panic!("released-energy destination overflowed before commit"));
    }
}
