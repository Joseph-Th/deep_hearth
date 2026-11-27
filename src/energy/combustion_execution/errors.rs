//! Typed admission and commit failures for finite fuel combustion.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Mass, Temperature};
use crate::inventory::{
    ExplicitConsumptionSelectionError, MaterialStackProjectionError, StockpileId,
    StockpileStructuralLoadError,
};
use crate::logistics::{PlayerEnergyStoreAccessError, PlayerStockpileAccessError};
use crate::material::CommodityKey;
use crate::structural::StructuralCommitError;

use super::EnergyStoreId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CombustionError {
    PlayerDead,
    PlayerBusy,
    PreparedStrikeActive,
    StoreAccess(PlayerEnergyStoreAccessError),
    SourceAccess(PlayerStockpileAccessError),
    UnknownStore {
        store: EnergyStoreId,
    },
    UnknownDefinition,
    NoCombustionProfile {
        store: EnergyStoreId,
    },
    UnsupportedFuel {
        store: EnergyStoreId,
        fuel: CommodityKey,
    },
    ImpureFuel {
        fuel: CommodityKey,
    },
    FuelTooHot {
        fuel: CommodityKey,
        temperature: Temperature,
        maximum: Temperature,
    },
    StackSelection(MaterialStackProjectionError),
    ExplicitSelection(ExplicitConsumptionSelectionError),
    StoreBusy {
        store: EnergyStoreId,
    },
    InsufficientFuel {
        source: StockpileId,
        available: Mass,
        required: Mass,
    },
    FractionalChargeTooSmall {
        available: Mass,
    },
    SourceMassOverflow {
        source: StockpileId,
    },
    StaleInventorySelection {
        expected: u64,
        actual: u64,
    },
    InventoryRevisionExhausted,
    EnergyRevisionExhausted,
    CombustedMassOverflow,
    InsufficientThermalCapacity {
        store: EnergyStoreId,
        stored: Energy,
        released: Energy,
        capacity: Energy,
    },
    StructuralLoad(StockpileStructuralLoadError),
}

impl Display for CombustionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerDead => formatter.write_str("dead player cannot combust fuel"),
            Self::PlayerBusy => formatter.write_str("player cannot tend fuel while work is active"),
            Self::PreparedStrikeActive => {
                formatter.write_str("player must lower the prepared weapon before tending fuel")
            }
            Self::StoreAccess(error) => {
                write!(formatter, "combustion store is inaccessible: {error}")
            }
            Self::SourceAccess(error) => {
                write!(formatter, "combustion fuel source is inaccessible: {error}")
            }
            Self::UnknownStore { store } => write!(
                formatter,
                "unknown combustion energy store {}",
                store.value()
            ),
            Self::UnknownDefinition => {
                formatter.write_str("combustion energy store references an unknown definition")
            }
            Self::NoCombustionProfile { store } => write!(
                formatter,
                "energy store {} does not accept solid-fuel combustion",
                store.value()
            ),
            Self::UnsupportedFuel { store, fuel } => write!(
                formatter,
                "energy store {} cannot burn material {} form {}",
                store.value(),
                fuel.material().value(),
                fuel.form().value()
            ),
            Self::ImpureFuel { fuel } => write!(
                formatter,
                "combustion fuel material {} form {} must be pure",
                fuel.material().value(),
                fuel.form().value()
            ),
            Self::FuelTooHot {
                fuel,
                temperature,
                maximum,
            } => write!(
                formatter,
                "fuel material {} form {} is {} mK, above the hand-safe limit of {} mK",
                fuel.material().value(),
                fuel.form().value(),
                temperature.millikelvin(),
                maximum.millikelvin()
            ),
            Self::StackSelection(error) => write!(formatter, "combustion fuel stack: {error}"),
            Self::ExplicitSelection(error) => {
                write!(formatter, "combustion fuel lot selection: {error}")
            }
            Self::StoreBusy { store } => {
                write!(formatter, "energy store {} is occupied", store.value())
            }
            Self::InsufficientFuel {
                source,
                available,
                required,
            } => write!(
                formatter,
                "fuel source {} contains {} mg but combustion requires {} mg",
                source.value(),
                available.milligrams(),
                required.milligrams()
            ),
            Self::FractionalChargeTooSmall { available } => write!(
                formatter,
                "fuel fragment of {} mg cannot release representable heat",
                available.milligrams()
            ),
            Self::SourceMassOverflow { source } => write!(
                formatter,
                "fuel source {} stored mass overflowed",
                source.value()
            ),
            Self::StaleInventorySelection { expected, actual } => write!(
                formatter,
                "combustion fuel selection inventory revision changed from {expected} to {actual}"
            ),
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted for combustion")
            }
            Self::EnergyRevisionExhausted => {
                formatter.write_str("energy revision space is exhausted for combustion")
            }
            Self::CombustedMassOverflow => {
                formatter.write_str("combusted terminal mass accounting overflowed")
            }
            Self::InsufficientThermalCapacity {
                store,
                stored,
                released,
                capacity,
            } => write!(
                formatter,
                "energy store {} has {} nJ stored and {} nJ capacity, insufficient for {} nJ combustion release",
                store.value(),
                stored.nanojoules(),
                capacity.nanojoules(),
                released.nanojoules()
            ),
            Self::StructuralLoad(error) => write!(
                formatter,
                "combustion fuel removal violates structural load constraints: {error}"
            ),
        }
    }
}

