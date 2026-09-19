//! Durable mining job identity, custody, schedule, and actor-safe read surface.

use std::fmt::{Debug, Formatter};

use serde::{Deserialize, Serialize};

use crate::core::quantity::Mass;
use crate::core::time::SimulationTick;
use crate::equipment::{EquipmentDefinitionId, EquipmentId, EquipmentOperationTrace};
use crate::geology::GeologicalDepositId;
use crate::inventory::StockpileId;
use crate::maintenance::Condition;
use crate::material::MaterialLotSpec;

use super::super::MiningMethodId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MiningJobId(u64);

impl MiningJobId {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        assert!(value != 0, "mining job id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::mining) struct MiningJobIdentity {
    pub(in crate::mining) id: MiningJobId,
    pub(in crate::mining) method: MiningMethodId,
    pub(in crate::mining) deposit: GeologicalDepositId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::mining) struct MiningJobResources {
    pub(in crate::mining) destination: StockpileId,
    pub(in crate::mining) equipment_trace: EquipmentOperationTrace,
    pub(in crate::mining) deposit_mass_before: Mass,
    pub(in crate::mining) requested_mass: Mass,
    pub(in crate::mining) output: MaterialLotSpec,
    pub(in crate::mining) equipment_condition_after: Condition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::mining) enum MiningJobPhase {
    Working,
    ReadyToClaim,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::mining) struct MiningJobSchedule {
    pub(in crate::mining) started_at: SimulationTick,
    pub(in crate::mining) completes_at: SimulationTick,
    pub(in crate::mining) phase: MiningJobPhase,
}

#[derive(Clone, Deserialize)]
#[cfg_attr(any(test, feature = "test-gameplay"), derive(PartialEq, Eq))]
#[serde(deny_unknown_fields)]
pub struct MiningJobRecord {
    pub(in crate::mining::state) identity: MiningJobIdentity,
    pub(in crate::mining::state) resources: MiningJobResources,
    pub(in crate::mining::state) schedule: MiningJobSchedule,
}

/// Actor-safe diagnostics for durable mining work.
///
/// The deposit binding, source-mass trace, and exact reserved output are execution proofs rather
/// than player knowledge. Public diagnostics therefore expose only the same schedule and equipment
/// facts available through the record's public read API.
impl Debug for MiningJobRecord {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MiningJobRecord")
            .field("id", &self.id())
            .field("method", &self.method())
            .field("destination", &self.destination())
            .field("equipment", &self.equipment())
            .field("equipment_definition", &self.equipment_definition())
            .field("started_at", &self.started_at())
            .field("completes_at", &self.completes_at())
            .field(
                "equipment_condition_before",
                &self.equipment_condition_before(),
            )
            .field(
                "equipment_condition_after",
                &self.equipment_condition_after(),
            )
            .field("working", &self.is_working())
            .field("ready_to_claim", &self.is_ready_to_claim())
            .finish_non_exhaustive()
    }
}

impl MiningJobRecord {
    pub(in crate::mining) const fn new(
        identity: MiningJobIdentity,
        resources: MiningJobResources,
        schedule: MiningJobSchedule,
    ) -> Self {
        Self {
            identity,
            resources,
            schedule,
        }
    }

    #[must_use]
    pub const fn id(&self) -> MiningJobId {
        self.identity.id
    }

    #[must_use]
    pub const fn method(&self) -> MiningMethodId {
        self.identity.method
    }

    #[must_use]
    pub(crate) const fn deposit(&self) -> GeologicalDepositId {
        self.identity.deposit
    }

    #[must_use]
    pub(crate) const fn deposit_mass_before(&self) -> Mass {
        self.resources.deposit_mass_before
    }

    #[must_use]
    pub(crate) const fn requested_mass(&self) -> Mass {
        self.resources.requested_mass
    }

    #[must_use]
    pub const fn destination(&self) -> StockpileId {
        self.resources.destination
    }

    #[must_use]
    pub const fn equipment(&self) -> EquipmentId {
        self.resources.equipment_trace.equipment()
    }

    #[must_use]
    pub const fn equipment_definition(&self) -> EquipmentDefinitionId {
        self.resources.equipment_trace.definition()
    }

    #[must_use]
    pub const fn started_at(&self) -> SimulationTick {
        self.schedule.started_at
    }

    #[must_use]
    pub const fn completes_at(&self) -> SimulationTick {
        self.schedule.completes_at
    }

    #[must_use]
    pub(crate) const fn output(&self) -> &MaterialLotSpec {
        &self.resources.output
    }

    #[must_use]
    pub const fn equipment_condition_before(&self) -> Condition {
        self.resources.equipment_trace.condition()
    }

    #[must_use]
    pub const fn equipment_condition_after(&self) -> Condition {
        self.resources.equipment_condition_after
    }

    #[must_use]
    pub const fn is_working(&self) -> bool {
        matches!(self.schedule.phase, MiningJobPhase::Working)
    }

    #[must_use]
    pub const fn is_ready_to_claim(&self) -> bool {
        matches!(self.schedule.phase, MiningJobPhase::ReadyToClaim)
    }
}
