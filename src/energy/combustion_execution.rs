//! Exact local solid-fuel combustion into finite thermal storage.

use crate::core::quantity::{AggregateMass, Energy, Mass};
use crate::core::state::AppState;
use crate::inventory::{
    ConsumptionSelection, ConsumptionSelectionError, MaterialStackProjection,
    MaterialStackProjectionError, StockpileId, StockpileStoredMassChange,
    TemperatureLimitedSelectionError, ValidatedMaterialEgress, ValidatedStockpileStructuralLoad,
    apply_material_egress, validate_explicit_consumption_selection,
    validate_material_egress_from_selection, validate_stockpile_stored_mass_changes,
    validate_temperature_limited_consumption_selection,
    validate_unreserved_stockpile_structural_load_headroom,
};
use crate::logistics::{validate_player_energy_store_access, validate_player_stockpile_access};
use crate::material::{CommodityKey, MAX_HAND_SAFE_MATERIAL_TEMPERATURE, MaterialInputSpec};
use crate::player::{PlayerActionGuard, validate_optional_player_action};
use crate::registry::Registries;

use super::{EnergyStoreId, energy_store_occupancy};

mod errors;
pub use errors::{CombustionCommitError, CombustionError};

#[must_use]
pub struct ValidatedCombustion {
    player_action: PlayerActionGuard,
    store: EnergyStoreId,
    expected_stored: Energy,
    released_energy: Energy,
    consumed_mass: Mass,
    fuel_material: crate::material::MaterialId,
    next_combusted_mass: AggregateMass,
    expected_energy_revision: u64,
    next_energy_revision: u64,
    expected_logistics_revision: u64,
    expected_combat_revision: u64,
    egress: ValidatedMaterialEgress,
    structural_load: Option<ValidatedStockpileStructuralLoad>,
}

impl ValidatedCombustion {
    /// Exact fuel mass that this validated charge will consume.
    #[must_use]
    pub const fn consumed_mass(&self) -> Mass {
        self.consumed_mass
    }

    /// Thermal energy this exact selected charge will release on commit.
    #[must_use]
    pub const fn released_energy(&self) -> Energy {
        self.released_energy
    }

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
        if state.logistics().player().is_some() && state.player_work().active().is_some() {
            return Err(CombustionCommitError::PlayerBecameBusy);
        }
        if state.combat().revision() != self.expected_combat_revision {
            return Err(CombustionCommitError::StaleCombatRevision {
                expected: self.expected_combat_revision,
                actual: state.combat().revision(),
            });
        }
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
    validate_combust_fuel_selected(registries, state, store, source, FuelSelection::Default)
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
    validate_combust_fuel_selected(
        registries,
        state,
        store,
        source,
        FuelSelection::Commodity(fuel),
    )
}

/// Burns up to one authored charge from one physically selected stack, including smaller
/// pure remnants, without borrowing from another same-commodity cohort. Full charges retain
/// their authored heat output; fragments release proportionate, rounded-down nanojoules.
/// The projection must still match its inventory revision and simulation tick.
pub fn validate_combust_stack_fuel(
    registries: &Registries,
    state: &AppState,
    store: EnergyStoreId,
    source: StockpileId,
    stack: &MaterialStackProjection,
) -> Result<ValidatedCombustion, CombustionError> {
    validate_combust_fuel_selected(
        registries,
        state,
        store,
        source,
        FuelSelection::Stack(stack),
    )
}

enum FuelSelection<'a> {
    Default,
    Commodity(CommodityKey),
    Stack(&'a MaterialStackProjection),
}

