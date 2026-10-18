//! Player-facing discovery of additive upgrades for nearby empty energy stores.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Power};
use crate::core::state::AppState;
use crate::inventory::{ConsumptionSelectionError, StockpileId, validate_consumption_selection};
use crate::logistics::{PlayerStockpileAccessError, validate_player_stockpile_access};
use crate::material::MaterialInputSpec;
use crate::registry::Registries;

use super::{
    EnergyStoreDefinitionId, EnergyStoreId, EnergyStoreUpgradeError, ValidatedEnergyStoreUpgrade,
    energy_store_occupancy, validate_upgrade_energy_store,
};

/// An observable, non-reserving option. Exact ownership, availability, and material readiness
/// are rechecked by the canonical upgrade validator when the player commits.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerEnergyStoreUpgradeOption {
    store: EnergyStoreId,
    target: EnergyStoreDefinitionId,
    name: String,
    source: StockpileId,
    additions: Vec<MaterialInputSpec>,
    capacity: Energy,
    max_input_power: Power,
    max_output_power: Power,
    material_ready: bool,
}

impl PlayerEnergyStoreUpgradeOption {
    #[must_use]
    pub const fn store(&self) -> EnergyStoreId {
        self.store
    }
    #[must_use]
    pub const fn target(&self) -> EnergyStoreDefinitionId {
        self.target
    }
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    #[must_use]
    pub const fn source(&self) -> StockpileId {
        self.source
    }
    #[must_use]
    pub fn additions(&self) -> &[MaterialInputSpec] {
        &self.additions
    }
    #[must_use]
    pub const fn capacity(&self) -> Energy {
        self.capacity
    }
    #[must_use]
    pub const fn max_input_power(&self) -> Power {
        self.max_input_power
    }
    #[must_use]
    pub const fn max_output_power(&self) -> Power {
        self.max_output_power
    }
    #[must_use]
    pub const fn material_ready(&self) -> bool {
        self.material_ready
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerEnergyStoreUpgradeCatalogError {
    PlayerNotAdmitted,
    SourceAccess(PlayerStockpileAccessError),
    UnknownSource { stockpile: StockpileId },
    SourceMassOverflow { stockpile: StockpileId },
}

impl Display for PlayerEnergyStoreUpgradeCatalogError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerNotAdmitted => f.write_str("player is not admitted"),
            Self::SourceAccess(error) => {
                write!(f, "energy upgrade material access failed: {error}")
            }
            Self::UnknownSource { stockpile } => write!(
                f,
                "unknown energy upgrade material source {}",
                stockpile.value()
            ),
            Self::SourceMassOverflow { stockpile } => write!(
                f,
                "material selection overflows in energy upgrade source {}",
                stockpile.value()
            ),
        }
    }
}

impl Error for PlayerEnergyStoreUpgradeCatalogError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::SourceAccess(error) => Some(error),
            Self::PlayerNotAdmitted
            | Self::UnknownSource { .. }
            | Self::SourceMassOverflow { .. } => None,
        }
    }
}

/// Lists feasible upgrade targets for empty, idle, exact-local stores using one accessible
/// stockpile. Missing input matter leaves a visible unready option; it never creates an item.
pub fn player_energy_store_upgrade_options(
    registries: &Registries,
    state: &AppState,
    source: StockpileId,
) -> Result<Vec<PlayerEnergyStoreUpgradeOption>, PlayerEnergyStoreUpgradeCatalogError> {
    if state.logistics().player().is_none() {
        return Err(PlayerEnergyStoreUpgradeCatalogError::PlayerNotAdmitted);
    }
    if state.inventory().get_stockpile(source).is_none() {
        return Err(PlayerEnergyStoreUpgradeCatalogError::UnknownSource { stockpile: source });
    }
    validate_player_stockpile_access(state, source)
        .map_err(PlayerEnergyStoreUpgradeCatalogError::SourceAccess)?;

    let mut options = Vec::new();
    for store in state.available_local_energy_stores() {
        if !store.stored().is_zero() || energy_store_occupancy(state, store.id()).is_some() {
            continue;
        }
        for target in registries.energy().definitions() {
            let Some(upgrade) = target
                .upgrade_profile()
                .filter(|upgrade| upgrade.from() == store.definition())
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
                    return Err(PlayerEnergyStoreUpgradeCatalogError::UnknownSource { stockpile });
                }
                Err(ConsumptionSelectionError::MassOverflow { stockpile }) => {
                    return Err(PlayerEnergyStoreUpgradeCatalogError::SourceMassOverflow {
                        stockpile,
                    });
                }
            };
            options.push(PlayerEnergyStoreUpgradeOption {
                store: store.id(),
                target: target.id(),
                name: target.name().to_owned(),
                source,
                additions: upgrade.additions().inputs().to_vec(),
                capacity: target.capacity(),
                max_input_power: target.max_input_power(),
                max_output_power: target.max_output_power(),
                material_ready,
            });
        }
    }
    Ok(options)
}

/// Revalidates a formerly observed choice. An old row cannot bypass locality, material
/// consumption, occupancy, an empty-store requirement, or owner revisions.
pub fn validate_upgrade_player_energy_store_option(
    registries: &Registries,
    state: &AppState,
    option: &PlayerEnergyStoreUpgradeOption,
) -> Result<ValidatedEnergyStoreUpgrade, EnergyStoreUpgradeError> {
    validate_upgrade_energy_store(
        registries,
        state,
        option.store,
        option.target,
        option.source,
    )
}
