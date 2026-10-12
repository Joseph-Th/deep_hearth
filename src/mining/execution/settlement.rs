//! Reserved mining-output settlement transaction and structural-load commitment.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::inventory::{
    MaterialLotId, ReservedDepositPlan, ReservedDepositPlanError, ReservedDepositRequest,
    STORAGE_AGE_PARTS_PER_TICK, StockpileId, StockpileStoredMassChange,
    StockpileStructuralLoadError, ValidatedStockpileStructuralLoad, apply_reserved_deposits,
    decide_reserved_deposits, validate_reserved_stockpile_structural_load_headroom,
    validate_stockpile_stored_mass_changes,
};
use crate::material::MaterialLotSpec;
use crate::registry::Registries;

use super::super::MiningJobId;
use super::tick::MiningTickPlan;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MiningSettlementError {
    UnknownJob { job: MiningJobId },
    NotAwaitingSettlement { job: MiningJobId },
    LotIdExhausted,
    InventoryRevisionExhausted,
    MiningRevisionExhausted,
    StructuralLoad(StockpileStructuralLoadError),
}

/// Observable result of transferring one completed mining output into inventory custody.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MiningOutputReceipt {
    job: MiningJobId,
    destination: StockpileId,
    output: MaterialLotSpec,
    landed_lot: MaterialLotId,
}

impl MiningOutputReceipt {
    #[must_use]
    pub const fn job(&self) -> MiningJobId {
        self.job
    }

    #[must_use]
    pub const fn destination(&self) -> StockpileId {
        self.destination
    }

    /// Returns the exact mined contribution before any inventory coalescing.
    #[must_use]
    pub const fn output(&self) -> &MaterialLotSpec {
        &self.output
    }

    /// Returns the merge-aware inventory identity that survived admission of the mined output.
    #[must_use]
    pub const fn landed_lot(&self) -> MaterialLotId {
        self.landed_lot
    }
}

impl Display for MiningSettlementError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownJob { job } => write!(formatter, "unknown mining job {}", job.value()),
            Self::NotAwaitingSettlement { job } => {
                write!(
                    formatter,
                    "mining job {} output is not awaiting settlement",
                    job.value()
                )
            }
            Self::LotIdExhausted => {
                formatter.write_str("material lot identifier space is exhausted")
            }
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted")
            }
            Self::MiningRevisionExhausted => {
                formatter.write_str("mining revision space is exhausted")
            }
            Self::StructuralLoad(error) => {
                write!(
                    formatter,
                    "mining output settlement structural load failed: {error}"
                )
            }
        }
    }
}

impl Error for MiningSettlementError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::StructuralLoad(error) => Some(error),
            Self::UnknownJob { .. }
            | Self::NotAwaitingSettlement { .. }
            | Self::LotIdExhausted
            | Self::InventoryRevisionExhausted
            | Self::MiningRevisionExhausted => None,
        }
    }
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MiningSettlementCommitError {
    StaleInventory { expected: u64, actual: u64 },
    StaleMining { expected: u64, actual: u64 },
}

#[cfg(test)]
impl Display for MiningSettlementCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleInventory { expected, actual } => write!(
                formatter,
                "validated mining settlement expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::StaleMining { expected, actual } => write!(
                formatter,
                "validated mining settlement expected mining revision {expected} but current revision is {actual}"
            ),
        }
    }
}

#[cfg(test)]
impl Error for MiningSettlementCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

#[must_use]
pub(crate) struct ValidatedMiningSettlement {
    job: MiningJobId,
    output: MaterialLotSpec,
    expected_mining_revision: u64,
    next_mining_revision: u64,
    inventory: ReservedDepositPlan,
    structural_load: Option<ValidatedStockpileStructuralLoad>,
}

