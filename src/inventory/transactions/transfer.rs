//! Test/gameplay-audit controlled-delivery transaction built on exact inventory relocation.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::material::{CommodityKey, FormId, MaterialId, MaterialInputSpec};
use crate::registry::Registries;
use crate::structural::StructuralCommitError;

use super::super::selection::{ConsumptionSelectionError, validate_consumption_selection};
use super::super::state::StockpileId;
use super::super::storage_validation::{
    CommodityReferenceError, StockpileStorageError, validate_commodity_reference,
};
use super::super::structural_integration::StockpileStructuralLoadError;
use super::{
    MaterialRelocationCommitError, MaterialRelocationError, ValidatedMaterialRelocation,
    validate_material_relocation_from_selection,
};

/// Fixture authorization for one controlled stockpile-to-stockpile delivery.
///
/// This type is compiled only for tests and the gameplay-audit feature. Inventory still validates and
/// commits exact custody and structural-load consequences, but ordinary runtime cannot manufacture a
/// pathless logistics event because world logistics is outside the current production scope.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct MaterialTransferResolution {
    source: StockpileId,
    destination: StockpileId,
    commodity: CommodityKey,
    mass: Mass,
}

impl MaterialTransferResolution {
    #[cfg(any(test, feature = "test-gameplay"))]
    pub(crate) const fn new(
        source: StockpileId,
        destination: StockpileId,
        commodity: CommodityKey,
        mass: Mass,
    ) -> Self {
        Self {
            source,
            destination,
            commodity,
            mass,
        }
    }
}

/// Failure while validating an atomic stockpile-to-stockpile transfer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MaterialTransferError {
    UnknownStockpile {
        stockpile: StockpileId,
    },
    SameStockpile {
        stockpile: StockpileId,
    },
    UnknownMaterial {
        material: MaterialId,
    },
    UnknownForm {
        form: FormId,
    },
    ZeroMass,
    Storage(StockpileStorageError),
    InsufficientMass {
        stockpile: StockpileId,
        commodity: CommodityKey,
        available: Mass,
        requested: Mass,
    },
    MassOverflow {
        stockpile: StockpileId,
    },
    CapacityExceeded {
        stockpile: StockpileId,
        capacity: Mass,
        committed: Mass,
        requested: Mass,
    },
    LotIdExhausted,
    RevisionExhausted,
    StructuralLoad(StockpileStructuralLoadError),
}

impl Display for MaterialTransferError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownStockpile { stockpile } => {
                write!(formatter, "unknown stockpile id {}", stockpile.value())
            }
            Self::SameStockpile { stockpile } => write!(
                formatter,
                "inventory transfer requires distinct source and destination; both are stockpile {}",
                stockpile.value()
            ),
            Self::UnknownMaterial { material } => {
                write!(formatter, "unknown material id {}", material.value())
            }
            Self::UnknownForm { form } => write!(formatter, "unknown form id {}", form.value()),
            Self::ZeroMass => formatter.write_str("transfer mass must be nonzero"),
            Self::Storage(error) => write!(formatter, "destination rejects transfer: {error}"),
            Self::InsufficientMass {
                stockpile,
                commodity: _commodity,
                available,
                requested,
            } => write!(
                formatter,
                "stockpile {} has {} mg available but {} mg was requested",
                stockpile.value(),
                available.milligrams(),
                requested.milligrams()
            ),
            Self::MassOverflow { stockpile } => write!(
                formatter,
                "mass accounting overflow in stockpile {}",
                stockpile.value()
            ),
            Self::CapacityExceeded {
                stockpile,
                capacity,
                committed,
                requested,
            } => write!(
                formatter,
                "stockpile {} capacity {} mg exceeded: {} mg committed, {} mg requested",
                stockpile.value(),
                capacity.milligrams(),
                committed.milligrams(),
                requested.milligrams()
            ),
            Self::LotIdExhausted => {
                formatter.write_str("material lot identifier space is exhausted")
            }
            Self::RevisionExhausted => formatter.write_str("inventory revision space is exhausted"),
            Self::StructuralLoad(error) => write!(
                formatter,
                "transfer cannot update stored-matter support load: {error}"
            ),
        }
    }
}

impl Error for MaterialTransferError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            Self::StructuralLoad(error) => Some(error),
            Self::UnknownStockpile { .. }
            | Self::SameStockpile { .. }
            | Self::UnknownMaterial { .. }
            | Self::UnknownForm { .. }
            | Self::InsufficientMass { .. }
            | Self::MassOverflow { .. }
            | Self::CapacityExceeded { .. }
            | Self::ZeroMass
            | Self::LotIdExhausted
            | Self::RevisionExhausted => None,
        }
    }
}

