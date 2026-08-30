//! Additive, matter-conserving upgrades of empty and idle finite energy stores.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Mass};
use crate::core::state::AppState;
use crate::inventory::{
    ConsumedMaterialTrace, StockpileId, StockpileStoredMassChange, StockpileStructuralLoadError,
    ValidatedMaterialEgress, ValidatedStockpileStructuralLoad, apply_material_egress,
    validate_consumption_selection, validate_material_egress_from_selection,
    validate_stockpile_stored_mass_changes,
};
use crate::production::{ProductionJobId, ProductionOccupancyRelease};
use crate::registry::Registries;
use crate::structural::StructuralCommitError;

use super::{EnergyStoreDefinitionId, EnergyStoreId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EnergyStoreUpgradeError {
    UnknownStore {
        store: EnergyStoreId,
    },
    UnknownTargetDefinition {
        target: EnergyStoreDefinitionId,
    },
    NoUpgradeProfile {
        target: EnergyStoreDefinitionId,
    },
    WrongBaseDefinition {
        store: EnergyStoreId,
        required: EnergyStoreDefinitionId,
        actual: EnergyStoreDefinitionId,
    },
    StoreNotEmpty {
        store: EnergyStoreId,
        stored: Energy,
    },
    StoreBusyProduction {
        store: EnergyStoreId,
        job: ProductionJobId,
        release: ProductionOccupancyRelease,
    },
    StoreBusyManualPower {
        store: EnergyStoreId,
    },
    UnknownSource {
        stockpile: StockpileId,
    },
    InsufficientMaterial {
        stockpile: StockpileId,
        available: Mass,
        required: Mass,
    },
    SourceMassOverflow {
        stockpile: StockpileId,
    },
    StaleInventorySelection {
        expected: u64,
        actual: u64,
    },
    InventoryRevisionExhausted,
    EnergyRevisionExhausted,
    StructuralLoad(StockpileStructuralLoadError),
}

impl Display for EnergyStoreUpgradeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownStore { store } => {
                write!(formatter, "unknown energy store {}", store.value())
            }
            Self::UnknownTargetDefinition { target } => write!(
                formatter,
                "unknown target energy-store definition {}",
                target.value()
            ),
            Self::NoUpgradeProfile { target } => write!(
                formatter,
                "energy-store definition {} has no authored additive upgrade path",
                target.value()
            ),
            Self::WrongBaseDefinition {
                store,
                required,
                actual,
            } => write!(
                formatter,
                "energy store {} uses definition {} but upgrade requires base definition {}",
                store.value(),
                actual.value(),
                required.value()
            ),
            Self::StoreNotEmpty { store, stored } => write!(
                formatter,
                "energy store {} still owns {} nJ and must be empty before its physical storage body can be upgraded",
                store.value(),
                stored.nanojoules()
            ),
            Self::StoreBusyProduction {
                store,
                job,
                release,
            } => write!(
                formatter,
                "energy store {} is occupied by production job {} {release} and cannot be upgraded",
                store.value(),
                job.value()
            ),
            Self::StoreBusyManualPower { store } => write!(
                formatter,
                "energy store {} is reserved by direct player-powered generation and cannot be upgraded",
                store.value()
            ),
            Self::UnknownSource { stockpile } => write!(
                formatter,
                "unknown energy-store upgrade material stockpile {}",
                stockpile.value()
            ),
            Self::InsufficientMaterial {
                stockpile,
                available,
                required,
            } => write!(
                formatter,
                "energy-store upgrade stockpile {} contains {} mg but {} mg of authored addition material is required",
                stockpile.value(),
                available.milligrams(),
                required.milligrams()
            ),
            Self::SourceMassOverflow { stockpile } => write!(
                formatter,
                "energy-store upgrade source {} mass accounting overflowed",
                stockpile.value()
            ),
            Self::StaleInventorySelection { expected, actual } => write!(
                formatter,
                "energy-store upgrade material selection expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted")
            }
            Self::EnergyRevisionExhausted => {
                formatter.write_str("energy revision space is exhausted")
            }
            Self::StructuralLoad(error) => {
                write!(
                    formatter,
                    "energy-store upgrade source load failed: {error}"
                )
            }
        }
    }
}

