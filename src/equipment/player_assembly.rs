//! Actor-facing equipment assembly choices over one accessible material stockpile.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Mass, Temperature};
use crate::core::state::AppState;
use crate::inventory::StockpileId;
use crate::logistics::{PlayerStockpileAccessError, validate_player_stockpile_access};
use crate::material::{CommodityKey, MaterialInputSpec};
use crate::registry::Registries;

use super::construction_execution::validate_assembly_material_selection;
use super::{
    EquipmentAssemblyError, EquipmentDefinitionId, ValidatedEquipmentAssembly,
    validate_assemble_equipment,
};

/// Current material readiness for one player-visible equipment assembly row.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerEquipmentAssemblyAvailability {
    Ready,
    MissingMaterial {
        commodity: CommodityKey,
        available: Mass,
        required: Mass,
    },
    MaterialTooHot {
        commodity: CommodityKey,
        temperature: Temperature,
        maximum: Temperature,
    },
}

/// One authored equipment assembly visible against a specific accessible stockpile.
///
/// This is read-side evidence, not an authorization token. Starting the option re-runs canonical
/// assembly validation against current owner state.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerEquipmentAssemblyOption {
    definition: EquipmentDefinitionId,
    name: String,
    source: StockpileId,
    embodied_mass: Mass,
    inputs: Vec<MaterialInputSpec>,
    availability: PlayerEquipmentAssemblyAvailability,
}

impl PlayerEquipmentAssemblyOption {
    #[must_use]
    pub const fn definition(&self) -> EquipmentDefinitionId {
        self.definition
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
    pub const fn embodied_mass(&self) -> Mass {
        self.embodied_mass
    }

    #[must_use]
    pub fn inputs(&self) -> &[MaterialInputSpec] {
        &self.inputs
    }

    pub const fn availability(&self) -> PlayerEquipmentAssemblyAvailability {
        self.availability
    }

    #[must_use]
    pub const fn is_ready(&self) -> bool {
        matches!(
            self.availability,
            PlayerEquipmentAssemblyAvailability::Ready
        )
    }
}

/// Failure while projecting player equipment assembly choices from one stockpile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerEquipmentAssemblyCatalogError {
    PlayerNotAdmitted,
    SourceAccess(PlayerStockpileAccessError),
    UnknownSource { stockpile: StockpileId },
    SourceMassOverflow { stockpile: StockpileId },
}

impl Display for PlayerEquipmentAssemblyCatalogError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerNotAdmitted => formatter.write_str("player is not admitted"),
            Self::SourceAccess(error) => {
                write!(
                    formatter,
                    "equipment assembly source access failed: {error}"
                )
            }
            Self::UnknownSource { stockpile } => write!(
                formatter,
                "equipment assembly source stockpile {} does not exist",
                stockpile.value()
            ),
            Self::SourceMassOverflow { stockpile } => write!(
                formatter,
                "equipment assembly source stockpile {} mass accounting overflowed",
                stockpile.value()
            ),
        }
    }
}

impl Error for PlayerEquipmentAssemblyCatalogError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::SourceAccess(error) => Some(error),
            Self::PlayerNotAdmitted
            | Self::UnknownSource { .. }
            | Self::SourceMassOverflow { .. } => None,
        }
    }
}

/// Projects authored equipment assembly rows against one currently accessible material source.
pub fn player_equipment_assembly_options(
    registries: &Registries,
    state: &AppState,
    source: StockpileId,
) -> Result<Vec<PlayerEquipmentAssemblyOption>, PlayerEquipmentAssemblyCatalogError> {
    if state.logistics().player().is_none() {
        return Err(PlayerEquipmentAssemblyCatalogError::PlayerNotAdmitted);
    }
    if state.inventory().get_stockpile(source).is_none() {
        return Err(PlayerEquipmentAssemblyCatalogError::UnknownSource { stockpile: source });
    }
    validate_player_stockpile_access(state, source)
        .map_err(PlayerEquipmentAssemblyCatalogError::SourceAccess)?;

    registries
        .equipment()
        .definitions()
        .filter_map(|definition| {
            let assembly = definition.assembly_profile()?;
            Some((definition, assembly))
        })
        .map(|(definition, assembly)| {
            let availability = match validate_assembly_material_selection(
                state.inventory(),
                source,
                assembly.inputs(),
            ) {
                Ok(_) => PlayerEquipmentAssemblyAvailability::Ready,
                Err(EquipmentAssemblyError::AssemblyMaterialTooHot {
                    commodity,
                    temperature,
                    maximum,
                }) => PlayerEquipmentAssemblyAvailability::MaterialTooHot {
                    commodity,
                    temperature,
                    maximum,
                },
                Err(EquipmentAssemblyError::InsufficientMaterial {
                    commodity,
                    available,
                    required,
                    ..
                }) => PlayerEquipmentAssemblyAvailability::MissingMaterial {
                    commodity,
                    available,
                    required,
                },
                Err(EquipmentAssemblyError::UnknownSource { stockpile }) => {
                    return Err(PlayerEquipmentAssemblyCatalogError::UnknownSource { stockpile });
                }
                Err(EquipmentAssemblyError::SourceMassOverflow { stockpile }) => {
                    return Err(PlayerEquipmentAssemblyCatalogError::SourceMassOverflow {
                        stockpile,
                    });
                }
                Err(_) => {
                    unreachable!("assembly material selection returns only material blockers")
                }
            };
            Ok(PlayerEquipmentAssemblyOption {
                definition: definition.id(),
                name: definition.name().to_owned(),
                source,
                embodied_mass: definition.mass(),
                inputs: assembly.inputs().to_vec(),
                availability,
            })
        })
        .collect()
}

/// Revalidates and starts one previously observed equipment assembly row.
pub fn validate_assemble_player_equipment_option(
    registries: &Registries,
    state: &AppState,
    option: &PlayerEquipmentAssemblyOption,
) -> Result<ValidatedEquipmentAssembly, EquipmentAssemblyError> {
    validate_assemble_equipment(registries, state, option.definition, option.source)
}

#[cfg(test)]
#[path = "player_assembly_tests.rs"]
mod tests;
