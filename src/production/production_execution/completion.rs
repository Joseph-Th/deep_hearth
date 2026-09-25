//! Owns in-flight availability, completion planning, and atomic production-job completion.

use std::borrow::Cow;

use crate::core::time::{SimulationTick, TickSpan};
use crate::energy::ReleasedEnergyTrace;
use crate::equipment::EquipmentOperationConditionOutcome;
use crate::inventory::{
    MaterialLotId, ReservedDepositPlan, StockpileId, ValidatedStockpileStructuralLoad,
};
use crate::material::MaterialLotSpec;

use super::super::definitions::ProcessId;
use super::super::resolution::ProcessOutputStreamId;
use super::super::state::{
    ProductionAvailabilityDependencyRevisions, ProductionJobId, ProductionSuspensionReason,
};
use super::start::ProcessOutputRoute;

mod application;
mod availability;
mod errors;
mod planning;

pub(crate) use application::apply_completion_plan;
pub(crate) use errors::{CompletionCommitError, CompletionPlanError};
pub(crate) use planning::{decide_due_completions, plan_player_death_suspension};

/// Observable active-time scheduling change caused by a production provider becoming unavailable or
/// usable again. Work-in-process remains owned by the same job across both transitions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductionAvailabilityChange {
    Suspended {
        job: ProductionJobId,
        reason: ProductionSuspensionReason,
        suspended_at: SimulationTick,
        remaining_active_time: TickSpan,
    },
    SuspensionReasonChanged {
        job: ProductionJobId,
        previous: ProductionSuspensionReason,
        reason: ProductionSuspensionReason,
    },
    Resumed {
        job: ProductionJobId,
        reason: ProductionSuspensionReason,
        resumed_at: SimulationTick,
        scheduled_completion: SimulationTick,
    },
}

impl ProductionAvailabilityChange {
    #[must_use]
    pub const fn job(self) -> ProductionJobId {
        match self {
            Self::Suspended {
                job,
                reason: _reason,
                suspended_at: _suspended_at,
                remaining_active_time: _remaining_active_time,
            } => job,
            Self::SuspensionReasonChanged {
                job,
                previous: _previous,
                reason: _reason,
            } => job,
            Self::Resumed {
                job,
                reason: _reason,
                resumed_at: _resumed_at,
                scheduled_completion: _scheduled_completion,
            } => job,
        }
    }
}

/// Finds one availability transition in the canonical stable job-ID ordering.
///
/// Completion planning emits at most one transition per job and preserves job ordering. Death
/// suspension inserts through the same ordering contract. Cross-owner consumers use this helper
/// instead of repeatedly rescanning the transition slice.
pub(crate) fn find_availability_change(
    changes: &[ProductionAvailabilityChange],
    job: ProductionJobId,
) -> Option<ProductionAvailabilityChange> {
    changes
        .binary_search_by_key(&job, |change| change.job())
        .ok()
        .map(|index| changes[index])
}

/// Observable completion emitted by one simulation tick after authoritative output is committed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessCompletion {
    job: ProductionJobId,
    process: ProcessId,
    routes: Vec<ProcessOutputRoute>,
    landings: Vec<ProcessOutputLanding>,
}

/// Inventory landing receipt for one completed production output stream.
///
/// `parcels` preserves both the exact contributed output and the surviving inventory identity for
/// each resolved parcel. The identity may predate this process when canonical coalescing merged the
/// contribution into an existing lot.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessOutputLanding {
    stream: ProcessOutputStreamId,
    destination: StockpileId,
    parcels: Vec<ProcessParcelLanding>,
}

/// One exact production contribution paired with the inventory lot that survived ingress.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessParcelLanding {
    lot: MaterialLotId,
    output: MaterialLotSpec,
}

impl ProcessParcelLanding {
    #[must_use]
    pub const fn lot(&self) -> MaterialLotId {
        self.lot
    }

    #[must_use]
    pub const fn output(&self) -> &MaterialLotSpec {
        &self.output
    }
}

impl ProcessOutputLanding {
    #[must_use]
    pub const fn stream(&self) -> ProcessOutputStreamId {
        self.stream
    }

    #[must_use]
    pub const fn destination(&self) -> StockpileId {
        self.destination
    }

    pub fn parcels(&self) -> &[ProcessParcelLanding] {
        &self.parcels
    }
}

impl ProcessCompletion {
    #[must_use]
    pub const fn job(&self) -> ProductionJobId {
        self.job
    }

    #[must_use]
    pub const fn process(&self) -> ProcessId {
        self.process
    }

    #[must_use]
    pub fn routes(&self) -> &[ProcessOutputRoute] {
        &self.routes
    }

    /// Returns merge-aware destination identities for each output stream in stable stream order.
    pub fn landings(&self) -> &[ProcessOutputLanding] {
        &self.landings
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct CompletionPlan {
    revisions: CompletionRevisionPlan,
    availability_dependency_revisions: ProductionAvailabilityDependencyRevisions,
    inventory_deposits: ReservedDepositPlan,
    availability_changes: Vec<ProductionAvailabilityChange>,
    jobs: Vec<ProductionJobId>,
    equipment_outcomes: Vec<EquipmentOperationConditionOutcome>,
    released_energy_outcomes: Vec<ReleasedEnergyTrace>,
    structural_load: Option<ValidatedStockpileStructuralLoad>,
}

impl CompletionPlan {
    fn has_authoritative_effects(&self) -> bool {
        !self.inventory_deposits.is_empty()
            || !self.availability_changes.is_empty()
            || !self.jobs.is_empty()
            || !self.equipment_outcomes.is_empty()
            || !self.released_energy_outcomes.is_empty()
            || self.structural_load.is_some()
    }

    pub(crate) fn availability_changes(&self) -> &[ProductionAvailabilityChange] {
        &self.availability_changes
    }

    /// Projects only the inventory deposits that this production tick will apply, allowing later
    /// same-tick inventory owners to plan against the deterministic post-production inventory.
    pub(crate) fn project_inventory_after_deposits<'inventory>(
        &self,
        inventory: &'inventory crate::inventory::InventoryState,
    ) -> Cow<'inventory, crate::inventory::InventoryState> {
        if self.inventory_deposits.is_empty() {
            return Cow::Borrowed(inventory);
        }
        Cow::Owned(self.inventory_deposits.project(inventory))
    }

    pub(crate) fn equipment_revision_steps(&self) -> u64 {
        u64::from(!self.equipment_outcomes.is_empty())
    }

    pub(crate) fn energy_revision_steps(&self) -> u64 {
        u64::from(!self.released_energy_outcomes.is_empty())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CompletionRevisionPlan {
    expected_production_revision: u64,
    next_production_revision: u64,
    expected_equipment_revision: u64,
    next_equipment_revision: u64,
    expected_energy_revision: u64,
    next_energy_revision: u64,
    expected_structure_revision: u64,
    player_labor_dependencies: Option<PlayerLaborRevisionDependencies>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PlayerLaborRevisionDependencies {
    expected_player_work_revision: u64,
    expected_survival_revision: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CompletionApplication {
    pub(crate) completions: Vec<ProcessCompletion>,
    pub(crate) availability_changes: Vec<ProductionAvailabilityChange>,
}