impl Error for CombustionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::StoreAccess(error) => Some(error),
            Self::SourceAccess(error) => Some(error),
            Self::StructuralLoad(error) => Some(error),
            Self::StackSelection(error) => Some(error),
            Self::ExplicitSelection(error) => Some(error),
            Self::PlayerDead
            | Self::PlayerBusy
            | Self::PreparedStrikeActive
            | Self::UnknownStore { .. }
            | Self::UnknownDefinition
            | Self::NoCombustionProfile { .. }
            | Self::UnsupportedFuel { .. }
            | Self::ImpureFuel { .. }
            | Self::FuelTooHot { .. }
            | Self::StoreBusy { .. }
            | Self::InsufficientFuel { .. }
            | Self::FractionalChargeTooSmall { .. }
            | Self::SourceMassOverflow { .. }
            | Self::StaleInventorySelection { .. }
            | Self::InventoryRevisionExhausted
            | Self::EnergyRevisionExhausted
            | Self::CombustedMassOverflow
            | Self::InsufficientThermalCapacity { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CombustionCommitError {
    PlayerBecameBusy,
    StaleCombatRevision { expected: u64, actual: u64 },
    StaleSurvival { expected: u64, actual: u64 },
    StaleLogistics { expected: u64, actual: u64 },
    StaleInventory { expected: u64, actual: u64 },
    StaleEnergy { expected: u64, actual: u64 },
    StoreChanged { store: EnergyStoreId },
    Structure(StructuralCommitError),
}

impl Display for CombustionCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerBecameBusy => formatter.write_str("player work started before fuel commit"),
            Self::StaleCombatRevision { expected, actual } => write!(
                formatter,
                "combustion combat revision changed from {expected} to {actual} before commit"
            ),
            Self::StaleSurvival { expected, actual } => write!(
                formatter,
                "combustion survival revision changed from {expected} to {actual} before commit"
            ),
            Self::StaleLogistics { expected, actual } => write!(
                formatter,
                "combustion logistics revision changed from {expected} to {actual} before commit"
            ),
            Self::StaleInventory { expected, actual } => write!(
                formatter,
                "combustion inventory revision changed from {expected} to {actual} before commit"
            ),
            Self::StaleEnergy { expected, actual } => write!(
                formatter,
                "combustion energy revision changed from {expected} to {actual} before commit"
            ),
            Self::StoreChanged { store } => write!(
                formatter,
                "combustion energy store {} changed before commit",
                store.value()
            ),
            Self::Structure(error) => write!(
                formatter,
                "combustion structural-load commit failed: {error}"
            ),
        }
    }
}

impl Error for CombustionCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::PlayerBecameBusy
            | Self::StaleCombatRevision { .. }
            | Self::StaleSurvival { .. }
            | Self::StaleLogistics { .. }
            | Self::StaleInventory { .. }
            | Self::StaleEnergy { .. }
            | Self::StoreChanged { .. } => None,
        }
    }
}
