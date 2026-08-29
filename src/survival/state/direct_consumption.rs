//! Persistent in-progress direct-consumption custody between admission and physiological uptake.

use serde::{Deserialize, Serialize};

use crate::core::quantity::{Mass, Temperature, Volume};
use crate::core::time::SimulationTick;
use crate::fluid::FluidDefinitionId;
use crate::inventory::{ConsumedMaterialTrace, checked_consumed_material_mass};

/// Exact food matter already removed from inventory while the player is still consuming it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PendingEating {
    consumed: Vec<ConsumedMaterialTrace>,
    started_at: SimulationTick,
    completes_at: SimulationTick,
}

impl PendingEating {
    #[must_use]
    pub(crate) fn new(
        consumed: Vec<ConsumedMaterialTrace>,
        started_at: SimulationTick,
        completes_at: SimulationTick,
    ) -> Self {
        Self {
            consumed,
            started_at,
            completes_at,
        }
    }

    #[must_use]
    pub(crate) fn consumed(&self) -> &[ConsumedMaterialTrace] {
        &self.consumed
    }

    #[must_use]
    pub(crate) fn total_mass(&self) -> Option<Mass> {
        checked_consumed_material_mass(&self.consumed)
    }

    #[must_use]
    pub(crate) const fn started_at(&self) -> SimulationTick {
        self.started_at
    }

    #[must_use]
    pub(crate) const fn completes_at(&self) -> SimulationTick {
        self.completes_at
    }
}

/// Exact drink identity already removed from its finite store while intake is still in progress.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PendingDrinking {
    fluid: FluidDefinitionId,
    volume: Volume,
    temperature: Temperature,
    started_at: SimulationTick,
    completes_at: SimulationTick,
}

impl PendingDrinking {
    #[must_use]
    pub(crate) const fn new(
        fluid: FluidDefinitionId,
        volume: Volume,
        temperature: Temperature,
        started_at: SimulationTick,
        completes_at: SimulationTick,
    ) -> Self {
        Self {
            fluid,
            volume,
            temperature,
            started_at,
            completes_at,
        }
    }

    #[must_use]
    pub(crate) const fn fluid(self) -> FluidDefinitionId {
        self.fluid
    }

    #[must_use]
    pub(crate) const fn volume(self) -> Volume {
        self.volume
    }

    #[must_use]
    pub(crate) const fn temperature(self) -> Temperature {
        self.temperature
    }

    #[must_use]
    pub(crate) const fn started_at(self) -> SimulationTick {
        self.started_at
    }

    #[must_use]
    pub(crate) const fn completes_at(self) -> SimulationTick {
        self.completes_at
    }
}

/// One admitted meal or drink whose matter has crossed the terminal consumption boundary but whose
/// physiological contribution is still being absorbed over the authored attention interval.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum PendingDirectConsumption {
    Eating(PendingEating),
    Drinking(PendingDrinking),
}

impl PendingDirectConsumption {
    #[must_use]
    pub(crate) const fn started_at(&self) -> SimulationTick {
        match self {
            Self::Eating(pending) => pending.started_at(),
            Self::Drinking(pending) => pending.started_at(),
        }
    }

    #[must_use]
    pub(crate) const fn completes_at(&self) -> SimulationTick {
        match self {
            Self::Eating(pending) => pending.completes_at(),
            Self::Drinking(pending) => pending.completes_at(),
        }
    }
}

/// Required serialized slot for the current-schema direct-consumption lifecycle.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DirectConsumptionState {
    pending: Option<PendingDirectConsumption>,
}

impl DirectConsumptionState {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self { pending: None }
    }

    #[must_use]
    pub(crate) const fn pending(&self) -> Option<&PendingDirectConsumption> {
        self.pending.as_ref()
    }

    pub(crate) fn begin(&mut self, pending: PendingDirectConsumption) {
        assert!(
            self.pending.is_none(),
            "direct consumption cannot begin while another intake remains pending"
        );
        self.pending = Some(pending);
    }

    pub(crate) fn set_pending(&mut self, pending: Option<PendingDirectConsumption>) {
        self.pending = pending;
    }
}