/// Failure when a validated transfer is committed after inventory has changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MaterialTransferCommitError {
    StaleInventoryRevision { expected: u64, actual: u64 },
    Structure(StructuralCommitError),
}

impl Display for MaterialTransferCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "validated transfer expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::Structure(error) => write!(
                formatter,
                "validated transfer could not commit stored-matter structural load: {error}"
            ),
        }
    }
}

impl Error for MaterialTransferCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StaleInventoryRevision { .. } => None,
        }
    }
}

/// Consumed proof that all preconditions for a two-stockpile transfer have been checked.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct ValidatedMaterialTransfer {
    relocation: ValidatedMaterialRelocation,
}

impl ValidatedMaterialTransfer {
    /// Atomically commits this already validated transfer and consumes the proof token.
    pub fn commit(self, state: &mut AppState) -> Result<(), MaterialTransferCommitError> {
        self.relocation.commit(state).map_err(|error| match error {
            MaterialRelocationCommitError::StaleInventoryRevision { expected, actual } => {
                MaterialTransferCommitError::StaleInventoryRevision { expected, actual }
            }
            MaterialRelocationCommitError::Structure(error) => {
                MaterialTransferCommitError::Structure(error)
            }
        })
    }
}

/// Validates one already physically resolved material transfer without mutating either stockpile.
pub fn validate_material_transfer(
    registries: &Registries,
    state: &AppState,
    resolution: MaterialTransferResolution,
) -> Result<ValidatedMaterialTransfer, MaterialTransferError> {
    let MaterialTransferResolution {
        source,
        destination,
        commodity,
        mass,
    } = resolution;
    validate_commodity_reference(registries, commodity).map_err(|error| match error {
        CommodityReferenceError::UnknownMaterial { material } => {
            MaterialTransferError::UnknownMaterial { material }
        }
        CommodityReferenceError::UnknownForm { form } => {
            MaterialTransferError::UnknownForm { form }
        }
        CommodityReferenceError::UnsupportedCommodity { commodity } => {
            MaterialTransferError::Storage(StockpileStorageError::UnsupportedCommodity {
                commodity,
            })
        }
    })?;
    if mass.is_zero() {
        return Err(MaterialTransferError::ZeroMass);
    }
    let input = MaterialInputSpec::new(commodity, mass);
    let selection = validate_consumption_selection(state.inventory(), source, &[input])
        .map_err(map_transfer_selection_error)?;
    let relocation =
        validate_material_relocation_from_selection(registries, state, destination, selection)
            .map_err(map_transfer_relocation_error)?;
    Ok(ValidatedMaterialTransfer { relocation })
}

fn map_transfer_selection_error(error: ConsumptionSelectionError) -> MaterialTransferError {
    match error {
        ConsumptionSelectionError::UnknownStockpile { stockpile } => {
            MaterialTransferError::UnknownStockpile { stockpile }
        }
        ConsumptionSelectionError::InsufficientMass {
            stockpile,
            commodity,
            available,
            requested,
        } => MaterialTransferError::InsufficientMass {
            stockpile,
            commodity,
            available,
            requested,
        },
        ConsumptionSelectionError::MassOverflow { stockpile } => {
            MaterialTransferError::MassOverflow { stockpile }
        }
    }
}

fn map_transfer_relocation_error(error: MaterialRelocationError) -> MaterialTransferError {
    match error {
        MaterialRelocationError::StaleSelection { expected, actual } => {
            unreachable!(
                "material transfer selection revision {expected} cannot become stale at revision {actual} between synchronous selection and relocation validation"
            )
        }
        MaterialRelocationError::UnknownSource { stockpile }
        | MaterialRelocationError::UnknownDestination { stockpile } => {
            MaterialTransferError::UnknownStockpile { stockpile }
        }
        MaterialRelocationError::SameStockpile { stockpile } => {
            MaterialTransferError::SameStockpile { stockpile }
        }
        MaterialRelocationError::DestinationStorage(error) => MaterialTransferError::Storage(error),
        MaterialRelocationError::DestinationMassOverflow { stockpile } => {
            MaterialTransferError::MassOverflow { stockpile }
        }
        MaterialRelocationError::DestinationCapacityExceeded {
            stockpile,
            capacity,
            committed,
            requested,
        } => MaterialTransferError::CapacityExceeded {
            stockpile,
            capacity,
            committed,
            requested,
        },
        MaterialRelocationError::LotIdExhausted => MaterialTransferError::LotIdExhausted,
        MaterialRelocationError::RevisionExhausted => MaterialTransferError::RevisionExhausted,
        MaterialRelocationError::StructuralLoad(error) => {
            MaterialTransferError::StructuralLoad(error)
        }
    }
}