impl Error for EnergyStoreUpgradeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::StructuralLoad(error) => Some(error),
            Self::UnknownStore { .. }
            | Self::UnknownTargetDefinition { .. }
            | Self::NoUpgradeProfile { .. }
            | Self::WrongBaseDefinition { .. }
            | Self::StoreNotEmpty { .. }
            | Self::StoreBusyProduction { .. }
            | Self::StoreBusyManualPower { .. }
            | Self::UnknownSource { .. }
            | Self::InsufficientMaterial { .. }
            | Self::SourceMassOverflow { .. }
            | Self::StaleInventorySelection { .. }
            | Self::InventoryRevisionExhausted
            | Self::EnergyRevisionExhausted => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EnergyStoreUpgradeCommitError {
    StaleInventory {
        expected: u64,
        actual: u64,
    },
    StaleEnergy {
        expected: u64,
        actual: u64,
    },
    UnknownStore {
        store: EnergyStoreId,
    },
    StoreChanged {
        store: EnergyStoreId,
    },
    StoreBusyProduction {
        store: EnergyStoreId,
        job: ProductionJobId,
    },
    StoreBusyManualPower {
        store: EnergyStoreId,
    },
    Structure(StructuralCommitError),
}

impl Display for EnergyStoreUpgradeCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleInventory { expected, actual } => write!(
                formatter,
                "energy-store upgrade expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::StaleEnergy { expected, actual } => write!(
                formatter,
                "energy-store upgrade expected energy revision {expected} but current revision is {actual}"
            ),
            Self::UnknownStore { store } => write!(
                formatter,
                "energy store {} disappeared before upgrade commit",
                store.value()
            ),
            Self::StoreChanged { store } => write!(
                formatter,
                "energy store {} changed after upgrade validation",
                store.value()
            ),
            Self::StoreBusyProduction { store, job } => write!(
                formatter,
                "energy store {} became occupied by production job {} before upgrade commit",
                store.value(),
                job.value()
            ),
            Self::StoreBusyManualPower { store } => write!(
                formatter,
                "energy store {} became reserved by direct player-powered generation before upgrade commit",
                store.value()
            ),
            Self::Structure(error) => {
                write!(formatter, "energy-store upgrade structure failed: {error}")
            }
        }
    }
}

impl Error for EnergyStoreUpgradeCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StaleInventory { .. }
            | Self::StaleEnergy { .. }
            | Self::UnknownStore { .. }
            | Self::StoreChanged { .. }
            | Self::StoreBusyProduction { .. }
            | Self::StoreBusyManualPower { .. } => None,
        }
    }
}

#[must_use]
pub struct ValidatedEnergyStoreUpgrade {
    store: EnergyStoreId,
    expected_definition: EnergyStoreDefinitionId,
    target_definition: EnergyStoreDefinitionId,
    expected_embodied_mass: Mass,
    additions: Vec<ConsumedMaterialTrace>,
    expected_energy_revision: u64,
    next_energy_revision: u64,
    egress: ValidatedMaterialEgress,
    structural_load: Option<ValidatedStockpileStructuralLoad>,
}

impl ValidatedEnergyStoreUpgrade {
    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<EnergyStoreId, EnergyStoreUpgradeCommitError> {
        if state.inventory().revision() != self.egress.expected_revision() {
            return Err(EnergyStoreUpgradeCommitError::StaleInventory {
                expected: self.egress.expected_revision(),
                actual: state.inventory().revision(),
            });
        }
        if state.energy().revision() != self.expected_energy_revision {
            return Err(EnergyStoreUpgradeCommitError::StaleEnergy {
                expected: self.expected_energy_revision,
                actual: state.energy().revision(),
            });
        }
        let record = state
            .energy()
            .get_store(self.store)
            .ok_or(EnergyStoreUpgradeCommitError::UnknownStore { store: self.store })?;
        if record.definition() != self.expected_definition
            || record.stored() != Energy::ZERO
            || record.embodied_mass() != self.expected_embodied_mass
        {
            return Err(EnergyStoreUpgradeCommitError::StoreChanged { store: self.store });
        }
        if let Some(job) = state.production().get_energy_occupant(self.store) {
            return Err(EnergyStoreUpgradeCommitError::StoreBusyProduction {
                store: self.store,
                job,
            });
        }
        if state
            .player_work()
            .get_manual_power_energy_occupant(self.store)
            .is_some()
        {
            return Err(EnergyStoreUpgradeCommitError::StoreBusyManualPower { store: self.store });
        }
        if let Some(load) = self.structural_load {
            load.commit(state)
                .map_err(EnergyStoreUpgradeCommitError::Structure)?;
        }
        apply_material_egress(state.inventory_state_mut(), self.egress);
        state.energy_state_mut().apply_upgrade(
            self.store,
            self.expected_definition,
            self.target_definition,
            self.additions,
            self.expected_energy_revision,
            self.next_energy_revision,
        );
        Ok(self.store)
    }
}

