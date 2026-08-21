//! Reserved mining-output claim transaction and structural-load commitment.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::inventory::{
    ReservedDepositPlan, ReservedDepositPlanError, ReservedDepositRequest, StockpileId,
    StockpileStoredMassChange, StockpileStructuralLoadError, ValidatedStockpileStructuralLoad,
    apply_reserved_deposits, decide_reserved_deposits, validate_stockpile_stored_mass_changes,
};
use crate::registry::Registries;
use crate::structural::StructuralCommitError;

use super::super::MiningJobId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MiningClaimError {
    UnknownJob { job: MiningJobId },
    NotReady { job: MiningJobId },
    LotIdExhausted,
    InventoryRevisionExhausted,
    MiningRevisionExhausted,
    DestinationMassOverflow { stockpile: StockpileId },
    StructuralLoad(StockpileStructuralLoadError),
}

impl Display for MiningClaimError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownJob { job } => write!(formatter, "unknown mining job {}", job.value()),
            Self::NotReady { job } => {
                write!(
                    formatter,
                    "mining job {} output is not ready to claim",
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
            Self::DestinationMassOverflow { stockpile } => write!(
                formatter,
                "claimed mining output overflows destination stockpile {} mass",
                stockpile.value()
            ),
            Self::StructuralLoad(error) => {
                write!(
                    formatter,
                    "claimed mining output structural load failed: {error}"
                )
            }
        }
    }
}

impl Error for MiningClaimError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::StructuralLoad(error) => Some(error),
            Self::UnknownJob { .. }
            | Self::NotReady { .. }
            | Self::LotIdExhausted
            | Self::InventoryRevisionExhausted
            | Self::MiningRevisionExhausted
            | Self::DestinationMassOverflow { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MiningClaimCommitError {
    StaleInventory { expected: u64, actual: u64 },
    StaleMining { expected: u64, actual: u64 },
    Structure(StructuralCommitError),
}

impl Display for MiningClaimCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleInventory { expected, actual } => write!(
                formatter,
                "validated mining claim expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::StaleMining { expected, actual } => write!(
                formatter,
                "validated mining claim expected mining revision {expected} but current revision is {actual}"
            ),
            Self::Structure(error) => {
                write!(
                    formatter,
                    "validated mining claim structural commit failed: {error}"
                )
            }
        }
    }
}

impl Error for MiningClaimCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StaleInventory { .. } | Self::StaleMining { .. } => None,
        }
    }
}

#[must_use]
pub struct ValidatedMiningClaim {
    job: MiningJobId,
    expected_mining_revision: u64,
    next_mining_revision: u64,
    inventory: ReservedDepositPlan,
    structural_load: Option<ValidatedStockpileStructuralLoad>,
}

impl ValidatedMiningClaim {
    pub fn commit(self, state: &mut AppState) -> Result<(), MiningClaimCommitError> {
        if state.inventory().revision() != self.inventory.expected_revision() {
            return Err(MiningClaimCommitError::StaleInventory {
                expected: self.inventory.expected_revision(),
                actual: state.inventory().revision(),
            });
        }
        if state.mining().revision() != self.expected_mining_revision {
            return Err(MiningClaimCommitError::StaleMining {
                expected: self.expected_mining_revision,
                actual: state.mining().revision(),
            });
        }
        if let Some(load) = self.structural_load {
            load.commit(state)
                .map_err(MiningClaimCommitError::Structure)?;
        }
        apply_reserved_deposits(state.inventory_state_mut(), self.inventory);
        state.mining_state_mut().remove_ready_job(
            self.job,
            self.expected_mining_revision,
            self.next_mining_revision,
        );
        Ok(())
    }
}

pub fn validate_claim_mining_output(
    registries: &Registries,
    state: &AppState,
    job: MiningJobId,
) -> Result<ValidatedMiningClaim, MiningClaimError> {
    let record = state
        .mining()
        .get_job(job)
        .ok_or(MiningClaimError::UnknownJob { job })?;
    let ready_at = record
        .ready_at()
        .ok_or(MiningClaimError::NotReady { job })?;
    let mass = record.output().mass();
    let inventory = decide_reserved_deposits(
        registries,
        state.inventory(),
        ready_at,
        vec![ReservedDepositRequest::new(
            record.destination(),
            vec![record.output().clone()],
            mass,
        )],
    )
    .map_err(|error| match error {
        ReservedDepositPlanError::LotIdExhausted => MiningClaimError::LotIdExhausted,
        ReservedDepositPlanError::RevisionExhausted => MiningClaimError::InventoryRevisionExhausted,
    })?;
    let destination = state
        .inventory()
        .get_stockpile(record.destination())
        .ok_or(MiningClaimError::DestinationMassOverflow {
            stockpile: record.destination(),
        })?;
    let stored_after = destination.stored_mass().checked_add(mass).ok_or(
        MiningClaimError::DestinationMassOverflow {
            stockpile: record.destination(),
        },
    )?;
    let structural_load = validate_stockpile_stored_mass_changes(
        registries,
        state,
        [StockpileStoredMassChange::new_committed_inbound(
            record.destination(),
            stored_after,
        )],
    )
    .map_err(MiningClaimError::StructuralLoad)?;
    let expected_mining_revision = state.mining().revision();
    let next_mining_revision = expected_mining_revision
        .checked_add(1)
        .ok_or(MiningClaimError::MiningRevisionExhausted)?;
    Ok(ValidatedMiningClaim {
        job,
        expected_mining_revision,
        next_mining_revision,
        inventory,
        structural_load,
    })
}