impl ValidatedMiningSettlement {
    #[cfg(test)]
    pub(crate) fn commit(
        self,
        state: &mut AppState,
    ) -> Result<MiningOutputReceipt, MiningSettlementCommitError> {
        if state.inventory().revision() != self.inventory.expected_revision() {
            return Err(MiningSettlementCommitError::StaleInventory {
                expected: self.inventory.expected_revision(),
                actual: state.inventory().revision(),
            });
        }
        self.inventory.assert_matches_state(state.inventory());
        if state.mining().revision() != self.expected_mining_revision {
            return Err(MiningSettlementCommitError::StaleMining {
                expected: self.expected_mining_revision,
                actual: state.mining().revision(),
            });
        }
        state.mining().assert_awaiting_settlement_job_removable(
            self.job,
            self.expected_mining_revision,
            self.next_mining_revision,
        );
        Ok(self.apply_prechecked(state))
    }

    /// Applies a settlement whose complete cross-owner snapshot was already validated for this
    /// exact state by the canonical tick planner.
    pub(crate) fn apply_prechecked(self, state: &mut AppState) -> MiningOutputReceipt {
        self.inventory.assert_matches_state(state.inventory());
        state.mining().assert_awaiting_settlement_job_removable(
            self.job,
            self.expected_mining_revision,
            self.next_mining_revision,
        );
        if let Some(load) = self.structural_load {
            load.commit(state).unwrap_or_else(|error| {
                panic!("prevalidated automatic mining output load failed: {error}")
            });
        }
        let mut receipts = apply_reserved_deposits(state.inventory_state_mut(), self.inventory);
        assert_eq!(
            receipts.len(),
            1,
            "one mining settlement must produce exactly one reserved inventory landing receipt"
        );
        let receipt = receipts
            .pop()
            .unwrap_or_else(|| unreachable!("mining settlement receipt cardinality checked above"));
        let destination = receipt.destination();
        let mut landed_lots = receipt.into_lot_ids();
        assert_eq!(
            landed_lots.len(),
            1,
            "one mining settlement must land exactly one output parcel"
        );
        let landed_lot = landed_lots
            .pop()
            .unwrap_or_else(|| unreachable!("mining settlement landing cardinality checked above"));
        state.mining_state_mut().remove_awaiting_settlement_job(
            self.job,
            self.expected_mining_revision,
            self.next_mining_revision,
        );
        MiningOutputReceipt {
            job: self.job,
            destination,
            output: self.output,
            landed_lot,
        }
    }
}

