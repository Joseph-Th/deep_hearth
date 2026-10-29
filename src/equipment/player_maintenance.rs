//! Player-facing equipment-service discovery and carried-stock admission.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::maintenance::Condition;
use crate::material::CommodityKey;
use crate::registry::Registries;

use super::{
    EquipmentDefinitionId, EquipmentId, EquipmentMaintenanceError, EquipmentMaintenanceRequest,
    EquipmentMaintenanceResolutionError, ValidatedEquipmentMaintenance, equipment_occupancy,
    resolve_equipment_maintenance, validate_equipment_maintenance,
};

/// One exact-local equipment service need the player can inspect without naming inventory lots.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerEquipmentMaintenanceOption {
    equipment: EquipmentId,
    definition: EquipmentDefinitionId,
    condition_before: Condition,
    target_condition: Condition,
    replacement: CommodityKey,
    required_replacement_mass: Mass,
    spent: CommodityKey,
    duration: TickSpan,
    replacement_ready: bool,
}

impl PlayerEquipmentMaintenanceOption {
    #[must_use]
    pub const fn equipment(self) -> EquipmentId {
        self.equipment
    }

    #[must_use]
    pub const fn definition(self) -> EquipmentDefinitionId {
        self.definition
    }

    #[must_use]
    pub const fn condition_before(self) -> Condition {
        self.condition_before
    }

    #[must_use]
    pub const fn target_condition(self) -> Condition {
        self.target_condition
    }

    #[must_use]
    pub const fn replacement(self) -> CommodityKey {
        self.replacement
    }

    #[must_use]
    pub const fn required_replacement_mass(self) -> Mass {
        self.required_replacement_mass
    }

    #[must_use]
    pub const fn spent(self) -> CommodityKey {
        self.spent
    }

    #[must_use]
    pub const fn duration(self) -> TickSpan {
        self.duration
    }

    #[must_use]
    pub const fn replacement_ready(self) -> bool {
        self.replacement_ready
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerEquipmentMaintenanceCatalogError {
    PlayerNotAdmitted,
}

impl Display for PlayerEquipmentMaintenanceCatalogError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerNotAdmitted => formatter.write_str("player is not admitted"),
        }
    }
}

impl Error for PlayerEquipmentMaintenanceCatalogError {}

/// Lists exact-local idle equipment below its authored service target.
///
/// `replacement_ready` means the canonical resolver can satisfy the service exchange from current
/// carried stock. It does not reserve matter or authorize work; start validation rechecks every
/// owner and condition.
pub fn player_equipment_maintenance_options(
    registries: &Registries,
    state: &AppState,
) -> Result<Vec<PlayerEquipmentMaintenanceOption>, PlayerEquipmentMaintenanceCatalogError> {
    let player = state
        .logistics()
        .player()
        .copied()
        .ok_or(PlayerEquipmentMaintenanceCatalogError::PlayerNotAdmitted)?;
    let carried = player.carried_stockpile();
    let mut options = Vec::new();
    for record in state.available_local_equipment() {
        if equipment_occupancy(state, record.id()).is_some() {
            continue;
        }
        let definition = registries
            .equipment()
            .get_equipment(record.definition())
            .unwrap_or_else(|| {
                panic!("runtime invariant broken: local equipment definition disappeared")
            });
        let Some(profile) = definition.maintenance_profile() else {
            continue;
        };
        if record.condition() >= profile.restored_condition() {
            continue;
        }
        let request = EquipmentMaintenanceRequest::new(record.id(), carried, carried);
        let replacement_ready = match resolve_equipment_maintenance(registries, state, request) {
            Ok(_) => true,
            Err(
                EquipmentMaintenanceResolutionError::InsufficientReplacementMaterial { .. }
                | EquipmentMaintenanceResolutionError::ReplacementMaterialTooHot { .. },
            ) => false,
            Err(EquipmentMaintenanceResolutionError::MaterialSelectionMassOverflow { .. }) => false,
            Err(error) => panic!(
                "runtime invariant broken: local maintainable equipment could not resolve player service option: {error}"
            ),
        };
        options.push(PlayerEquipmentMaintenanceOption {
            equipment: record.id(),
            definition: record.definition(),
            condition_before: record.condition(),
            target_condition: profile.restored_condition(),
            replacement: profile.replacement(),
            required_replacement_mass: profile.required_replacement_mass(record.condition()),
            spent: profile.spent(),
            duration: profile.required_service_duration(record.condition()),
            replacement_ready,
        });
    }
    Ok(options)
}

#[derive(Debug)]
pub enum PlayerEquipmentMaintenanceStartError {
    PlayerNotAdmitted,
    Resolution(EquipmentMaintenanceResolutionError),
    Validation(EquipmentMaintenanceError),
}

impl Display for PlayerEquipmentMaintenanceStartError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerNotAdmitted => formatter.write_str("player is not admitted"),
            Self::Resolution(error) => {
                write!(formatter, "equipment service resolution failed: {error}")
            }
            Self::Validation(error) => {
                write!(formatter, "equipment service validation failed: {error}")
            }
        }
    }
}

impl Error for PlayerEquipmentMaintenanceStartError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::PlayerNotAdmitted => None,
            Self::Resolution(error) => Some(error),
            Self::Validation(error) => Some(error),
        }
    }
}

/// Validates service of one visible local equipment instance from current carried replacement stock.
///
/// Spent material returns to the same carried stockpile. Exact lot selection, equipment locality,
/// material purity, occupancy, survival cost, work timing, and condition transition remain owned by
/// the canonical maintenance resolver/validator.
pub fn validate_start_player_equipment_maintenance(
    registries: &Registries,
    state: &AppState,
    equipment: EquipmentId,
) -> Result<ValidatedEquipmentMaintenance, PlayerEquipmentMaintenanceStartError> {
    let carried = state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .ok_or(PlayerEquipmentMaintenanceStartError::PlayerNotAdmitted)?;
    let resolution = resolve_equipment_maintenance(
        registries,
        state,
        EquipmentMaintenanceRequest::new(equipment, carried, carried),
    )
    .map_err(PlayerEquipmentMaintenanceStartError::Resolution)?;
    validate_equipment_maintenance(registries, state, resolution)
        .map_err(PlayerEquipmentMaintenanceStartError::Validation)
}

#[cfg(test)]
#[path = "player_maintenance_tests.rs"]
mod tests;
