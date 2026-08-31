//! Durable production-job schema and read-only job projections.

use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::core::quantity::Mass;
use crate::core::time::{SimulationTick, TickSpan};
use crate::energy::{ConsumedEnergyTrace, ReleasedEnergyTrace};
use crate::equipment::{EquipmentId, EquipmentOperationTrace};
use crate::inventory::{
    ConsumedMaterialTrace, MaterialStorageHistory, StockpileId, checked_consumed_material_mass,
};
use crate::maintenance::Condition;
use crate::material::MaterialLotSpec;

use super::super::definitions::ProcessId;
use super::super::resolution::ProcessOutputStreamId;

/// Durable routing for one physically inseparable resolved output stream.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductionOutputStream {
    pub(in crate::production) id: ProcessOutputStreamId,
    pub(in crate::production) destination: StockpileId,
    pub(in crate::production) outputs: Vec<MaterialLotSpec>,
}

/// Why an in-flight production job is currently unable to accumulate active process time.
///
/// Suspension never manufactures a failure product. The production job remains the authoritative
/// owner of its consumed matter and energy until its physical requirements become usable again.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ProductionSuspensionReason {
    EquipmentSupportUnavailable { equipment: EquipmentId },
    OutputSupportUnavailable { stockpile: StockpileId },
    PlayerLaborUnavailable,
}

/// When an occupied resource can become available to unrelated work.
///
/// Running jobs have a scheduled wall-clock release. Suspended jobs expose no scheduled release
/// because availability depends on physical recovery and resume.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductionOccupancyRelease {
    Scheduled(SimulationTick),
    AwaitingResume,
}

impl Display for ProductionOccupancyRelease {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Scheduled(tick) => write!(formatter, "until tick {}", tick.value()),
            Self::AwaitingResume => {
                formatter.write_str("while its production job is suspended awaiting recovery")
            }
        }
    }
}

/// Durable pause state for one production job whose active-time clock is not currently advancing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductionSuspension {
    pub(super) suspended_at: SimulationTick,
    pub(super) remaining_active_time: TickSpan,
    pub(super) reason: ProductionSuspensionReason,
}

impl ProductionSuspension {
    pub(super) const fn new(
        suspended_at: SimulationTick,
        remaining_active_time: TickSpan,
        reason: ProductionSuspensionReason,
    ) -> Self {
        Self {
            suspended_at,
            remaining_active_time,
            reason,
        }
    }

    #[must_use]
    pub const fn suspended_at(self) -> SimulationTick {
        self.suspended_at
    }

    #[must_use]
    pub const fn remaining_active_time(self) -> TickSpan {
        self.remaining_active_time
    }

    #[must_use]
    pub const fn reason(self) -> ProductionSuspensionReason {
        self.reason
    }
}

impl ProductionOutputStream {
    #[must_use]
    pub const fn id(&self) -> ProcessOutputStreamId {
        self.id
    }

    #[must_use]
    pub const fn destination(&self) -> StockpileId {
        self.destination
    }

    #[must_use]
    pub fn outputs(&self) -> &[MaterialLotSpec] {
        &self.outputs
    }
}

/// Persistent monotonically allocated production job identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ProductionJobId(u64);

impl ProductionJobId {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        assert!(value != 0, "production job id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// Durable running material transformation with capacity reserved until completion.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductionJobRecord {
    pub(in crate::production) identity: ProductionJobIdentity,
    pub(in crate::production) schedule: ProductionJobSchedule,
    pub(in crate::production) resources: ProductionJobResources,
    pub(in crate::production) equipment: ProductionJobEquipment,
    pub(in crate::production) output_streams: Vec<ProductionOutputStream>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::production) struct ProductionJobIdentity {
    pub(in crate::production) id: ProductionJobId,
    pub(in crate::production) process: ProcessId,
    pub(in crate::production) source: StockpileId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::production) struct ProductionJobSchedule {
    pub(in crate::production) started_at: SimulationTick,
    pub(in crate::production) completes_at: SimulationTick,
    pub(in crate::production) active_duration: TickSpan,
    /// Wall-clock suspension time from completed pause intervals.
    ///
    /// The currently active suspension, if any, is deliberately excluded until resume. This keeps
    /// `completes_at = started_at + active_duration + completed_suspension_time` true for both
    /// running and suspended jobs while retaining enough durable history to replay the schedule.
    pub(in crate::production) completed_suspension_time: TickSpan,
    pub(in crate::production) suspension: Option<ProductionSuspension>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::production) struct ProductionJobResources {
    pub(in crate::production) consumed_inputs: Vec<ConsumedMaterialTrace>,
    pub(in crate::production) material_storage_history: MaterialStorageHistory,
    pub(in crate::production) consumed_energy: Option<ConsumedEnergyTrace>,
    pub(in crate::production) released_energy: Option<ReleasedEnergyTrace>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::production) struct ProductionJobEquipment {
    pub(in crate::production) provider: Option<EquipmentOperationTrace>,
    pub(in crate::production) requires_active_support: bool,
    pub(in crate::production) condition_after: Option<Condition>,
}

impl ProductionJobRecord {
    #[must_use]
    pub const fn id(&self) -> ProductionJobId {
        self.identity.id
    }