fn validate_mining_output_settlement(
    registries: &Registries,
    state: &AppState,
    job: MiningJobId,
    admitted_at: SimulationTick,
    expected_mining_revision: u64,
    mining_immediate_steps: u64,
    released_mining_future_demand: u64,
) -> Result<ValidatedMiningSettlement, MiningSettlementError> {
    let record = state
        .mining()
        .get_job(job)
        .ok_or(MiningSettlementError::UnknownJob { job })?;
    let output = record.output().clone();
    let unsettled_ticks = admitted_at
        .checked_duration_since(record.completes_at())
        .unwrap_or_else(|| {
            panic!("validated mining output cannot complete after its inventory admission time")
        });
    let storage_age_parts = u128::from(unsettled_ticks.value())
        .checked_mul(STORAGE_AGE_PARTS_PER_TICK)
        .unwrap_or_else(|| unreachable!("u64 unsettled ticks times storage age parts fits u128"));
    let inventory = decide_reserved_deposits(
        registries,
        state.inventory(),
        record.completes_at(),
        admitted_at,
        vec![ReservedDepositRequest::new(
            record.destination(),
            vec![output.clone()],
            storage_age_parts,
        )],
    )
    .map_err(|error| match error {
        ReservedDepositPlanError::LotIdExhausted => MiningSettlementError::LotIdExhausted,
        ReservedDepositPlanError::RevisionExhausted => {
            MiningSettlementError::InventoryRevisionExhausted
        }
    })?;
    if !state.has_material_lot_id_headroom_from_after_releasing(inventory.next_lot_id(), 0, 1) {
        return Err(MiningSettlementError::LotIdExhausted);
    }
    if !state.can_spend_inventory_revisions_after_releasing(1, 1) {
        return Err(MiningSettlementError::InventoryRevisionExhausted);
    }
    let destination = record.destination();
    let stored_after = inventory
        .stored_mass_after_by_destination(state.inventory())
        .get(&destination)
        .copied()
        .unwrap_or_else(|| panic!("mining settlement deposit plan omitted its destination"));
    let structural_load = validate_stockpile_stored_mass_changes(
        registries,
        state,
        [StockpileStoredMassChange::new(destination, stored_after)],
    )
    .map_err(MiningSettlementError::StructuralLoad)?;
    let released_structure_demand = u64::from(
        state
            .inventory()
            .get_stockpile(destination)
            .unwrap_or_else(|| {
                panic!(
                    "runtime invariant broken: mining settlement references missing destination stockpile {}",
                    destination.value()
                )
            })
            .supported_by()
            .is_some(),
    );
    validate_reserved_stockpile_structural_load_headroom(
        state,
        structural_load.as_ref(),
        released_structure_demand,
    )
    .map_err(MiningSettlementError::StructuralLoad)?;
    if !state.can_spend_mining_revisions_after_releasing(
        mining_immediate_steps,
        released_mining_future_demand,
    ) {
        return Err(MiningSettlementError::MiningRevisionExhausted);
    }
    let next_mining_revision = expected_mining_revision
        .checked_add(1)
        .unwrap_or_else(|| unreachable!("mining settlement consumes its reserved future revision"));
    Ok(ValidatedMiningSettlement {
        job,
        output,
        expected_mining_revision,
        next_mining_revision,
        inventory,
        structural_load,
    })
}

#[cfg(test)]
pub(crate) fn validate_mining_output_settlement_for_test(
    registries: &Registries,
    state: &AppState,
    job: MiningJobId,
) -> Result<ValidatedMiningSettlement, MiningSettlementError> {
    validate_awaiting_mining_output_settlement(registries, state, job, state.tick())
}

pub(crate) fn validate_awaiting_mining_output_settlement(
    registries: &Registries,
    state: &AppState,
    job: MiningJobId,
    admitted_at: SimulationTick,
) -> Result<ValidatedMiningSettlement, MiningSettlementError> {
    let record = state
        .mining()
        .get_job(job)
        .ok_or(MiningSettlementError::UnknownJob { job })?;
    if !record.is_awaiting_settlement() {
        return Err(MiningSettlementError::NotAwaitingSettlement { job });
    }
    validate_mining_output_settlement(
        registries,
        state,
        job,
        admitted_at,
        state.mining().revision(),
        1,
        1,
    )
}

/// Plans same-tick inventory settlement for one mining operation that is due to complete now.
///
/// Mining completion itself does not mutate inventory or structures, so this plan can bind those
/// owners before the tick mutates anything. The returned token expects the mining owner revision
/// immediately after the due job transitions into durable awaiting-settlement custody.
pub(crate) fn validate_due_mining_output_settlement(
    registries: &Registries,
    state: &AppState,
    plan: MiningTickPlan,
    admitted_at: SimulationTick,
) -> Result<ValidatedMiningSettlement, MiningSettlementError> {
    let job = plan.job();
    let record = state
        .mining()
        .get_job(job)
        .ok_or(MiningSettlementError::UnknownJob { job })?;
    if !record.is_working() || record.completes_at() != admitted_at {
        return Err(MiningSettlementError::NotAwaitingSettlement { job });
    }
    let expected_mining_revision = plan
        .expected_mining_revision()
        .checked_add(plan.mining_revision_steps())
        .ok_or(MiningSettlementError::MiningRevisionExhausted)?;
    validate_mining_output_settlement(
        registries,
        state,
        job,
        admitted_at,
        expected_mining_revision,
        2,
        2,
    )
}

#[cfg(test)]
#[path = "settlement_tests.rs"]
mod tests;
