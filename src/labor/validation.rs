//! Exhaustive persistence validation for cross-owner player labor references.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::crafting::CraftingRegistry;

use super::{PlayerWork, PlayerWorkState};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerWorkValidationError {
    WorkWithoutPlayer,
    ManualCraftJobMissing,
    ManualCraftProcessMismatch,
    MiningJobMissing,
    MiningJobNotWorking,
    ManualCraftMissingWork,
    MultiplePlayerJobs,
    MiningMissingWork,
}

impl Display for PlayerWorkValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WorkWithoutPlayer => formatter.write_str("player work exists without a player"),
            Self::ManualCraftJobMissing => {
                formatter.write_str("player work references missing manual craft job")
            }
            Self::ManualCraftProcessMismatch => formatter
                .write_str("player work references a production job that is not manual crafting"),
            Self::MiningJobMissing => {
                formatter.write_str("player work references missing mining job")
            }
            Self::MiningJobNotWorking => {
                formatter.write_str("player work references mining that is no longer active")
            }
            Self::ManualCraftMissingWork => {
                formatter.write_str("active manual crafting job does not own player labor")
            }
            Self::MultiplePlayerJobs => {
                formatter.write_str("more than one active job requires exclusive player labor")
            }
            Self::MiningMissingWork => {
                formatter.write_str("working mining job does not own player labor")
            }
        }
    }
}

impl Error for PlayerWorkValidationError {}

pub(crate) fn validate_loaded_player_work(
    crafting: &CraftingRegistry,
    state: &AppState,
    work_state: &PlayerWorkState,
) -> Result<(), PlayerWorkValidationError> {
    let manual_jobs = state
        .production()
        .jobs()
        .filter(|job| crafting.get_manual(job.process()).is_some())
        .map(|job| job.id())
        .collect::<Vec<_>>();
    let mining_jobs = state
        .mining()
        .jobs()
        .filter(|job| job.is_working())
        .map(|job| job.id())
        .collect::<Vec<_>>();
    if manual_jobs.len() + mining_jobs.len() > 1 {
        return Err(PlayerWorkValidationError::MultiplePlayerJobs);
    }
    let Some(work) = work_state.active() else {
        if !manual_jobs.is_empty() {
            return Err(PlayerWorkValidationError::ManualCraftMissingWork);
        }
        if !mining_jobs.is_empty() {
            return Err(PlayerWorkValidationError::MiningMissingWork);
        }
        return Ok(());
    };
    if state.survival().player().is_none() {
        return Err(PlayerWorkValidationError::WorkWithoutPlayer);
    }
    match work {
        PlayerWork::ManualCraft { job } => {
            let Some(record) = state.production().get_job(job) else {
                return Err(PlayerWorkValidationError::ManualCraftJobMissing);
            };
            if crafting.get_manual(record.process()).is_none() {
                return Err(PlayerWorkValidationError::ManualCraftProcessMismatch);
            }
            if manual_jobs.as_slice() != [job] {
                return Err(PlayerWorkValidationError::ManualCraftMissingWork);
            }
        }
        PlayerWork::Mining { job } => {
            let Some(record) = state.mining().get_job(job) else {
                return Err(PlayerWorkValidationError::MiningJobMissing);
            };
            if !record.is_working() {
                return Err(PlayerWorkValidationError::MiningJobNotWorking);
            }
            if mining_jobs.as_slice() != [job] {
                return Err(PlayerWorkValidationError::MiningMissingWork);
            }
        }
    }
    Ok(())
}
