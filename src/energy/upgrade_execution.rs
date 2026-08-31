//! Additive, matter-conserving upgrades of empty and idle finite energy stores.

use crate::core::quantity::{Energy, Mass};
use crate::core::state::AppState;
use crate::inventory::{
    ConsumedMaterialTrace, StockpileId, StockpileStoredMassChange, ValidatedMaterialEgress,
    ValidatedStockpileStructuralLoad, apply_material_egress, validate_consumption_selection,
    validate_material_egress_from_selection, validate_stockpile_stored_mass_changes,
};
use crate::registry::Registries;

use super::state::EnergyStoreUpgradeMutation;
use super::{EnergyStoreDefinitionId, EnergyStoreId};

mod errors;

pub use errors::{EnergyStoreUpgradeCommitError, EnergyStoreUpgradeError};

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
        self.egress.assert_matches_state(state.inventory());
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
        let mutation = EnergyStoreUpgradeMutation {
            store: self.store,
            expected_definition: self.expected_definition,
            target_definition: self.target_definition,
            expected_embodied_mass: self.expected_embodied_mass,
            additions: self.additions,
        };
        state.energy().assert_upgrade_available(
            &mutation,
            self.expected_energy_revision,
            self.next_energy_revision,
        );
        if let Some(load) = self.structural_load {
            load.commit(state)
                .map_err(EnergyStoreUpgradeCommitError::Structure)?;
        }
        apply_material_egress(state.inventory_state_mut(), self.egress);
        state.energy_state_mut().apply_upgrade(
            mutation,
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