fn validate_combust_fuel_selected(
    registries: &Registries,
    state: &AppState,
    store: EnergyStoreId,
    source: StockpileId,
    selected_fuel: FuelSelection<'_>,
) -> Result<ValidatedCombustion, CombustionError> {
    let player_action =
        validate_optional_player_action(state).map_err(|_| CombustionError::PlayerDead)?;
    if state.logistics().player().is_some() && state.player_work().active().is_some() {
        return Err(CombustionError::PlayerBusy);
    }
    if state.logistics().player().is_some() && state.combat().has_prepared_strike() {
        return Err(CombustionError::PreparedStrikeActive);
    }
    // Authorize both physical endpoints before exposing store identity or contents.
    validate_player_energy_store_access(state, store).map_err(CombustionError::StoreAccess)?;
    validate_player_stockpile_access(state, source).map_err(CombustionError::SourceAccess)?;
    let record = state
        .energy()
        .get_store(store)
        .ok_or(CombustionError::UnknownStore { store })?;
    let definition = registries
        .energy()
        .get_store(record.definition())
        .ok_or(CombustionError::UnknownDefinition)?;
    let profile = definition
        .combustion_profile()
        .ok_or(CombustionError::NoCombustionProfile { store })?;
    let fuel = match selected_fuel {
        FuelSelection::Default => None,
        FuelSelection::Commodity(fuel) => Some(fuel),
        FuelSelection::Stack(stack) => Some(stack.commodity()),
    };
    let profile = if let Some(fuel) = fuel {
        definition
            .combustion_profile_for(fuel)
            .ok_or(CombustionError::UnsupportedFuel { store, fuel })?
    } else {
        profile
    };
    if energy_store_occupancy(state, store).is_some() {
        return Err(CombustionError::StoreBusy { store });
    }
    // A numbered physical stack can burn a genuine leftover rather than leaving
    // unusable fuel after the last whole charge. Commodity/default admission still
    // selects one full authored charge, so those paths never silently change size.
    let fuel_mass = match selected_fuel {
        FuelSelection::Stack(stack) => stack.mass().min(profile.fuel_mass()),
        FuelSelection::Default | FuelSelection::Commodity(_) => profile.fuel_mass(),
    };
    let full_mass = u128::from(profile.fuel_mass().milligrams());
    let portion = u128::from(fuel_mass.milligrams());
    let full_energy = profile.released_energy().nanojoules();
    // Split the division before multiplying. Both products fit u128 even when
    // an authored charge approaches the quantity limits, and truncation never
    // grants extra heat for a fractional nanojoule.
    let released =
        (full_energy / full_mass) * portion + ((full_energy % full_mass) * portion) / full_mass;
    if released == 0 {
        return Err(CombustionError::FractionalChargeTooSmall {
            available: fuel_mass,
        });
    }
    let released_energy = Energy::from_nanojoules(released);
    let stored_after = record.stored().checked_add(released_energy).ok_or(
        CombustionError::InsufficientThermalCapacity {
            store,
            stored: record.stored(),
            released: released_energy,
            capacity: definition.capacity(),
        },
    )?;
    if stored_after > definition.capacity() {
        return Err(CombustionError::InsufficientThermalCapacity {
            store,
            stored: record.stored(),
            released: released_energy,
            capacity: definition.capacity(),
        });
    }
    let selection = select_fuel(state, source, selected_fuel, profile.fuel(), fuel_mass)?;
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
        .checked_add(AggregateMass::from_mass(fuel_mass))
        .ok_or(CombustionError::CombustedMassOverflow)?;
    let expected_energy_revision = state.energy().revision();
    let next_energy_revision = expected_energy_revision
        .checked_add(1)
        .unwrap_or_else(|| unreachable!("energy headroom check includes combustion revision"));
    Ok(ValidatedCombustion {
        player_action,
        store,
        expected_stored: record.stored(),
        released_energy,
        consumed_mass: fuel_mass,
        fuel_material,
        next_combusted_mass,
        expected_energy_revision,
        next_energy_revision,
        expected_logistics_revision: state.logistics().revision(),
        expected_combat_revision: state.combat().revision(),
        egress,
        structural_load,
    })
}

/// Select only hand-safe, pure fuel. A form-based request may skip hot cohorts;
/// an exact displayed stack must never substitute another cohort.
fn select_fuel(
    state: &AppState,
    source: StockpileId,
    selected_fuel: FuelSelection<'_>,
    fuel: CommodityKey,
    fuel_mass: Mass,
) -> Result<ConsumptionSelection, CombustionError> {
    let input = [MaterialInputSpec::pure(fuel, fuel_mass)];
    match selected_fuel {
        FuelSelection::Stack(stack) => {
            let slices = stack
                .selections_for_mass(state, source, fuel_mass)
                .map_err(|error| match error {
                    MaterialStackProjectionError::MassExceedsStack {
                        requested,
                        available,
                    } => CombustionError::InsufficientFuel {
                        source,
                        available,
                        required: requested,
                    },
                    error @ (MaterialStackProjectionError::UnknownStockpile { .. }
                    | MaterialStackProjectionError::StaleInventoryRevision { .. }
                    | MaterialStackProjectionError::StaleSimulationTick { .. }
                    | MaterialStackProjectionError::StackMembershipChanged { .. }
                    | MaterialStackProjectionError::SourceMismatch { .. }
                    | MaterialStackProjectionError::ZeroMass) => {
                        CombustionError::StackSelection(error)
                    }
                })?;
            if !input[0].is_satisfied_by(stack.composition()) {
                return Err(CombustionError::ImpureFuel { fuel });
            }
            if stack.temperature() > MAX_HAND_SAFE_MATERIAL_TEMPERATURE {
                return Err(CombustionError::FuelTooHot {
                    fuel,
                    temperature: stack.temperature(),
                    maximum: MAX_HAND_SAFE_MATERIAL_TEMPERATURE,
                });
            }
            validate_explicit_consumption_selection(state.inventory(), source, &slices)
                .map_err(CombustionError::ExplicitSelection)
        }
        FuelSelection::Default | FuelSelection::Commodity(_) => {
            validate_temperature_limited_consumption_selection(
                state.inventory(),
                source,
                &input,
                MAX_HAND_SAFE_MATERIAL_TEMPERATURE,
            )
            .map_err(|error| match error {
                TemperatureLimitedSelectionError::Selection(
                    ConsumptionSelectionError::UnknownStockpile { stockpile },
                ) => CombustionError::SourceMassOverflow { source: stockpile },
                TemperatureLimitedSelectionError::Selection(
                    ConsumptionSelectionError::InsufficientMass {
                        available,
                        requested,
                        ..
                    },
                ) => CombustionError::InsufficientFuel {
                    source,
                    available,
                    required: requested,
                },
                TemperatureLimitedSelectionError::Selection(
                    ConsumptionSelectionError::MassOverflow { .. },
                ) => CombustionError::SourceMassOverflow { source },
                TemperatureLimitedSelectionError::TooHot {
                    commodity,
                    temperature,
                    maximum,
                } => CombustionError::FuelTooHot {
                    fuel: commodity,
                    temperature,
                    maximum,
                },
            })
        }
    }
}

#[cfg(test)]
#[path = "combustion_execution_tests.rs"]
mod tests;
