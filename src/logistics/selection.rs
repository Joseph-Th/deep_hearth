//! Persisted quick-access selection over canonical player-carried custody.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::equipment::EquipmentId;
use crate::material::CommodityKey;
use crate::player::{
    PlayerActionGuard, PlayerActionValidationError, StalePlayerSurvival, validate_player_action,
};
use crate::registry::Registries;

use super::{PlayerActiveSelection, PlayerQuickAccessSlot};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerActiveSelectionError {
    PlayerNotAdmitted,
    PlayerDead,
    EquipmentNotCarried { equipment: EquipmentId },
    MaterialNotCarried { commodity: CommodityKey },
    UnsupportedMaterial { commodity: CommodityKey },
    LogisticsRevisionExhausted,
}

impl Display for PlayerActiveSelectionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerNotAdmitted => formatter.write_str("player is not admitted"),
            Self::PlayerDead => formatter.write_str("dead player cannot bind quick-access items"),
            Self::EquipmentNotCarried { equipment } => write!(
                formatter,
                "equipment {} cannot be selected because it is not player-carried",
                equipment.value()
            ),
            Self::MaterialNotCarried { commodity } => write!(
                formatter,
                "material {} form {} cannot be selected because carried custody contains none",
                commodity.material().value(),
                commodity.form().value()
            ),
            Self::UnsupportedMaterial { commodity } => write!(
                formatter,
                "material {} form {} is not an authored commodity",
                commodity.material().value(),
                commodity.form().value()
            ),
            Self::LogisticsRevisionExhausted => {
                formatter.write_str("logistics revision space is exhausted")
            }
        }
    }
}

impl Error for PlayerActiveSelectionError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerActiveSelectionCommitError {
    StaleSurvivalRevision { expected: u64, actual: u64 },
    StaleLogisticsRevision { expected: u64, actual: u64 },
    StaleInventoryRevision { expected: u64, actual: u64 },
    EquipmentNoLongerCarried { equipment: EquipmentId },
    MaterialNoLongerCarried { commodity: CommodityKey },
}

impl Display for PlayerActiveSelectionCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleSurvivalRevision { expected, actual } => write!(
                formatter,
                "active selection expected survival revision {expected} but current revision is {actual}"
            ),
            Self::StaleLogisticsRevision { expected, actual } => write!(
                formatter,
                "active selection expected logistics revision {expected} but current revision is {actual}"
            ),
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "active material selection expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::EquipmentNoLongerCarried { equipment } => write!(
                formatter,
                "equipment {} left player-carried custody before selection commit",
                equipment.value()
            ),
            Self::MaterialNoLongerCarried { commodity } => write!(
                formatter,
                "material {} form {} left player-carried custody before selection commit",
                commodity.material().value(),
                commodity.form().value()
            ),
        }
    }
}

impl Error for PlayerActiveSelectionCommitError {}

#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValidatedPlayerActiveSelection {
    player_action: PlayerActionGuard,
    expected_logistics_revision: u64,
    expected_inventory_revision: Option<u64>,
    next_logistics_revision: Option<u64>,
    slot: PlayerQuickAccessSlot,
    selection: Option<PlayerActiveSelection>,
}

