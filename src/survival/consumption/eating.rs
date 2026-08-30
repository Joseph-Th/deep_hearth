//! Exact conserved food-consumption transaction and physiological reserve resolution.

mod errors;
mod resolution;

use std::collections::BTreeMap;

use crate::core::quantity::{AggregateMass, AggregateVolume, Energy, Mass};
use crate::core::state::AppState;
use crate::inventory::{
    ExplicitConsumptionSelectionError, MaterialEgressError, MaterialLotId, MaterialLotSelection,
    StockpileId, StockpileStoredMassChange, ValidatedMaterialEgress,
    ValidatedStockpileStructuralLoad, apply_material_egress,
    validate_explicit_consumption_selection, validate_material_egress_from_selection,
    validate_stockpile_stored_mass_changes,
};
use crate::labor::{
    EatingWork, PlayerAttentionError, PlayerWork, ValidatedPlayerAttentionHold,
    validate_player_attention,
};
use crate::material::MaterialId;
use crate::registry::Registries;

use super::super::FoodCategory;
use super::super::state::PendingEating;
pub use errors::{EatCommitError, EatError};
use resolution::{meal_absorption_offer, resolve_meal_offer};

pub(crate) use resolution::trace_absorption_offer;

#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EatPortionOutcome {
    lot: MaterialLotId,
    mass: Mass,
    category: FoodCategory,
}

impl EatPortionOutcome {
    #[must_use]
    pub const fn lot(&self) -> MaterialLotId {
        self.lot
    }
    #[must_use]
    pub const fn mass(&self) -> Mass {
        self.mass
    }
    #[must_use]
    pub const fn category(&self) -> FoodCategory {
        self.category
    }
}

/// Normalized nutrition offered by one eating action before per-tick reserve caps are applied.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NutritionGain {
    grain_ppm: u32,
    fruit_ppm: u32,
    protein_ppm: u32,
}

impl NutritionGain {
    #[must_use]
    pub(super) const fn from_parts_per_million(
        grain_ppm: u32,
        fruit_ppm: u32,
        protein_ppm: u32,
    ) -> Self {
        Self {
            grain_ppm,
            fruit_ppm,
            protein_ppm,
        }
    }

    #[must_use]
    pub const fn get(self, category: FoodCategory) -> u32 {
        match category {
            FoodCategory::Grain => self.grain_ppm,
            FoodCategory::Fruit => self.fruit_ppm,
            FoodCategory::Protein => self.protein_ppm,
        }
    }

    #[must_use]
    pub const fn total_ppm(self) -> u32 {
        self.grain_ppm + self.fruit_ppm + self.protein_ppm
    }
}

#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EatOutcome {
    portions: Vec<EatPortionOutcome>,
    total_mass: Mass,
    energy_offered: Energy,
    hydration_offered: AggregateVolume,
    nutrition_offered: NutritionGain,
}

impl EatOutcome {
    pub fn portions(&self) -> &[EatPortionOutcome] {
        &self.portions
    }
    #[must_use]
    pub const fn total_mass(&self) -> Mass {
        self.total_mass
    }
    #[must_use]
    pub const fn energy_offered(&self) -> Energy {
        self.energy_offered
    }
    #[must_use]
    pub const fn hydration_offered(&self) -> AggregateVolume {
        self.hydration_offered
    }
    #[must_use]
    pub const fn nutrition_offered(&self) -> NutritionGain {
        self.nutrition_offered
    }
}

#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct ValidatedEat {
    attention: ValidatedPlayerAttentionHold,
    expected_survival_revision: u64,
    next_survival_revision: u64,
    egress: ValidatedMaterialEgress,
    structural: Option<ValidatedStockpileStructuralLoad>,
    pending: PendingEating,
    next_consumed_masses: Vec<(MaterialId, AggregateMass)>,
    outcome: EatOutcome,
}

fn map_eat_selection_error(error: ExplicitConsumptionSelectionError) -> EatError {
    match error {
        ExplicitConsumptionSelectionError::UnknownStockpile { stockpile } => {
            EatError::UnknownStockpile { stockpile }
        }
        ExplicitConsumptionSelectionError::EmptySelection => EatError::EmptySelection,
        ExplicitConsumptionSelectionError::ZeroMass { lot } => EatError::ZeroMass { lot },
        ExplicitConsumptionSelectionError::DuplicateLot { lot } => EatError::DuplicateLot { lot },
        ExplicitConsumptionSelectionError::UnknownLot { lot } => EatError::UnknownLot { lot },
        ExplicitConsumptionSelectionError::LotOwnedElsewhere {
            lot,
            requested_source,
            actual_source,
        } => EatError::LotOwnedElsewhere {
            lot,
            requested_source,
            actual_source,
        },
        ExplicitConsumptionSelectionError::InsufficientLotMass {
            lot,
            available,
            requested,
        } => EatError::InsufficientLotMass {
            lot,
            available,
            requested,
        },
        ExplicitConsumptionSelectionError::MassOverflow { stockpile } => {
            EatError::InventoryMassOverflow { stockpile }
        }
    }
}

fn resolve_consumed_mass_totals(
    state: &AppState,
    additions: BTreeMap<MaterialId, AggregateMass>,
) -> Result<Vec<(MaterialId, AggregateMass)>, EatError> {
    additions
        .into_iter()
        .map(|(material, addition)| {
            state
                .survival()
                .consumed_mass(material)
                .checked_add(addition)
                .map(|next| (material, next))
                .ok_or(EatError::ConsumedMatterOverflow { material })
        })
        .collect()
}

