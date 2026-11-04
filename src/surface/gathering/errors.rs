//! Typed rejections for hand gathering, without source or inventory mutations.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Mass, Temperature};
use crate::inventory::{StockpileId, StockpileStorageError};
use crate::labor::{PlayerWorkStartError, SurfaceGatheringMethodId};
use crate::logistics::PlayerStockpileAccessError;
use crate::material::{CommodityKey, MAX_HAND_SAFE_ASSEMBLY_TEMPERATURE};

use super::super::SurfaceResourceId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SurfaceGatheringError {
    PlayerNotLocated,
    PlayerDead,
    StalePlayerOption {
        expected_surface_revision: u64,
        actual_surface_revision: u64,
    },
    UnknownMethod {
        method: SurfaceGatheringMethodId,
    },
    ResourceUnavailableAtPlayer {
        resource: SurfaceResourceId,
    },
    CommodityMismatch {
        method: SurfaceGatheringMethodId,
        expected: CommodityKey,
        actual: CommodityKey,
    },
    ZeroMass,
    BatchTooLarge {
        requested: Mass,
        maximum: Mass,
    },
    InsufficientResource {
        requested: Mass,
        remaining: Mass,
    },
    ResourceTooHot {
        resource: SurfaceResourceId,
        temperature: Temperature,
        maximum: Temperature,
    },
    DestinationAccess(PlayerStockpileAccessError),
    DestinationMissing {
        stockpile: StockpileId,
    },
    DestinationMounted {
        stockpile: StockpileId,
    },
    DestinationStorage(StockpileStorageError),
    DestinationMassOverflow {
        stockpile: StockpileId,
    },
    DestinationCapacityExceeded {
        stockpile: StockpileId,
        capacity: Mass,
        committed: Mass,
        requested: Mass,
    },
    MaterialLotIdExhausted,
    InventoryRevisionExhausted,
    SurfaceRevisionExhausted,
    CompletionTickOverflow,
    FoodAgeOverflow,
    PlayerWork(PlayerWorkStartError),
}

pub(super) fn hand_gathering_temperature_blocker(
    resource: SurfaceResourceId,
    temperature: Temperature,
) -> Option<SurfaceGatheringError> {
    (temperature > MAX_HAND_SAFE_ASSEMBLY_TEMPERATURE).then_some(
        SurfaceGatheringError::ResourceTooHot {
            resource,
            temperature,
            maximum: MAX_HAND_SAFE_ASSEMBLY_TEMPERATURE,
        },
    )
}

impl Display for SurfaceGatheringError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerNotLocated => {
                formatter.write_str("surface gathering requires an admitted player location")
            }
            Self::PlayerDead => formatter.write_str("dead player cannot gather surface resources"),
            Self::StalePlayerOption {
                expected_surface_revision,
                actual_surface_revision,
            } => write!(
                formatter,
                "player surface-gathering option was projected at surface revision {expected_surface_revision} but current revision is {actual_surface_revision}"
            ),
            Self::UnknownMethod { method } => write!(
                formatter,
                "unknown surface gathering method {}",
                method.value()
            ),
            Self::ResourceUnavailableAtPlayer { resource } => write!(
                formatter,
                "surface resource {} is not available at the player's location",
                resource.value()
            ),
            Self::CommodityMismatch {
                method,
                expected,
                actual,
            } => write!(
                formatter,
                "surface gathering method {} expects commodity {} but resource contains {}",
                method.value(),
                expected.value(),
                actual.value()
            ),
            Self::ZeroMass => formatter.write_str("surface gathering mass must be nonzero"),
            Self::BatchTooLarge { requested, maximum } => write!(
                formatter,
                "surface gathering requested {} mg but the method permits at most {} mg per batch",
                requested.milligrams(),
                maximum.milligrams()
            ),
            Self::InsufficientResource {
                requested,
                remaining,
            } => write!(
                formatter,
                "surface gathering requested {} mg but only {} mg remains",
                requested.milligrams(),
                remaining.milligrams()
            ),
            Self::ResourceTooHot {
                resource,
                temperature,
                maximum,
            } => write!(
                formatter,
                "surface resource {} is {} mK and exceeds the {} mK unprotected hand-gathering limit",
                resource.value(),
                temperature.millikelvin(),
                maximum.millikelvin(),
            ),
            Self::DestinationAccess(error) => write!(
                formatter,
                "surface gathering destination is inaccessible: {error}"
            ),
            Self::DestinationMissing { stockpile } => write!(
                formatter,
                "surface gathering destination stockpile {} is missing",
                stockpile.value()
            ),
            Self::DestinationMounted { stockpile } => write!(
                formatter,
                "surface gathering destination stockpile {} is mounted and cannot receive direct gathering",
                stockpile.value()
            ),
            Self::DestinationStorage(error) => write!(
                formatter,
                "surface gathering destination cannot store the resource: {error}"
            ),
            Self::DestinationMassOverflow { stockpile } => write!(
                formatter,
                "surface gathering destination stockpile {} mass overflowed",
                stockpile.value()
            ),
            Self::DestinationCapacityExceeded {
                stockpile,
                capacity,
                committed,
                requested,
            } => write!(
                formatter,
                "surface gathering destination stockpile {} has {} mg capacity with {} mg committed and cannot reserve another {} mg",
                stockpile.value(),
                capacity.milligrams(),
                committed.milligrams(),
                requested.milligrams()
            ),
            Self::MaterialLotIdExhausted => formatter
                .write_str("material lot identifier space is exhausted for surface gathering"),
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted for surface gathering")
            }
            Self::SurfaceRevisionExhausted => {
                formatter.write_str("surface revision space is exhausted for gathering")
            }
            Self::CompletionTickOverflow => {
                formatter.write_str("surface gathering completion tick overflowed")
            }
            Self::FoodAgeOverflow => {
                formatter.write_str("surface food opportunity aging cannot be projected")
            }
            Self::PlayerWork(error) => write!(
                formatter,
                "surface gathering cannot reserve player work: {error}"
            ),
        }
    }
}

impl Error for SurfaceGatheringError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::DestinationAccess(error) => Some(error),
            Self::DestinationStorage(error) => Some(error),
            Self::PlayerWork(error) => Some(error),
            Self::PlayerNotLocated
            | Self::PlayerDead
            | Self::StalePlayerOption { .. }
            | Self::UnknownMethod { .. }
            | Self::ResourceUnavailableAtPlayer { .. }
            | Self::CommodityMismatch { .. }
            | Self::ZeroMass
            | Self::BatchTooLarge { .. }
            | Self::InsufficientResource { .. }
            | Self::ResourceTooHot { .. }
            | Self::DestinationMissing { .. }
            | Self::DestinationMounted { .. }
            | Self::DestinationMassOverflow { .. }
            | Self::DestinationCapacityExceeded { .. }
            | Self::MaterialLotIdExhausted
            | Self::InventoryRevisionExhausted
            | Self::SurfaceRevisionExhausted
            | Self::CompletionTickOverflow
            | Self::FoodAgeOverflow => None,
        }
    }
}
