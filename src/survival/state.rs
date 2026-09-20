//! Owns persistent player survival state and terminal consumption accounting.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::core::quantity::{AggregateMass, AggregateVolume};
use crate::fluid::FluidDefinitionId;
use crate::material::MaterialId;

use super::PhysiologyDefinition;

mod direct_consumption;
mod player;

pub(crate) use direct_consumption::{
    DirectConsumptionState, PendingDirectConsumption, PendingDrinking, PendingEating,
};
pub(crate) use player::player_record;
pub use player::{NutritionReserves, PlayerSurvivalRecord, Vitality};

pub const NUTRITION_PARTS_PER_MILLION: u32 = crate::core::arithmetic::NORMALIZED_PARTS_PER_MILLION;

/// Persistent owner for survival state. A fresh simulation has no player until explicitly admitted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurvivalState {
    revision: u64,
    player: Option<PlayerSurvivalRecord>,
    direct_consumption: DirectConsumptionState,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    consumed_matter: BTreeMap<MaterialId, AggregateMass>,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    consumed_fluids: BTreeMap<FluidDefinitionId, AggregateVolume>,
}

impl SurvivalState {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            revision: 0,
            player: None,
            direct_consumption: DirectConsumptionState::new(),
            consumed_matter: BTreeMap::new(),
            consumed_fluids: BTreeMap::new(),
        }
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub(crate) const fn can_advance_revision_by(&self, steps: u64) -> bool {
        self.revision.checked_add(steps).is_some()
    }

    #[must_use]
    pub const fn player(&self) -> Option<&PlayerSurvivalRecord> {
        self.player.as_ref()
    }

    #[must_use]
    pub(crate) const fn pending_direct_consumption(&self) -> Option<&PendingDirectConsumption> {
        self.direct_consumption.pending()
    }

    pub(crate) fn has_valid_player_bounds(&self, physiology: PhysiologyDefinition) -> bool {
        self.player.is_none_or(|player| {
            player.metabolic_energy() <= physiology.maximum_metabolic_energy()
                && player.hydration() <= physiology.maximum_hydration()
                && player.vitality().parts_per_million() <= Vitality::MAXIMUM.parts_per_million()
                && player.nutrition().has_valid_bounds()
                && player.vitality_recovery_remainder() < NUTRITION_PARTS_PER_MILLION
                && (player.vitality() != Vitality::MAXIMUM
                    || player.vitality_recovery_remainder() == 0)
        })
    }

    /// Iterates food matter transferred out of inventory into the terminal survival-consumption
    /// conservation boundary.
    ///
    /// This is cumulative consumed matter, not live body mass. Biological transformation and waste
    /// streams are outside the current simulation scope, so this terminal owner closes accounting.
    pub(crate) fn consumed_matter(&self) -> impl Iterator<Item = (MaterialId, AggregateMass)> + '_ {
        self.consumed_matter
            .iter()
            .map(|(material, mass)| (*material, *mass))
    }

    pub(crate) fn consumed_mass(&self, material: MaterialId) -> AggregateMass {
        self.consumed_matter
            .get(&material)
            .copied()
            .unwrap_or(AggregateMass::ZERO)
    }

    /// Iterates fluid volume transferred out of stores into the terminal survival-consumption
    /// conservation boundary. This is cumulative consumed volume, not current body water.
    pub(crate) fn consumed_fluids(
        &self,
    ) -> impl Iterator<Item = (FluidDefinitionId, AggregateVolume)> + '_ {
        self.consumed_fluids
            .iter()
            .map(|(fluid, volume)| (*fluid, *volume))
    }

    pub(crate) fn consumed_fluid_volume(&self, fluid: FluidDefinitionId) -> AggregateVolume {
        self.consumed_fluids
            .get(&fluid)
            .copied()
            .unwrap_or(AggregateVolume::ZERO)
    }

    pub(crate) fn apply_player(
        &mut self,
        expected_revision: u64,
        next_revision: u64,
        player: PlayerSurvivalRecord,
    ) {
        assert_eq!(
            self.revision, expected_revision,
            "survival mutation requires its validated owner revision"
        );
        assert_eq!(
            self.revision.checked_add(1),
            Some(next_revision),
            "survival mutation must advance revision exactly once"
        );
        self.player = Some(player);
        self.revision = next_revision;
    }

    pub(crate) fn apply_player_and_direct_consumption(
        &mut self,
        expected_revision: u64,
        next_revision: u64,
        player: PlayerSurvivalRecord,
        pending: Option<PendingDirectConsumption>,
    ) {
        self.apply_player(expected_revision, next_revision, player);
        self.direct_consumption.set_pending(pending);
    }

    pub(crate) fn assert_direct_consumption_begin_available(
        &self,
        expected_revision: u64,
        next_revision: u64,
    ) {
        assert_eq!(
            self.revision, expected_revision,
            "direct consumption requires its validated survival revision"
        );
        assert_eq!(
            expected_revision.checked_add(1),
            Some(next_revision),
            "direct consumption must advance survival revision exactly once"
        );
        self.direct_consumption.assert_begin_available();
    }

    pub(crate) fn begin_food_consumption(
        &mut self,
        expected_revision: u64,
        next_revision: u64,
        pending: PendingEating,
        next_consumed_masses: Vec<(MaterialId, AggregateMass)>,
    ) {
        self.assert_direct_consumption_begin_available(expected_revision, next_revision);
        self.advance_revision(expected_revision, next_revision);
        self.direct_consumption
            .begin(PendingDirectConsumption::Eating(pending));
        for (material, mass) in next_consumed_masses {
            self.consumed_matter.insert(material, mass);
        }
    }

    pub(crate) fn begin_fluid_consumption(
        &mut self,
        expected_revision: u64,
        next_revision: u64,
        pending: PendingDrinking,
        fluid: FluidDefinitionId,
        next_consumed_volume: AggregateVolume,
    ) {
        self.assert_direct_consumption_begin_available(expected_revision, next_revision);
        self.advance_revision(expected_revision, next_revision);
        self.direct_consumption
            .begin(PendingDirectConsumption::Drinking(pending));
        self.consumed_fluids.insert(fluid, next_consumed_volume);
    }

    fn advance_revision(&mut self, expected_revision: u64, next_revision: u64) {
        assert_eq!(
            self.revision, expected_revision,
            "survival mutation requires its validated owner revision"
        );
        assert_eq!(
            self.revision.checked_add(1),
            Some(next_revision),
            "survival mutation must advance revision exactly once"
        );
        self.revision = next_revision;
    }
}