    #[must_use]
    pub const fn process(&self) -> ProcessId {
        self.identity.process
    }

    #[must_use]
    pub const fn source(&self) -> StockpileId {
        self.identity.source
    }

    #[must_use]
    pub const fn started_at(&self) -> SimulationTick {
        self.schedule.started_at
    }

    #[must_use]
    pub const fn completes_at(&self) -> SimulationTick {
        self.schedule.completes_at
    }

    /// Returns the authored/resolved amount of active process time required by this operation.
    /// Wall-clock suspension never changes this physics contract.
    #[must_use]
    pub const fn active_duration(&self) -> TickSpan {
        self.schedule.active_duration
    }

    /// Returns the current suspension state, if this job is retaining work-in-process while paused.
    #[must_use]
    pub const fn suspension(&self) -> Option<ProductionSuspension> {
        self.schedule.suspension
    }

    #[must_use]
    pub const fn is_suspended(&self) -> bool {
        self.schedule.suspension.is_some()
    }

    /// Returns the externally meaningful release horizon for resources exclusively owned by this
    /// job. A suspended operation has no scheduled release until it resumes.
    #[must_use]
    pub const fn occupancy_release(&self) -> ProductionOccupancyRelease {
        if self.schedule.suspension.is_some() {
            ProductionOccupancyRelease::AwaitingResume
        } else {
            ProductionOccupancyRelease::Scheduled(self.schedule.completes_at)
        }
    }

    #[must_use]
    pub fn consumed_mass(&self) -> Mass {
        checked_consumed_material_mass(&self.resources.consumed_inputs).unwrap_or_else(|| {
            panic!(
                "validated production job {} consumed input mass overflowed",
                self.id().value()
            )
        })
    }

    #[must_use]
    pub fn consumed_inputs(&self) -> &[ConsumedMaterialTrace] {
        &self.resources.consumed_inputs
    }

    /// Returns inherited perishability exposure for the in-flight matter, rebased to job start.
    #[must_use]
    pub(crate) const fn material_storage_history(&self) -> MaterialStorageHistory {
        self.resources.material_storage_history
    }

    /// Returns the finite energy moved into this in-flight operation at start, if any.
    #[must_use]
    pub const fn consumed_energy(&self) -> Option<ConsumedEnergyTrace> {
        self.resources.consumed_energy
    }

    /// Returns exact energy released from process inputs and awaiting sink commit, if any.
    #[must_use]
    pub const fn released_energy(&self) -> Option<ReleasedEnergyTrace> {
        self.resources.released_energy
    }

    /// Returns the equipment provider exclusively occupied by this operation, if any.
    #[must_use]
    pub const fn equipment_provider(&self) -> Option<EquipmentOperationTrace> {
        self.equipment.provider
    }

    /// Whether this operation was authorized only while its equipment had an active structural
    /// support. Unsupported/free-standing providers do not acquire this requirement implicitly.
    #[must_use]
    pub const fn has_required_active_support(&self) -> bool {
        self.equipment.requires_active_support
    }

    /// Returns the persisted post-operation condition for the occupied equipment provider.
    #[must_use]
    pub const fn equipment_condition_after(&self) -> Option<Condition> {
        self.equipment.condition_after
    }

    /// Returns exact material streams and their committed destinations.
    #[must_use]
    pub fn output_streams(&self) -> &[ProductionOutputStream] {
        &self.output_streams
    }

    /// Returns the sole durable stream for process families that require single-stream output.
    #[must_use]
    pub fn single_output_stream(&self) -> Option<&ProductionOutputStream> {
        let [stream] = self.output_streams.as_slice() else {
            return None;
        };
        Some(stream)
    }
}