impl ValidatedPlayerActiveSelection {
    pub fn commit(self, state: &mut AppState) -> Result<(), PlayerActiveSelectionCommitError> {
        self.player_action.precheck(state).map_err(
            |StalePlayerSurvival { expected, actual }| {
                PlayerActiveSelectionCommitError::StaleSurvivalRevision { expected, actual }
            },
        )?;
        let actual = state.logistics().revision();
        if actual != self.expected_logistics_revision {
            return Err(PlayerActiveSelectionCommitError::StaleLogisticsRevision {
                expected: self.expected_logistics_revision,
                actual,
            });
        }
        if self.next_logistics_revision.is_some() {
            match self.selection {
                Some(PlayerActiveSelection::Equipment(equipment)) => {
                    if !state.logistics().is_equipment_carried(equipment) {
                        return Err(PlayerActiveSelectionCommitError::EquipmentNoLongerCarried {
                            equipment,
                        });
                    }
                }
                Some(PlayerActiveSelection::Material(commodity)) => {
                    let expected = self.expected_inventory_revision.unwrap_or_else(|| {
                        unreachable!("material selection binds inventory revision")
                    });
                    let actual = state.inventory().revision();
                    if actual != expected {
                        return Err(PlayerActiveSelectionCommitError::StaleInventoryRevision {
                            expected,
                            actual,
                        });
                    }
                    let carried = state
                        .logistics()
                        .player()
                        .map(|player| player.carried_stockpile())
                        .unwrap_or_else(|| unreachable!("validated selection requires player"));
                    let mass = state
                        .inventory()
                        .get_stockpile(carried)
                        .unwrap_or_else(|| unreachable!("admitted player carried stockpile exists"))
                        .get_mass(commodity);
                    if mass.is_zero() {
                        return Err(PlayerActiveSelectionCommitError::MaterialNoLongerCarried {
                            commodity,
                        });
                    }
                }
                None => {}
            }
        }
        if let Some(next_revision) = self.next_logistics_revision {
            state.logistics_state_mut().apply_quick_access_binding(
                self.expected_logistics_revision,
                next_revision,
                self.slot,
                self.selection,
            );
        }
        Ok(())
    }
}

/// Failure while changing which persisted quick-access slot drives direct actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerQuickAccessSelectionError {
    PlayerNotAdmitted,
    PlayerDead,
    LogisticsRevisionExhausted,
}

impl Display for PlayerQuickAccessSelectionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerNotAdmitted => formatter.write_str("player is not admitted"),
            Self::PlayerDead => formatter.write_str("dead player cannot change quick-access slots"),
            Self::LogisticsRevisionExhausted => {
                formatter.write_str("logistics revision space is exhausted")
            }
        }
    }
}

impl Error for PlayerQuickAccessSelectionError {}

/// Commit failure for a previously validated quick-access slot change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerQuickAccessSelectionCommitError {
    StaleSurvivalRevision { expected: u64, actual: u64 },
    StaleLogisticsRevision { expected: u64, actual: u64 },
}

impl Display for PlayerQuickAccessSelectionCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleSurvivalRevision { expected, actual } => write!(
                formatter,
                "quick-access selection expected survival revision {expected} but current revision is {actual}"
            ),
            Self::StaleLogisticsRevision { expected, actual } => write!(
                formatter,
                "quick-access selection expected logistics revision {expected} but current revision is {actual}"
            ),
        }
    }
}

impl Error for PlayerQuickAccessSelectionCommitError {}

#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValidatedPlayerQuickAccessSelection {
    player_action: PlayerActionGuard,
    expected_logistics_revision: u64,
    next_logistics_revision: Option<u64>,
    slot: PlayerQuickAccessSlot,
}

impl ValidatedPlayerQuickAccessSelection {
    pub fn commit(self, state: &mut AppState) -> Result<(), PlayerQuickAccessSelectionCommitError> {
        self.player_action.precheck(state).map_err(
            |StalePlayerSurvival { expected, actual }| {
                PlayerQuickAccessSelectionCommitError::StaleSurvivalRevision { expected, actual }
            },
        )?;
        let actual = state.logistics().revision();
        if actual != self.expected_logistics_revision {
            return Err(
                PlayerQuickAccessSelectionCommitError::StaleLogisticsRevision {
                    expected: self.expected_logistics_revision,
                    actual,
                },
            );
        }
        if let Some(next_revision) = self.next_logistics_revision {
            state.logistics_state_mut().apply_active_quick_access_slot(
                self.expected_logistics_revision,
                next_revision,
                self.slot,
            );
        }
        Ok(())
    }
}

/// Selects one canonical carried equipment instance or semantic carried-material commodity as the
/// player's quick-access context, or clears the active hand with `None`.
///
/// Material selection deliberately does not persist a `MaterialLotId`. Exact lots are resolved by
/// the owning action from current inventory so coalescing/splitting cannot invalidate the selected
/// item presentation. Once selected, a material commodity may remain active after its mass reaches
/// zero so later pickup of the same commodity naturally restores the same quick-access context.
pub fn validate_set_active_selection(
    registries: &Registries,
    state: &AppState,
    selection: Option<PlayerActiveSelection>,
) -> Result<ValidatedPlayerActiveSelection, PlayerActiveSelectionError> {
    let slot = state.logistics().active_quick_access_slot();
    validate_set_quick_access_slot(registries, state, slot, selection)
}

