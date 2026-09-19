//! Material-reform failures for exact equipment-maintenance component exchange.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::inventory::{StockpileId, StockpileStorageError, StockpileStructuralLoadError};
use crate::material::CommodityKey;

use super::super::super::state::EquipmentId;

/// Public maintenance-facing translation of the crate-private exact material-reform boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EquipmentMaintenanceMaterialError {
    StaleSelection {
        expected: u64,
        actual: u64,
    },
    UnknownSpentDestination {
        stockpile: StockpileId,
    },
    UnknownSpentMaterial {
        material: crate::material::MaterialId,
    },
    UnknownSpentForm {
        form: crate::material::FormId,
    },
    SpentMaterialChanged {
        source: crate::material::MaterialId,
        target: crate::material::MaterialId,
    },
    SpentPhaseChanged {
        replacement: crate::material::FormId,
        spent: crate::material::FormId,
    },
    SpentFormUnchanged {
        commodity: CommodityKey,
    },
    SpentStorage(StockpileStorageError),
    SpentMassOverflow {
        stockpile: StockpileId,
    },
    SpentCapacityExceeded {
        stockpile: StockpileId,
        capacity: Mass,
        committed: Mass,
        requested: Mass,
    },
    LotIdExhausted,
    InventoryRevisionExhausted,
    EmbodiedComponentMismatch {
        equipment: EquipmentId,
        component: CommodityKey,
        embodied: Mass,
        required: Mass,
    },
    InvalidEmbodiedComponent {
        equipment: EquipmentId,
    },
    StructuralLoad(StockpileStructuralLoadError),
}

impl Display for EquipmentMaintenanceMaterialError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleSelection { expected, actual } => write!(
                formatter,
                "maintenance material selection expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::UnknownSpentDestination { stockpile } => write!(
                formatter,
                "spent maintenance destination stockpile {} does not exist",
                stockpile.value()
            ),
            Self::UnknownSpentMaterial { material } => write!(
                formatter,
                "spent maintenance output references unknown material {}",
                material.value()
            ),
            Self::UnknownSpentForm { form } => write!(
                formatter,
                "spent maintenance output references unknown form {}",
                form.value()
            ),
            Self::SpentMaterialChanged { source, target } => write!(
                formatter,
                "equipment maintenance cannot change material identity from {} to {}",
                source.value(),
                target.value()
            ),
            Self::SpentPhaseChanged { replacement, spent } => write!(
                formatter,
                "equipment maintenance cannot change material phase from form {} to form {} without a thermal process",
                replacement.value(),
                spent.value()
            ),
            Self::SpentFormUnchanged { commodity } => write!(
                formatter,
                "equipment maintenance spent output must differ from replacement commodity {}",
                commodity.value()
            ),
            Self::SpentStorage(error) => {
                write!(
                    formatter,
                    "spent maintenance storage rejects material: {error}"
                )
            }
            Self::SpentMassOverflow { stockpile } => write!(
                formatter,
                "spent maintenance material overflows stockpile {} mass accounting",
                stockpile.value()
            ),
            Self::SpentCapacityExceeded {
                stockpile,
                capacity,
                committed,
                requested,
            } => write!(
                formatter,
                "spent maintenance material exceeds stockpile {} capacity {} mg: {} mg committed, {} mg requested",
                stockpile.value(),
                capacity.milligrams(),
                committed.milligrams(),
                requested.milligrams()
            ),
            Self::LotIdExhausted => formatter.write_str(
                "material lot identifier space is exhausted during equipment maintenance",
            ),
            Self::InventoryRevisionExhausted => formatter
                .write_str("inventory revision space is exhausted during equipment maintenance"),
            Self::EmbodiedComponentMismatch {
                equipment,
                component,
                embodied,
                required,
            } => write!(
                formatter,
                "equipment {} contains {} mg of service component {} but {} mg must be exchanged",
                equipment.value(),
                embodied.milligrams(),
                component.value(),
                required.milligrams()
            ),
            Self::InvalidEmbodiedComponent { equipment } => write!(
                formatter,
                "equipment {} contains an invalid embodied component trace for maintenance exchange",
                equipment.value()
            ),
            Self::StructuralLoad(error) => write!(
                formatter,
                "maintenance material movement cannot update stored-matter load: {error}"
            ),
        }
    }
}

impl Error for EquipmentMaintenanceMaterialError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::SpentStorage(error) => Some(error),
            Self::StructuralLoad(error) => Some(error),
            Self::StaleSelection {
                expected: _expected,
                actual: _actual,
            } => None,
            Self::UnknownSpentDestination {
                stockpile: _stockpile,
            }
            | Self::SpentMassOverflow {
                stockpile: _stockpile,
            } => None,
            Self::UnknownSpentMaterial {
                material: _material,
            }
            | Self::SpentMaterialChanged {
                source: _material,
                target: _,
            } => None,
            Self::SpentPhaseChanged {
                replacement: _replacement,
                spent: _spent,
            } => None,
            Self::SpentFormUnchanged {
                commodity: _commodity,
            } => None,
            Self::UnknownSpentForm { form: _form } => None,
            Self::SpentCapacityExceeded {
                stockpile: _stockpile,
                capacity: _capacity,
                committed: _committed,
                requested: _requested,
            } => None,
            Self::LotIdExhausted
            | Self::InventoryRevisionExhausted
            | Self::EmbodiedComponentMismatch { .. }
            | Self::InvalidEmbodiedComponent { .. } => None,
        }
    }
}
