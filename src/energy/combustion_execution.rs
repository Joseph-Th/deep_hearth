//! Exact local solid-fuel combustion into finite thermal storage.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{AggregateMass, Energy, Mass};
use crate::core::state::AppState;
use crate::inventory::{
    StockpileId, StockpileStoredMassChange, StockpileStructuralLoadError, ValidatedMaterialEgress,
    ValidatedStockpileStructuralLoad, apply_material_egress, validate_consumption_selection,
    validate_material_egress_from_selection, validate_stockpile_stored_mass_changes,
    validate_unreserved_stockpile_structural_load_headroom,
};
use crate::logistics::{
    PlayerEnergyStoreAccessError, PlayerStockpileAccessError, validate_player_energy_store_access,
    validate_player_stockpile_access,
};
use crate::material::{CommodityKey, MaterialInputSpec};
use crate::player::{PlayerActionGuard, validate_optional_player_action};
use crate::registry::Registries;
use crate::structural::StructuralCommitError;

use super::{EnergyStoreId, energy_store_occupancy};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CombustionError {
    PlayerDead,
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
    StoreBusy {
        store: EnergyStoreId,
    },
    InsufficientFuel {
        source: StockpileId,
        available: Mass,
        required: Mass,
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
            Self::PlayerDead
            | Self::UnknownStore { .. }
            | Self::UnknownDefinition
            | Self::NoCombustionProfile { .. }
            | Self::UnsupportedFuel { .. }
            | Self::StoreBusy { .. }
            | Self::InsufficientFuel { .. }
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
            Self::StaleSurvival { .. }
            | Self::StaleLogistics { .. }
            | Self::StaleInventory { .. }
            | Self::StaleEnergy { .. }
            | Self::StoreChanged { .. } => None,
        }
    }
}

#[must_use]
pub struct ValidatedCombustion {
    player_action: PlayerActionGuard,
    store: EnergyStoreId,
    expected_stored: Energy,
    released_energy: Energy,
    fuel_material: crate::material::MaterialId,
    next_combusted_mass: AggregateMass,
    expected_energy_revision: u64,
    next_energy_revision: u64,
    expected_logistics_revision: u64,
    egress: ValidatedMaterialEgress,
    structural_load: Option<ValidatedStockpileStructuralLoad>,
}

impl ValidatedCombustion {
    pub fn commit(self, state: &mut AppState) -> Result<Energy, CombustionCommitError> {
        if state.logistics().revision() != self.expected_logistics_revision {
            return Err(CombustionCommitError::StaleLogistics {
                expected: self.expected_logistics_revision,
                actual: state.logistics().revision(),
            });
        }
        self.player_action.precheck(state).map_err(|stale| {
            CombustionCommitError::StaleSurvival {
                expected: stale.expected,
                actual: stale.actual,
            }
        })?;
        if state.inventory().revision() != self.egress.expected_revision() {
            return Err(CombustionCommitError::StaleInventory {
                expected: self.egress.expected_revision(),
                actual: state.inventory().revision(),
            });
        }
        if state.energy().revision() != self.expected_energy_revision {
            return Err(CombustionCommitError::StaleEnergy {
                expected: self.expected_energy_revision,
                actual: state.energy().revision(),
            });
        }
        let Some(record) = state.energy().get_store(self.store) else {
            return Err(CombustionCommitError::StoreChanged { store: self.store });
        };
        if record.stored() != self.expected_stored
            || energy_store_occupancy(state, self.store).is_some()
        {
            return Err(CombustionCommitError::StoreChanged { store: self.store });
        }
        self.egress.assert_matches_state(state.inventory());
        if let Some(load) = self.structural_load {
            load.commit(state)
                .map_err(CombustionCommitError::Structure)?;
        }
        apply_material_egress(state.inventory_state_mut(), self.egress);
        state.energy_state_mut().apply_combustion(
            self.store,
            self.released_energy,
            self.fuel_material,
            self.next_combusted_mass,
            self.expected_energy_revision,
            self.next_energy_revision,
        );
        Ok(self.released_energy)
    }
}

/// Validates one authored fuel charge into a local finite thermal store.
pub fn validate_combust_fuel(
    registries: &Registries,
    state: &AppState,
    store: EnergyStoreId,
    source: StockpileId,
) -> Result<ValidatedCombustion, CombustionError> {
    validate_combust_fuel_selected(registries, state, store, source, None)
}

/// Uses one explicitly selected fuel form. Refuses to substitute logs when chips are insufficient,
/// so reusing byproduct chips is an actual player decision with a distinct heat budget.
pub fn validate_combust_selected_fuel(
    registries: &Registries,
    state: &AppState,
    store: EnergyStoreId,
    source: StockpileId,
    fuel: CommodityKey,
) -> Result<ValidatedCombustion, CombustionError> {
    validate_combust_fuel_selected(registries, state, store, source, Some(fuel))
}