/// Binds one canonical carried selection to a durable quick-access slot.
///
/// Binding an item that is already present in another slot moves that semantic binding rather than
/// duplicating it. Material bindings deliberately survive later depletion; exact lots are resolved
/// only when an action actually consumes or moves matter.
pub fn validate_set_quick_access_slot(
    registries: &Registries,
    state: &AppState,
    slot: PlayerQuickAccessSlot,
    selection: Option<PlayerActiveSelection>,
) -> Result<ValidatedPlayerActiveSelection, PlayerActiveSelectionError> {
    let player_action = validate_player_action(state).map_err(|error| match error {
        PlayerActionValidationError::PlayerNotAdmitted => {
            PlayerActiveSelectionError::PlayerNotAdmitted
        }
        PlayerActionValidationError::PlayerDead => PlayerActiveSelectionError::PlayerDead,
    })?;
    let player = state
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| unreachable!("validated player action requires logistics player"));
    let expected_logistics_revision = state.logistics().revision();
    if state.logistics().quick_access_slot(slot) == selection {
        return Ok(ValidatedPlayerActiveSelection {
            player_action,
            expected_logistics_revision,
            expected_inventory_revision: None,
            next_logistics_revision: None,
            slot,
            selection,
        });
    }
    let expected_inventory_revision = match selection {
        Some(PlayerActiveSelection::Equipment(equipment)) => {
            if !state.logistics().is_equipment_carried(equipment) {
                return Err(PlayerActiveSelectionError::EquipmentNotCarried { equipment });
            }
            None
        }
        Some(PlayerActiveSelection::Material(commodity)) => {
            if !registries.materials().has_commodity(commodity) {
                return Err(PlayerActiveSelectionError::UnsupportedMaterial { commodity });
            }
            let carried = state
                .inventory()
                .get_stockpile(player.carried_stockpile())
                .unwrap_or_else(|| unreachable!("admitted player carried stockpile exists"));
            if carried.get_mass(commodity).is_zero() {
                return Err(PlayerActiveSelectionError::MaterialNotCarried { commodity });
            }
            Some(state.inventory().revision())
        }
        None => None,
    };
    if !state.can_spend_logistics_revisions(1) {
        return Err(PlayerActiveSelectionError::LogisticsRevisionExhausted);
    }
    let next_logistics_revision = Some(
        expected_logistics_revision
            .checked_add(1)
            .unwrap_or_else(|| unreachable!("logistics headroom check includes active selection")),
    );
    Ok(ValidatedPlayerActiveSelection {
        player_action,
        expected_logistics_revision,
        expected_inventory_revision,
        next_logistics_revision,
        slot,
        selection,
    })
}

/// Selects one of the nine persisted quick-access slots, including an empty slot.
pub fn validate_select_quick_access_slot(
    state: &AppState,
    slot: PlayerQuickAccessSlot,
) -> Result<ValidatedPlayerQuickAccessSelection, PlayerQuickAccessSelectionError> {
    let player_action = validate_player_action(state).map_err(|error| match error {
        PlayerActionValidationError::PlayerNotAdmitted => {
            PlayerQuickAccessSelectionError::PlayerNotAdmitted
        }
        PlayerActionValidationError::PlayerDead => PlayerQuickAccessSelectionError::PlayerDead,
    })?;
    let expected_logistics_revision = state.logistics().revision();
    if state.logistics().active_quick_access_slot() == slot {
        return Ok(ValidatedPlayerQuickAccessSelection {
            player_action,
            expected_logistics_revision,
            next_logistics_revision: None,
            slot,
        });
    }
    if !state.can_spend_logistics_revisions(1) {
        return Err(PlayerQuickAccessSelectionError::LogisticsRevisionExhausted);
    }
    let next_logistics_revision = expected_logistics_revision
        .checked_add(1)
        .unwrap_or_else(|| {
            unreachable!("logistics headroom check includes quick-access selection")
        });
    Ok(ValidatedPlayerQuickAccessSelection {
        player_action,
        expected_logistics_revision,
        next_logistics_revision: Some(next_logistics_revision),
        slot,
    })
}

#[cfg(test)]
#[path = "selection_tests.rs"]
mod tests;
