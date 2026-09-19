//! Revision-bound deferred-ingress reservation for validated finite energy sinks.

use crate::core::time::TickSpan;
use crate::energy::state::EnergyState;
use crate::registry::Registries;

use super::{
    EnergyIngressReservationError, EnergySinkCapacityError, ReleasedEnergyTrace,
    ValidatedEnergySink, validate_energy_sink_capacity_at_release,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EnergyIngressReservation {
    expected_revision: u64,
    trace: ReleasedEnergyTrace,
}

impl EnergyIngressReservation {
    pub(crate) const fn expected_revision(self) -> u64 {
        self.expected_revision
    }

    pub(crate) const fn trace(self) -> ReleasedEnergyTrace {
        self.trace
    }

    pub(crate) fn assert_matches_state(&self, state: &EnergyState) {
        assert_eq!(
            state.revision(),
            self.expected_revision,
            "energy ingress reservation requires its validated owner revision"
        );
        let record = state.get_store(self.trace.destination).unwrap_or_else(|| {
            panic!(
                "validated energy sink {} disappeared before process start",
                self.trace.destination.value()
            )
        });
        assert_eq!(
            record.definition(),
            self.trace.definition,
            "validated energy sink definition changed before process start"
        );
    }
}

pub(crate) fn validate_energy_ingress_reservation(
    registries: &Registries,
    state: &EnergyState,
    selection: ValidatedEnergySink,
    release_after: TickSpan,
) -> Result<EnergyIngressReservation, EnergyIngressReservationError> {
    if state.revision() != selection.expected_revision {
        return Err(EnergyIngressReservationError::StaleSelection {
            expected: selection.expected_revision,
            actual: state.revision(),
        });
    }
    let trace = selection.trace;
    let Some(record) = state.get_store(trace.destination) else {
        return Err(EnergyIngressReservationError::UnknownStore {
            store: trace.destination,
        });
    };
    validate_energy_sink_capacity_at_release(
        registries,
        record.definition(),
        record.stored(),
        trace.energy,
        release_after,
    )
    .map_err(|error| match error {
        EnergySinkCapacityError::Overflow => EnergyIngressReservationError::CapacityOverflow {
            store: trace.destination,
        },
        EnergySinkCapacityError::Insufficient {
            stored,
            requested,
            capacity,
        } => EnergyIngressReservationError::InsufficientCapacity {
            store: trace.destination,
            stored,
            requested,
            capacity,
        },
    })?;
    Ok(EnergyIngressReservation {
        expected_revision: state.revision(),
        trace,
    })
}
