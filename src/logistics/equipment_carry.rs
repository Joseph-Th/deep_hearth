//! Ordinary player pickup/drop of portable equipment under the body carrying envelope.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::equipment::{EquipmentId, equipment_occupancy};
use crate::inventory::{
    StockpileCapacityChangeCommitError, StockpileCapacityChangeError,
    ValidatedStockpileCapacityChange, validate_stockpile_capacity_change,
};
use crate::player::{PlayerActionGuard, PlayerActionValidationError, validate_player_action};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerEquipmentCarryError {
    PlayerNotAdmitted,
    PlayerDead,
    PlayerBusy,
    UnknownEquipment {
        equipment: EquipmentId,
    },
    UnknownDefinition {
        equipment: EquipmentId,
    },
    EquipmentBusy {
        equipment: EquipmentId,
    },
    EquipmentRequiresStructuralSupport {
        equipment: EquipmentId,
    },
    EquipmentMounted {
        equipment: EquipmentId,
    },
    AlreadyCarried {
        equipment: EquipmentId,
    },
    NotCarried {
        equipment: EquipmentId,
    },
    EquipmentNotLocated {
        equipment: EquipmentId,
    },
    EquipmentRemote {
        equipment: EquipmentId,
        equipment_position: VoxelCoord,
        player_position: VoxelCoord,
    },
    EquipmentTooHeavy {
        equipment: EquipmentId,
        mass: Mass,
    },
    CapacityExceeded {
        equipment: EquipmentId,
        capacity_after_pickup: Mass,
        committed: Mass,
    },
    InventoryRevisionExhausted,
    LogisticsRevisionExhausted,
}

impl Display for PlayerEquipmentCarryError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerNotAdmitted => formatter.write_str("player is not admitted"),
            Self::PlayerDead => formatter.write_str("dead player cannot move equipment"),
            Self::PlayerBusy => {
                formatter.write_str("player cannot move equipment while work is active")
            }
            Self::UnknownEquipment { equipment } => {
                write!(formatter, "unknown equipment {}", equipment.value())
            }
            Self::UnknownDefinition { equipment } => write!(
                formatter,
                "equipment {} references an unknown definition",
                equipment.value()
            ),
            Self::EquipmentBusy { equipment } => write!(
                formatter,
                "equipment {} is occupied by active work",
                equipment.value()
            ),
            Self::EquipmentRequiresStructuralSupport { equipment } => write!(
                formatter,
                "equipment {} is installed machinery and cannot be player-carried",
                equipment.value()
            ),
            Self::EquipmentMounted { equipment } => write!(
                formatter,
                "equipment {} must be unmounted before pickup",
                equipment.value()
            ),
            Self::AlreadyCarried { equipment } => {
                write!(
                    formatter,
                    "equipment {} is already carried",
                    equipment.value()
                )
            }
            Self::NotCarried { equipment } => {
                write!(formatter, "equipment {} is not carried", equipment.value())
            }
            Self::EquipmentNotLocated { equipment } => write!(
                formatter,
                "equipment {} has no logistics-owned stationary location",
                equipment.value()
            ),
            Self::EquipmentRemote {
                equipment,
                equipment_position,
                player_position,
            } => write!(
                formatter,
                "equipment {} is at ({},{},{}) but player is at ({},{},{})",
                equipment.value(),
                equipment_position.x(),
                equipment_position.y(),
                equipment_position.z(),
                player_position.x(),
                player_position.y(),
                player_position.z()
            ),
            Self::EquipmentTooHeavy { equipment, mass } => write!(
                formatter,
                "equipment {} mass {} mg leaves no carried-material capacity",
                equipment.value(),
                mass.milligrams()
            ),
            Self::CapacityExceeded {
                equipment,
                capacity_after_pickup,
                committed,
            } => write!(
                formatter,
                "equipment {} pickup would leave {} mg material capacity below {} mg already committed",
                equipment.value(),
                capacity_after_pickup.milligrams(),
                committed.milligrams()
            ),
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted")
            }
            Self::LogisticsRevisionExhausted => {
                formatter.write_str("logistics revision space is exhausted")
            }
        }
    }
}

impl Error for PlayerEquipmentCarryError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerEquipmentCarryCommitError {
    StaleSurvivalRevision { expected: u64, actual: u64 },
    StaleLogisticsRevision { expected: u64, actual: u64 },
    StaleEquipmentRevision { expected: u64, actual: u64 },
    StaleInventoryRevision { expected: u64, actual: u64 },
    CarriedCapacityChanged { expected: Mass, actual: Mass },
    PlayerBecameBusy,
    EquipmentBecameBusy { equipment: EquipmentId },
}

impl Display for PlayerEquipmentCarryCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleSurvivalRevision { expected, actual } => write!(
                formatter,
                "equipment carry expected survival revision {expected} but current revision is {actual}"
            ),
            Self::StaleLogisticsRevision { expected, actual } => write!(
                formatter,
                "equipment carry expected logistics revision {expected} but current revision is {actual}"
            ),
            Self::StaleEquipmentRevision { expected, actual } => write!(
                formatter,
                "equipment carry expected equipment revision {expected} but current revision is {actual}"
            ),
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "equipment carry expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::CarriedCapacityChanged { expected, actual } => write!(
                formatter,
                "player carried material capacity changed from validated {} mg to {} mg",
                expected.milligrams(),
                actual.milligrams()
            ),
            Self::PlayerBecameBusy => {
                formatter.write_str("player work started after equipment carry validation")
            }
            Self::EquipmentBecameBusy { equipment } => write!(
                formatter,
                "equipment {} became occupied after carry validation",
                equipment.value()
            ),
        }
    }
}

impl Error for PlayerEquipmentCarryCommitError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EquipmentCarryDirection {
    Pickup { position: VoxelCoord },
    Drop { position: VoxelCoord },
}

#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValidatedPlayerEquipmentCarry {
    player_action: PlayerActionGuard,
    equipment: EquipmentId,
    expected_equipment_revision: u64,
    expected_logistics_revision: u64,
    next_logistics_revision: u64,
    direction: EquipmentCarryDirection,
    capacity: ValidatedStockpileCapacityChange,
}

impl ValidatedPlayerEquipmentCarry {
    pub fn commit(self, state: &mut AppState) -> Result<(), PlayerEquipmentCarryCommitError> {
        self.player_action.precheck(state).map_err(|stale| {
            PlayerEquipmentCarryCommitError::StaleSurvivalRevision {
                expected: stale.expected,
                actual: stale.actual,
            }
        })?;
        let actual_logistics = state.logistics().revision();
        if actual_logistics != self.expected_logistics_revision {
            return Err(PlayerEquipmentCarryCommitError::StaleLogisticsRevision {
                expected: self.expected_logistics_revision,
                actual: actual_logistics,
            });
        }
        let actual_equipment = state.equipment().revision();
        if actual_equipment != self.expected_equipment_revision {
            return Err(PlayerEquipmentCarryCommitError::StaleEquipmentRevision {
                expected: self.expected_equipment_revision,
                actual: actual_equipment,
            });
        }
        if state.player_work().active().is_some() {
            return Err(PlayerEquipmentCarryCommitError::PlayerBecameBusy);
        }
        if equipment_occupancy(state, self.equipment).is_some() {
            return Err(PlayerEquipmentCarryCommitError::EquipmentBecameBusy {
                equipment: self.equipment,
            });
        }
        self.capacity.precheck(state).map_err(|error| match error {
            StockpileCapacityChangeCommitError::StaleInventoryRevision { expected, actual } => {
                PlayerEquipmentCarryCommitError::StaleInventoryRevision { expected, actual }
            }
            StockpileCapacityChangeCommitError::CapacityChanged {
                expected, actual, ..
            } => PlayerEquipmentCarryCommitError::CarriedCapacityChanged { expected, actual },
        })?;
        self.capacity.apply(state);
        match self.direction {
            EquipmentCarryDirection::Pickup { position } => {
                state.logistics_state_mut().apply_equipment_pickup(
                    self.expected_logistics_revision,
                    self.next_logistics_revision,
                    self.equipment,
                    position,
                );
            }
            EquipmentCarryDirection::Drop { position } => {
                state.logistics_state_mut().apply_equipment_drop(
                    self.expected_logistics_revision,
                    self.next_logistics_revision,
                    self.equipment,
                    position,
                );
            }
        }
        Ok(())
    }
}

fn player_action(state: &AppState) -> Result<PlayerActionGuard, PlayerEquipmentCarryError> {
    validate_player_action(state).map_err(|error| match error {
        PlayerActionValidationError::PlayerNotAdmitted => {
            PlayerEquipmentCarryError::PlayerNotAdmitted
        }
        PlayerActionValidationError::PlayerDead => PlayerEquipmentCarryError::PlayerDead,
    })
}

fn common_validation(
    registries: &Registries,
    state: &AppState,
    equipment: EquipmentId,
) -> Result<(PlayerActionGuard, Mass), PlayerEquipmentCarryError> {
    let action = player_action(state)?;
    if state.player_work().active().is_some() {
        return Err(PlayerEquipmentCarryError::PlayerBusy);
    }
    let record = state
        .equipment()
        .get_equipment(equipment)
        .ok_or(PlayerEquipmentCarryError::UnknownEquipment { equipment })?;
    let definition = registries
        .equipment()
        .get_equipment(record.definition())
        .ok_or(PlayerEquipmentCarryError::UnknownDefinition { equipment })?;
    if definition.requires_structural_support() {
        return Err(PlayerEquipmentCarryError::EquipmentRequiresStructuralSupport { equipment });
    }
    if record.supported_by().is_some() {
        return Err(PlayerEquipmentCarryError::EquipmentMounted { equipment });
    }
    if equipment_occupancy(state, equipment).is_some() {
        return Err(PlayerEquipmentCarryError::EquipmentBusy { equipment });
    }
    Ok((action, record.embodied_mass()))
}