fn validate_combust_fuel_selected(
    registries: &Registries,
    state: &AppState,
    store: EnergyStoreId,
    source: StockpileId,
    selected_fuel: Option<CommodityKey>,
) -> Result<ValidatedCombustion, CombustionError> {
    let player_action =
        validate_optional_player_action(state).map_err(|_| CombustionError::PlayerDead)?;
    let record = state
        .energy()
        .get_store(store)
        .ok_or(CombustionError::UnknownStore { store })?;
    validate_player_energy_store_access(state, store).map_err(CombustionError::StoreAccess)?;
    validate_player_stockpile_access(state, source).map_err(CombustionError::SourceAccess)?;
    let definition = registries
        .energy()
        .get_store(record.definition())
        .ok_or(CombustionError::UnknownDefinition)?;
    let profile = definition
        .combustion_profile()
        .ok_or(CombustionError::NoCombustionProfile { store })?;
    let profile = if let Some(fuel) = selected_fuel {
        definition
            .combustion_profile_for(fuel)
            .ok_or(CombustionError::UnsupportedFuel { store, fuel })?
    } else {
        profile
    };
    if energy_store_occupancy(state, store).is_some() {
        return Err(CombustionError::StoreBusy { store });
    }
    let stored_after = record
        .stored()
        .checked_add(profile.released_energy())
        .ok_or(CombustionError::InsufficientThermalCapacity {
            store,
            stored: record.stored(),
            released: profile.released_energy(),
            capacity: definition.capacity(),
        })?;
    if stored_after > definition.capacity() {
        return Err(CombustionError::InsufficientThermalCapacity {
            store,
            stored: record.stored(),
            released: profile.released_energy(),
            capacity: definition.capacity(),
        });
    }
    let input = [MaterialInputSpec::pure(profile.fuel(), profile.fuel_mass())];
    let selection =
        validate_consumption_selection(state.inventory(), source, &input).map_err(|error| {
            match error {
                crate::inventory::ConsumptionSelectionError::UnknownStockpile { stockpile } => {
                    CombustionError::SourceMassOverflow { source: stockpile }
                }
                crate::inventory::ConsumptionSelectionError::InsufficientMass {
                    available,
                    requested,
                    ..
                } => CombustionError::InsufficientFuel {
                    source,
                    available,
                    required: requested,
                },
                crate::inventory::ConsumptionSelectionError::MassOverflow { .. } => {
                    CombustionError::SourceMassOverflow { source }
                }
            }
        })?;
    let egress =
        validate_material_egress_from_selection(state.inventory(), selection).map_err(|error| {
            match error {
                crate::inventory::MaterialEgressError::StaleSelection { expected, actual } => {
                    CombustionError::StaleInventorySelection { expected, actual }
                }
                crate::inventory::MaterialEgressError::RevisionExhausted => {
                    CombustionError::InventoryRevisionExhausted
                }
            }
        })?;
    let source_after = egress.source_stored_mass_after(state.inventory());
    let structural_load = validate_stockpile_stored_mass_changes(
        registries,
        state,
        [StockpileStoredMassChange::new(source, source_after)],
    )
    .map_err(CombustionError::StructuralLoad)?;
    validate_unreserved_stockpile_structural_load_headroom(state, structural_load.as_ref())
        .map_err(CombustionError::StructuralLoad)?;
    if !state.can_spend_inventory_revisions(1) {
        return Err(CombustionError::InventoryRevisionExhausted);
    }
    if !state.can_spend_energy_revisions(1) {
        return Err(CombustionError::EnergyRevisionExhausted);
    }
    let fuel_material = profile.fuel().material();
    let next_combusted_mass = state
        .energy()
        .combusted_mass(fuel_material)
        .checked_add(AggregateMass::from_mass(profile.fuel_mass()))
        .ok_or(CombustionError::CombustedMassOverflow)?;
    let expected_energy_revision = state.energy().revision();
    let next_energy_revision = expected_energy_revision
        .checked_add(1)
        .unwrap_or_else(|| unreachable!("energy headroom check includes combustion revision"));
    Ok(ValidatedCombustion {
        player_action,
        store,
        expected_stored: record.stored(),
        released_energy: profile.released_energy(),
        fuel_material,
        next_combusted_mass,
        expected_energy_revision,
        next_energy_revision,
        expected_logistics_revision: state.logistics().revision(),
        egress,
        structural_load,
    })
}

#[cfg(test)]
#[path = "combustion_execution_tests.rs"]
mod tests;
