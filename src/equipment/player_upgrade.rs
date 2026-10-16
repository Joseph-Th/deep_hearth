//! Actor-facing additive equipment upgrade choices over local equipment and material custody.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::{ConsumptionSelectionError, StockpileId, validate_consumption_selection};
use crate::logistics::{PlayerStockpileAccessError, validate_player_stockpile_access};
use crate::material::MaterialInputSpec;
use crate::registry::Registries;

use super::{
    EquipmentDefinitionId, EquipmentId, EquipmentUpgradeError, ValidatedEquipmentUpgrade,
    equipment_occupancy, validate_upgrade_equipment,
};

/// One additive upgrade currently visible for exact-local idle equipment.
///
/// `material_ready` is a read-side projection only. Upgrade start revalidates equipment state,
/// locality, occupancy, source matter, structural consequences, survival, and owner revisions.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerEquipmentUpgradeOption {
    equipment: EquipmentId,
    base_definition: EquipmentDefinitionId,
    target_definition: EquipmentDefinitionId,
    target_name: String,
    source: StockpileId,
    target_mass: Mass,
    additions: Vec<MaterialInputSpec>,
    material_ready: bool,
}

impl PlayerEquipmentUpgradeOption {
    #[must_use]
    pub const fn equipment(&self) -> EquipmentId {
        self.equipment
    }

    #[must_use]
    pub const fn base_definition(&self) -> EquipmentDefinitionId {
        self.base_definition
    }

    #[must_use]
    pub const fn target_definition(&self) -> EquipmentDefinitionId {
        self.target_definition
    }

    #[must_use]
    pub fn target_name(&self) -> &str {
        &self.target_name
    }

    #[must_use]
    pub const fn source(&self) -> StockpileId {
        self.source
    }

    #[must_use]
    pub const fn target_mass(&self) -> Mass {
        self.target_mass
    }

    #[must_use]
    pub fn additions(&self) -> &[MaterialInputSpec] {
        &self.additions
    }

    #[must_use]
    pub const fn material_ready(&self) -> bool {
        self.material_ready
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerEquipmentUpgradeCatalogError {
    PlayerNotAdmitted,
    SourceAccess(PlayerStockpileAccessError),
    UnknownSource { stockpile: StockpileId },
    SourceMassOverflow { stockpile: StockpileId },
}

impl Display for PlayerEquipmentUpgradeCatalogError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerNotAdmitted => formatter.write_str("player is not admitted"),
            Self::SourceAccess(error) => {
                write!(formatter, "equipment upgrade source access failed: {error}")
            }
            Self::UnknownSource { stockpile } => write!(
                formatter,
                "equipment upgrade source stockpile {} does not exist",
                stockpile.value()
            ),
            Self::SourceMassOverflow { stockpile } => write!(
                formatter,
                "equipment upgrade source stockpile {} mass accounting overflowed",
                stockpile.value()
            ),
        }
    }
}

impl Error for PlayerEquipmentUpgradeCatalogError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::SourceAccess(error) => Some(error),
            Self::PlayerNotAdmitted
            | Self::UnknownSource { .. }
            | Self::SourceMassOverflow { .. } => None,
        }
    }
}

/// Lists upgrades for local stationary idle equipment against one accessible material source.
pub fn player_equipment_upgrade_options(
    registries: &Registries,
    state: &AppState,
    source: StockpileId,
) -> Result<Vec<PlayerEquipmentUpgradeOption>, PlayerEquipmentUpgradeCatalogError> {
    if state.logistics().player().is_none() {
        return Err(PlayerEquipmentUpgradeCatalogError::PlayerNotAdmitted);
    }
    if state.inventory().get_stockpile(source).is_none() {
        return Err(PlayerEquipmentUpgradeCatalogError::UnknownSource { stockpile: source });
    }
    validate_player_stockpile_access(state, source)
        .map_err(PlayerEquipmentUpgradeCatalogError::SourceAccess)?;

    let mut options = Vec::new();
    for record in state.available_local_equipment() {
        if state.logistics().is_equipment_carried(record.id())
            || record.supported_by().is_some()
            || equipment_occupancy(state, record.id()).is_some()
        {
            continue;
        }
        for target in registries.equipment().definitions() {
            let Some(upgrade) = target
                .upgrade_profile()
                .filter(|upgrade| upgrade.from() == record.definition())
            else {
                continue;
            };
            let material_ready = match validate_consumption_selection(
                state.inventory(),
                source,
                upgrade.additions().inputs(),
            ) {
                Ok(_) => true,
                Err(ConsumptionSelectionError::InsufficientMass { .. }) => false,
                Err(ConsumptionSelectionError::UnknownStockpile { stockpile }) => {
                    return Err(PlayerEquipmentUpgradeCatalogError::UnknownSource { stockpile });
                }
                Err(ConsumptionSelectionError::MassOverflow { stockpile }) => {
                    return Err(PlayerEquipmentUpgradeCatalogError::SourceMassOverflow {
                        stockpile,
                    });
                }
            };
            options.push(PlayerEquipmentUpgradeOption {
                equipment: record.id(),
                base_definition: record.definition(),
                target_definition: target.id(),
                target_name: target.name().to_owned(),
                source,
                target_mass: target.mass(),
                additions: upgrade.additions().inputs().to_vec(),
                material_ready,
            });
        }
    }
    Ok(options)
}

/// Revalidates and starts one previously observed equipment upgrade row.
pub fn validate_upgrade_player_equipment_option(
    registries: &Registries,
    state: &AppState,
    option: &PlayerEquipmentUpgradeOption,
) -> Result<ValidatedEquipmentUpgrade, EquipmentUpgradeError> {
    validate_upgrade_equipment(
        registries,
        state,
        option.equipment,
        option.target_definition,
        option.source,
    )
}

#[cfg(test)]
#[path = "player_upgrade_tests.rs"]
mod tests;
