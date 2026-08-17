//! Persistent player survival state; mutation is owned by sibling survival transactions.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::core::quantity::{AggregateMass, AggregateVolume, Energy, Volume};
use crate::fluid::FluidDefinitionId;
use crate::material::MaterialId;

use super::PhysiologyDefinition;

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

/// Persistent survival quantities for the single locally controlled player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerSurvivalRecord {
    metabolic_energy: Energy,
    hydration: Volume,
    vitality: Vitality,
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
}

/// Persistent owner for survival state. A fresh simulation has no player until explicitly admitted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurvivalState {
    revision: u64,
    player: Option<PlayerSurvivalRecord>,
    metabolic_matter: BTreeMap<MaterialId, AggregateMass>,
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
        material: MaterialId,
        next_metabolic_mass: AggregateMass,
    ) {
        self.apply_player(expected_revision, next_revision, player);
        self.metabolic_matter.insert(material, next_metabolic_mass);
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
) -> PlayerSurvivalRecord {
    PlayerSurvivalRecord {
        metabolic_energy,
        hydration,
        vitality,
    }
}