/// Validates an additive upgrade of one empty, idle, material-backed energy store.
pub fn validate_upgrade_energy_store(
    registries: &Registries,
    state: &AppState,
    store: EnergyStoreId,
    target: EnergyStoreDefinitionId,
    source: StockpileId,
) -> Result<ValidatedEnergyStoreUpgrade, EnergyStoreUpgradeError> {
    let record = state
        .energy()
        .get_store(store)
        .ok_or(EnergyStoreUpgradeError::UnknownStore { store })?;
    let target_definition = registries
        .energy()
        .get_store(target)
        .ok_or(EnergyStoreUpgradeError::UnknownTargetDefinition { target })?;
    let upgrade = target_definition
        .upgrade_profile()
        .ok_or(EnergyStoreUpgradeError::NoUpgradeProfile { target })?;
    if record.definition() != upgrade.from() {
        return Err(EnergyStoreUpgradeError::WrongBaseDefinition {
            store,
            required: upgrade.from(),
            actual: record.definition(),
        });
    }
    if !record.stored().is_zero() {
        return Err(EnergyStoreUpgradeError::StoreNotEmpty {
            store,
            stored: record.stored(),
        });
    }
    if let Some(job) = state.production().get_energy_occupant(store) {
        let release = state
            .production()
            .get_job(job)
            .unwrap_or_else(|| {
                panic!(
                    "runtime invariant broken: energy occupancy references missing production job {}",
                    job.value()
                )
            })
            .occupancy_release();
        return Err(EnergyStoreUpgradeError::StoreBusyProduction {
            store,
            job,
            release,
        });
    }
    if state
        .player_work()
        .get_manual_power_energy_occupant(store)
        .is_some()
    {
        return Err(EnergyStoreUpgradeError::StoreBusyManualPower { store });
    }
    let selection =
        validate_consumption_selection(state.inventory(), source, upgrade.additions().inputs())
            .map_err(|error| match error {
                crate::inventory::ConsumptionSelectionError::UnknownStockpile { stockpile } => {
                    EnergyStoreUpgradeError::UnknownSource { stockpile }
                }
                crate::inventory::ConsumptionSelectionError::InsufficientMass {
                    stockpile,
                    available,
                    requested,
                    ..
                } => EnergyStoreUpgradeError::InsufficientMaterial {
                    stockpile,
                    available,
                    required: requested,
                },
                crate::inventory::ConsumptionSelectionError::MassOverflow { stockpile } => {
                    EnergyStoreUpgradeError::SourceMassOverflow { stockpile }
                }
            })?;
    let additions = selection.consumed_inputs().to_vec();
    let egress =
        validate_material_egress_from_selection(state.inventory(), selection).map_err(|error| {
            match error {
                crate::inventory::MaterialEgressError::StaleSelection { expected, actual } => {
                    EnergyStoreUpgradeError::StaleInventorySelection { expected, actual }
                }
                crate::inventory::MaterialEgressError::RevisionExhausted => {
                    EnergyStoreUpgradeError::InventoryRevisionExhausted
                }
            }
        })?;
    let source_record = state
        .inventory()
        .get_stockpile(source)
        .ok_or(EnergyStoreUpgradeError::UnknownSource { stockpile: source })?;
    let source_after = source_record
        .stored_mass()
        .checked_sub(egress.total_consumed())
        .ok_or(EnergyStoreUpgradeError::SourceMassOverflow { stockpile: source })?;
    let structural_load = validate_stockpile_stored_mass_changes(
        registries,
        state,
        [StockpileStoredMassChange::new(source, source_after)],
    )
    .map_err(EnergyStoreUpgradeError::StructuralLoad)?;
    let expected_energy_revision = state.energy().revision();
    let next_energy_revision = expected_energy_revision
        .checked_add(1)
        .ok_or(EnergyStoreUpgradeError::EnergyRevisionExhausted)?;
    Ok(ValidatedEnergyStoreUpgrade {
        store,
        expected_definition: record.definition(),
        target_definition: target,
        expected_embodied_mass: record.embodied_mass(),
        additions,
        expected_energy_revision,
        next_energy_revision,
        egress,
        structural_load,
    })
}

#[cfg(test)]
#[path = "upgrade_execution_tests.rs"]
mod tests;
