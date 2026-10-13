//! Durable direct-player work for setting one materialized structural member.

use serde::{Deserialize, Serialize};

use crate::core::time::SimulationTick;
use crate::labor::StructuralConstructionMethodId;
use crate::structural::StructuralElementId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructuralConstructionWork {
    element: StructuralElementId,
    method: StructuralConstructionMethodId,
    started_at: SimulationTick,
    completes_at: SimulationTick,
}

impl StructuralConstructionWork {
    #[must_use]
    pub(crate) const fn new(
        element: StructuralElementId,
        method: StructuralConstructionMethodId,
        started_at: SimulationTick,
        completes_at: SimulationTick,
    ) -> Self {
        Self {
            element,
            method,
            started_at,
            completes_at,
        }
    }

    #[must_use]
    pub const fn element(self) -> StructuralElementId {
        self.element
    }

    #[must_use]
    pub const fn method(self) -> StructuralConstructionMethodId {
        self.method
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
