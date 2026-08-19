//! Persistent player survival state; mutation is owned by sibling survival transactions.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::core::quantity::{AggregateMass, AggregateVolume, Energy, Volume};
use crate::fluid::FluidDefinitionId;
use crate::material::MaterialId;

use super::{FoodCategory, PhysiologyDefinition};

pub const NUTRITION_PARTS_PER_MILLION: u32 = 1_000_000;

/// Player vitality in normalized parts per million.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Vitality(u32);

impl Vitality {
    pub const MAXIMUM: Self = Self(1_000_000);
    pub const ZERO: Self = Self(0);

    #[must_use]
    pub const fn parts_per_million(self) -> u32 {
        self.0
    }

    pub(crate) const fn from_parts_per_million_unchecked(value: u32) -> Self {
        Self(value)
    }
}

/// Persistent recent dietary contribution by broad food category.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NutritionReserves {
    grain: u32,
    fruit: u32,
    protein: u32,
}

impl NutritionReserves {
    pub const FULL: Self = Self {
        grain: NUTRITION_PARTS_PER_MILLION,
        fruit: NUTRITION_PARTS_PER_MILLION,
        protein: NUTRITION_PARTS_PER_MILLION,
    };

    #[must_use]
    pub const fn get(self, category: FoodCategory) -> u32 {
        match category {
            FoodCategory::Grain => self.grain,
            FoodCategory::Fruit => self.fruit,
            FoodCategory::Protein => self.protein,
        }
    }

    #[must_use]
    pub const fn quality_ppm(self) -> u32 {
        ((self.grain as u64 + self.fruit as u64 + self.protein as u64) / 3) as u32
    }

    pub(crate) fn add(self, category: FoodCategory, gain: u32) -> (Self, u32) {
        let before = self.get(category);
        let after = before.saturating_add(gain).min(NUTRITION_PARTS_PER_MILLION);
        let mut next = self;
        match category {
            FoodCategory::Grain => next.grain = after,
            FoodCategory::Fruit => next.fruit = after,
            FoodCategory::Protein => next.protein = after,
        }
        (next, after - before)
    }

    pub(crate) fn decay(self, amount: u32) -> Self {
        Self {
            grain: self.grain.saturating_sub(amount),
            fruit: self.fruit.saturating_sub(amount),
            protein: self.protein.saturating_sub(amount),
        }
    }

    pub(crate) const fn has_valid_bounds(self) -> bool {
        self.grain <= NUTRITION_PARTS_PER_MILLION
            && self.fruit <= NUTRITION_PARTS_PER_MILLION
            && self.protein <= NUTRITION_PARTS_PER_MILLION
    }

    #[cfg(test)]
    pub(crate) const fn from_parts_per_million(grain: u32, fruit: u32, protein: u32) -> Self {
        Self {
            grain,
            fruit,
            protein,
        }
    }
}

/// Persistent survival quantities for the single locally controlled player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerSurvivalRecord {
    metabolic_energy: Energy,
    hydration: Volume,
    vitality: Vitality,
    nutrition: NutritionReserves,
}

impl PlayerSurvivalRecord {
    #[must_use]
    pub const fn metabolic_energy(self) -> Energy {
        self.metabolic_energy
    }

    #[must_use]
    pub const fn hydration(self) -> Volume {
        self.hydration
    }

    #[must_use]
    pub const fn vitality(self) -> Vitality {
        self.vitality
    }

    #[must_use]
    pub const fn nutrition(self) -> NutritionReserves {
        self.nutrition
    }
}

/// Persistent owner for survival state. A fresh simulation has no player until explicitly admitted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurvivalState {
    revision: u64,
    player: Option<PlayerSurvivalRecord>,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    metabolic_matter: BTreeMap<MaterialId, AggregateMass>,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    ingested_fluids: BTreeMap<FluidDefinitionId, AggregateVolume>,
}

impl SurvivalState {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            revision: 0,
            player: None,
            metabolic_matter: BTreeMap::new(),
            ingested_fluids: BTreeMap::new(),
        }
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn player(&self) -> Option<&PlayerSurvivalRecord> {
        self.player.as_ref()
    }

    pub(crate) fn has_valid_player_bounds(&self, physiology: PhysiologyDefinition) -> bool {
        self.player.is_none_or(|player| {
            player.metabolic_energy() <= physiology.maximum_metabolic_energy()
                && player.hydration() <= physiology.maximum_hydration()
                && player.vitality().parts_per_million() <= Vitality::MAXIMUM.parts_per_million()
                && player.nutrition().has_valid_bounds()
        })
    }

    /// Iterates material mass currently owned by the biological metabolism boundary.
    pub(crate) fn metabolic_matter(
        &self,
    ) -> impl Iterator<Item = (MaterialId, AggregateMass)> + '_ {
        self.metabolic_matter
            .iter()
            .map(|(material, mass)| (*material, *mass))
    }

    pub(crate) fn metabolic_mass(&self, material: MaterialId) -> AggregateMass {
        self.metabolic_matter
            .get(&material)
            .copied()
            .unwrap_or(AggregateMass::ZERO)
    }

    /// Iterates finite fluid volume transferred into the biological survival owner.
    pub(crate) fn ingested_fluids(
        &self,
    ) -> impl Iterator<Item = (FluidDefinitionId, AggregateVolume)> + '_ {
        self.ingested_fluids
            .iter()
            .map(|(fluid, volume)| (*fluid, *volume))
    }

    pub(crate) fn ingested_fluid_volume(&self, fluid: FluidDefinitionId) -> AggregateVolume {
        self.ingested_fluids
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

    pub(crate) fn apply_food_ingestion(
        &mut self,
        expected_revision: u64,
        next_revision: u64,
        player: PlayerSurvivalRecord,
        next_metabolic_masses: Vec<(MaterialId, AggregateMass)>,
    ) {
        self.apply_player(expected_revision, next_revision, player);
        for (material, mass) in next_metabolic_masses {
            self.metabolic_matter.insert(material, mass);
        }
    }

    pub(crate) fn apply_fluid_ingestion(
        &mut self,
        expected_revision: u64,
        next_revision: u64,
        player: PlayerSurvivalRecord,
        fluid: FluidDefinitionId,
        next_ingested_volume: AggregateVolume,
    ) {
        self.apply_player(expected_revision, next_revision, player);
        self.ingested_fluids.insert(fluid, next_ingested_volume);
    }
}

pub(crate) const fn player_record(
    metabolic_energy: Energy,
    hydration: Volume,
    vitality: Vitality,
    nutrition: NutritionReserves,
) -> PlayerSurvivalRecord {
    PlayerSurvivalRecord {
        metabolic_energy,
        hydration,
        vitality,
        nutrition,
    }
}
