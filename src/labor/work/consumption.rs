//! Durable direct-consumption attention intervals.

use serde::{Deserialize, Serialize};

use crate::core::quantity::{Mass, Volume};
use crate::core::time::SimulationTick;

/// Durable attention interval occupied by one already-admitted direct meal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EatingWork {
    mass: Mass,
    started_at: SimulationTick,
    completes_at: SimulationTick,
}

impl EatingWork {
    pub(crate) const fn new(
        mass: Mass,
        started_at: SimulationTick,
        completes_at: SimulationTick,
    ) -> Self {
        Self {
            mass,
            started_at,
            completes_at,
        }
    }

    #[must_use]
    pub const fn mass(self) -> Mass {
        self.mass
    }

    #[must_use]
    pub const fn started_at(self) -> SimulationTick {
        self.started_at
    }

    #[must_use]
    pub const fn completes_at(self) -> SimulationTick {
        self.completes_at
    }
}

/// Durable attention interval occupied by one already-admitted direct drink.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrinkingWork {
    volume: Volume,
    started_at: SimulationTick,
    completes_at: SimulationTick,
}

impl DrinkingWork {
    pub(crate) const fn new(
        volume: Volume,
        started_at: SimulationTick,
        completes_at: SimulationTick,
    ) -> Self {
        Self {
            volume,
            started_at,
            completes_at,
        }
    }

    #[must_use]
    pub const fn volume(self) -> Volume {
        self.volume
    }

    #[must_use]
    pub const fn started_at(self) -> SimulationTick {
        self.started_at
    }

    #[must_use]
    pub const fn completes_at(self) -> SimulationTick {
        self.completes_at
    }
}