fn revisions(state: &AppState) -> Result<(u64, u64), PlayerEquipmentCarryError> {
    if !state.can_spend_inventory_revisions(1) {
        return Err(PlayerEquipmentCarryError::InventoryRevisionExhausted);
    }
    if !state.can_spend_logistics_revisions(1) {
        return Err(PlayerEquipmentCarryError::LogisticsRevisionExhausted);
    }
    let expected = state.logistics().revision();
    Ok((
        expected,
        expected
            .checked_add(1)
            .unwrap_or_else(|| unreachable!("logistics headroom check includes equipment carry")),
    ))
}

pub fn validate_pickup_equipment(
    registries: &Registries,
    state: &AppState,
    equipment: EquipmentId,
) -> Result<ValidatedPlayerEquipmentCarry, PlayerEquipmentCarryError> {
    let (action, mass) = common_validation(registries, state, equipment)?;
    if state.logistics().is_equipment_carried(equipment) {
        return Err(PlayerEquipmentCarryError::AlreadyCarried { equipment });
    }
    let player = state
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| unreachable!("ordinary player action requires logistics"));
    let position = state
        .logistics()
        .stationary_equipment_position(equipment)
        .ok_or(PlayerEquipmentCarryError::EquipmentNotLocated { equipment })?;
    if position != player.position() {
        return Err(PlayerEquipmentCarryError::EquipmentRemote {
            equipment,
            equipment_position: position,
            player_position: player.position(),
        });
    }
    let carried = state
        .inventory()
        .get_stockpile(player.carried_stockpile())
        .unwrap_or_else(|| unreachable!("admitted player carried stockpile exists"));
    let next_capacity = carried
        .capacity()
        .checked_sub(mass)
        .ok_or(PlayerEquipmentCarryError::EquipmentTooHeavy { equipment, mass })?;
    if next_capacity.is_zero() {
        return Err(PlayerEquipmentCarryError::EquipmentTooHeavy { equipment, mass });
    }
    let capacity = validate_stockpile_capacity_change(
        state.inventory(),
        player.carried_stockpile(),
        next_capacity,
    )
    .map_err(|error| match error {
        StockpileCapacityChangeError::CapacityBelowCommitted {
            capacity,
            committed,
            ..
        } => PlayerEquipmentCarryError::CapacityExceeded {
            equipment,
            capacity_after_pickup: capacity,
            committed,
        },
        StockpileCapacityChangeError::RevisionExhausted => {
            PlayerEquipmentCarryError::InventoryRevisionExhausted
        }
        StockpileCapacityChangeError::ZeroCapacity { .. } => {
            PlayerEquipmentCarryError::EquipmentTooHeavy { equipment, mass }
        }
        StockpileCapacityChangeError::UnknownStockpile { .. } => {
            unreachable!("admitted player carried stockpile exists")
        }
    })?;
    let (expected_logistics_revision, next_logistics_revision) = revisions(state)?;
    Ok(ValidatedPlayerEquipmentCarry {
        player_action: action,
        equipment,
        expected_equipment_revision: state.equipment().revision(),
        expected_logistics_revision,
        next_logistics_revision,
        direction: EquipmentCarryDirection::Pickup { position },
        capacity,
    })
}

pub fn validate_drop_equipment(
    registries: &Registries,
    state: &AppState,
    equipment: EquipmentId,
) -> Result<ValidatedPlayerEquipmentCarry, PlayerEquipmentCarryError> {
    let (action, mass) = common_validation(registries, state, equipment)?;
    if !state.logistics().is_equipment_carried(equipment) {
        return Err(PlayerEquipmentCarryError::NotCarried { equipment });
    }
    let player = state
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| unreachable!("ordinary player action requires logistics"));
    let carried = state
        .inventory()
        .get_stockpile(player.carried_stockpile())
        .unwrap_or_else(|| unreachable!("admitted player carried stockpile exists"));
    let next_capacity = carried.capacity().checked_add(mass).unwrap_or_else(|| {
        panic!("dropping carried equipment cannot overflow authored body capacity")
    });
    let capacity = validate_stockpile_capacity_change(
        state.inventory(),
        player.carried_stockpile(),
        next_capacity,
    )
    .map_err(|error| match error {
        StockpileCapacityChangeError::RevisionExhausted => {
            PlayerEquipmentCarryError::InventoryRevisionExhausted
        }
        StockpileCapacityChangeError::UnknownStockpile { .. }
        | StockpileCapacityChangeError::ZeroCapacity { .. }
        | StockpileCapacityChangeError::CapacityBelowCommitted { .. } => {
            unreachable!("dropping equipment only restores body material capacity")
        }
    })?;
    let (expected_logistics_revision, next_logistics_revision) = revisions(state)?;
    Ok(ValidatedPlayerEquipmentCarry {
        player_action: action,
        equipment,
        expected_equipment_revision: state.equipment().revision(),
        expected_logistics_revision,
        next_logistics_revision,
        direction: EquipmentCarryDirection::Drop {
            position: player.position(),
        },
        capacity,
    })
}

#[cfg(test)]
#[path = "equipment_carry_tests.rs"]
mod tests;
