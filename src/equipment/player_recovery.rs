//! Player-facing equipment recovery into carried custody.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::registry::Registries;

use super::{
    EquipmentDisassemblyError, EquipmentId, ValidatedEquipmentDisassembly,
    validate_disassemble_equipment,
};

#[derive(Debug)]
pub enum PlayerEquipmentRecoveryError {
    PlayerNotAdmitted,
    Disassembly(EquipmentDisassemblyError),
}

impl Display for PlayerEquipmentRecoveryError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerNotAdmitted => formatter.write_str("player is not admitted"),
            Self::Disassembly(error) => write!(formatter, "equipment recovery failed: {error}"),
        }
    }
}

impl Error for PlayerEquipmentRecoveryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::PlayerNotAdmitted => None,
            Self::Disassembly(error) => Some(error),
        }
    }
}

/// Validates recovery of one visible stationary equipment instance into carried custody.
///
/// The canonical disassembly owner still decides exact recovered material, including wear-driven
/// scrap conversion, equipment locality/occupancy, carried capacity, structural load, survival,
/// and stale-state rejection. Carried equipment must be dropped before recovery, preserving the
/// existing explicit world-custody boundary.
pub fn validate_player_disassemble_equipment(
    registries: &Registries,
    state: &AppState,
    equipment: EquipmentId,
) -> Result<ValidatedEquipmentDisassembly, PlayerEquipmentRecoveryError> {
    let carried = state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .ok_or(PlayerEquipmentRecoveryError::PlayerNotAdmitted)?;
    validate_disassemble_equipment(registries, state, equipment, carried)
        .map_err(PlayerEquipmentRecoveryError::Disassembly)
}

#[cfg(test)]
#[path = "player_recovery_tests.rs"]
mod tests;
