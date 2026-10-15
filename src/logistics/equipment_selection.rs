//! Persisted active-tool selection over canonical player-carried equipment custody.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::equipment::EquipmentId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerEquipmentSelectionError {
    PlayerNotAdmitted,
    EquipmentNotCarried { equipment: EquipmentId },
    LogisticsRevisionExhausted,
}

impl Display for PlayerEquipmentSelectionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerNotAdmitted => formatter.write_str("player is not admitted"),
            Self::EquipmentNotCarried { equipment } => write!(
                formatter,
                "equipment {} cannot be selected because it is not player-carried",
                equipment.value()
            ),
            Self::LogisticsRevisionExhausted => {
                formatter.write_str("logistics revision space is exhausted")
            }
        }
    }
}

impl Error for PlayerEquipmentSelectionError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerEquipmentSelectionCommitError {
    StaleLogisticsRevision { expected: u64, actual: u64 },
    EquipmentNoLongerCarried { equipment: EquipmentId },
}

impl Display for PlayerEquipmentSelectionCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleLogisticsRevision { expected, actual } => write!(
                formatter,
                "active-equipment selection expected logistics revision {expected} but current revision is {actual}"
            ),
            Self::EquipmentNoLongerCarried { equipment } => write!(
                formatter,
                "equipment {} left player-carried custody before selection commit",
                equipment.value()
            ),
        }
    }
}

impl Error for PlayerEquipmentSelectionCommitError {}

#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValidatedPlayerEquipmentSelection {
    expected_revision: u64,
    next_revision: Option<u64>,
    equipment: Option<EquipmentId>,
}

impl ValidatedPlayerEquipmentSelection {
    pub fn commit(self, state: &mut AppState) -> Result<(), PlayerEquipmentSelectionCommitError> {
        let actual = state.logistics().revision();
        if actual != self.expected_revision {
            return Err(
                PlayerEquipmentSelectionCommitError::StaleLogisticsRevision {
                    expected: self.expected_revision,
                    actual,
                },
            );
        }
        if let Some(equipment) = self.equipment
            && !state.logistics().is_equipment_carried(equipment)
        {
            return Err(
                PlayerEquipmentSelectionCommitError::EquipmentNoLongerCarried { equipment },
            );
        }
        if let Some(next_revision) = self.next_revision {
            state
                .logistics_state_mut()
                .apply_active_equipment_selection(
                    self.expected_revision,
                    next_revision,
                    self.equipment,
                );
        }
        Ok(())
    }
}

/// Selects one already-carried equipment instance as the default direct-action tool, or clears the
/// active hand with `None`. Re-selecting the current value is an idempotent no-op.
pub fn validate_set_active_equipment(
    state: &AppState,
    equipment: Option<EquipmentId>,
) -> Result<ValidatedPlayerEquipmentSelection, PlayerEquipmentSelectionError> {
    if state.logistics().player().is_none() {
        return Err(PlayerEquipmentSelectionError::PlayerNotAdmitted);
    }
    if let Some(equipment) = equipment
        && !state.logistics().is_equipment_carried(equipment)
    {
        return Err(PlayerEquipmentSelectionError::EquipmentNotCarried { equipment });
    }
    let expected_revision = state.logistics().revision();
    let next_revision = if state.logistics().active_equipment() == equipment {
        None
    } else {
        if !state.can_spend_logistics_revisions(1) {
            return Err(PlayerEquipmentSelectionError::LogisticsRevisionExhausted);
        }
        Some(
            expected_revision
                .checked_add(1)
                .unwrap_or_else(|| unreachable!("logistics headroom check includes selection")),
        )
    };
    Ok(ValidatedPlayerEquipmentSelection {
        expected_revision,
        next_revision,
        equipment,
    })
}

#[cfg(test)]
#[path = "equipment_selection_tests.rs"]
mod tests;