pub fn validate_eat(
    registries: &Registries,
    state: &AppState,
    source: StockpileId,
    selections: &[MaterialLotSelection],
) -> Result<ValidatedEat, EatError> {
    let attention = validate_player_attention(state).map_err(|error| match error {
        PlayerAttentionError::SurvivalNotInitialized => EatError::SurvivalNotInitialized,
        PlayerAttentionError::PlayerDead => EatError::PlayerDead,
        PlayerAttentionError::Busy { active } => EatError::PlayerBusy { active },
    })?;
    let exact_selection =
        validate_explicit_consumption_selection(state.inventory(), source, selections)
            .map_err(map_eat_selection_error)?;
    let physiology = registries.survival().physiology();
    let player = state
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| unreachable!("validated player attention requires survival state"));
    if player.metabolic_energy() > physiology.maximum_metabolic_energy() {
        return Err(EatError::MetabolicEnergyOverflow);
    }
    if player.hydration() > physiology.maximum_hydration() {
        return Err(EatError::HydrationOverflow);
    }
    let total_mass = exact_selection.total_consumed();
    let maximum_meal_mass = physiology.direct_consumption().maximum_meal_mass();
    if total_mass > maximum_meal_mass {
        return Err(EatError::MealMassExceedsIntakeLimit {
            mass: total_mass,
            maximum: maximum_meal_mass,
        });
    }
    let duration = physiology
        .direct_consumption()
        .meal_duration(total_mass)
        .unwrap_or_else(|| unreachable!("validated nonzero bounded meal must have a duration"));
    let completes_at = state
        .tick()
        .checked_add_span(duration)
        .ok_or(EatError::CompletionTickOverflow { duration })?;
    let attention = attention
        .hold(PlayerWork::Eating {
            work: EatingWork::new(total_mass, state.tick(), completes_at),
        })
        .ok_or(EatError::PlayerWorkRevisionExhausted)?;
    let offer = resolve_meal_offer(registries, state, selections)?;

    let egress = validate_material_egress_from_selection(state.inventory(), exact_selection)
        .map_err(|error| match error {
            MaterialEgressError::StaleSelection {
                expected: _,
                actual: _,
            } => unreachable!("synchronous eating selection cannot become stale before validation"),
            MaterialEgressError::RevisionExhausted => EatError::InventoryRevisionExhausted,
        })?;
    let source_record = state
        .inventory()
        .get_stockpile(source)
        .ok_or(EatError::UnknownStockpile { stockpile: source })?;
    let source_after = source_record
        .stored_mass()
        .checked_sub(egress.total_consumed())
        .ok_or(EatError::InventoryMassOverflow { stockpile: source })?;
    let structural = validate_stockpile_stored_mass_changes(
        registries,
        state,
        [StockpileStoredMassChange::new(source, source_after)],
    )
    .map_err(EatError::StructuralLoad)?;
    let expected_survival_revision = state.survival().revision();
    let next_survival_revision = expected_survival_revision
        .checked_add(1)
        .ok_or(EatError::SurvivalRevisionExhausted)?;
    let absorption_offer = meal_absorption_offer(&offer, physiology.maximum_metabolic_energy())?;
    let next_consumed_masses = resolve_consumed_mass_totals(state, offer.consumed_additions)?;
    let pending = PendingEating::new(
        egress.consumed_inputs().to_vec(),
        state.tick(),
        completes_at,
    );

    Ok(ValidatedEat {
        attention,
        expected_survival_revision,
        next_survival_revision,
        egress,
        structural,
        pending,
        next_consumed_masses,
        outcome: EatOutcome {
            portions: offer.portions,
            total_mass,
            energy_offered: absorption_offer.energy(),
            hydration_offered: absorption_offer.hydration(),
            nutrition_offered: absorption_offer.nutrition(),
        },
    })
}

impl ValidatedEat {
    pub fn commit(self, state: &mut AppState) -> Result<EatOutcome, EatCommitError> {
        if let Err(conflict) = self.attention.precheck(state) {
            return Err(EatCommitError::StalePlayerWorkRevision {
                expected: conflict.expected(),
                actual: conflict.actual(),
            });
        }
        let actual_survival_revision = state.survival().revision();
        if actual_survival_revision != self.expected_survival_revision {
            return Err(EatCommitError::StaleSurvivalRevision {
                expected: self.expected_survival_revision,
                actual: actual_survival_revision,
            });
        }
        let actual_inventory_revision = state.inventory().revision();
        if actual_inventory_revision != self.egress.expected_revision() {
            return Err(EatCommitError::StaleInventoryRevision {
                expected: self.egress.expected_revision(),
                actual: actual_inventory_revision,
            });
        }
        self.egress.assert_matches_state(state.inventory());
        state.survival().assert_direct_consumption_begin_available(
            self.expected_survival_revision,
            self.next_survival_revision,
        );
        if let Some(structural) = self.structural {
            structural
                .commit(state)
                .map_err(EatCommitError::Structure)?;
        }
        apply_material_egress(state.inventory_state_mut(), self.egress);
        state.survival_state_mut().begin_food_consumption(
            self.expected_survival_revision,
            self.next_survival_revision,
            self.pending,
            self.next_consumed_masses,
        );
        self.attention.apply(state);
        Ok(self.outcome)
    }
}
