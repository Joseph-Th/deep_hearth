//! Canonical per-tick survival resource costs shared by execution and work admission.

use crate::core::quantity::{Energy, Volume};

use super::definitions::PhysiologyDefinition;

/// Additional per-tick physiological cost of the player's current physical work.
///
/// Basal metabolism remains authored by `PhysiologyDefinition`; work owners contribute only the
/// incremental cost above rest so simulation can combine them without creating a second metabolism
/// path.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SurvivalExertion {
    energy_cost_per_tick: Energy,
    hydration_loss_per_tick: Volume,
}

impl SurvivalExertion {
    pub const REST: Self = Self {
        energy_cost_per_tick: Energy::ZERO,
        hydration_loss_per_tick: Volume::ZERO,
    };

    #[must_use]
    pub const fn new(energy_cost_per_tick: Energy, hydration_loss_per_tick: Volume) -> Self {
        Self {
            energy_cost_per_tick,
            hydration_loss_per_tick,
        }
    }

    #[must_use]
    pub const fn energy_cost_per_tick(self) -> Energy {
        self.energy_cost_per_tick
    }

    #[must_use]
    pub const fn hydration_loss_per_tick(self) -> Volume {
        self.hydration_loss_per_tick
    }

    /// Rejects a resting profile where an authored action represents active physical player work.
    pub(crate) const fn assert_active_player_work(self) {
        assert!(
            !self.energy_cost_per_tick.is_zero(),
            "active player work exertion must consume metabolic energy"
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SurvivalTickResourceCostError {
    EnergyOverflow,
    HydrationOverflow,
}

/// Exact metabolic-energy and hydration cost of one authoritative simulation tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SurvivalTickResourceCost {
    metabolic_energy: Energy,
    hydration: Volume,
}

impl SurvivalTickResourceCost {
    #[must_use]
    pub(crate) const fn metabolic_energy(self) -> Energy {
        self.metabolic_energy
    }

    #[must_use]
    pub(crate) const fn hydration(self) -> Volume {
        self.hydration
    }
}

/// Resolves basal physiology plus the incremental cost of the current player work.
///
/// Survival tick execution and player-work admission both consume this owner calculation so their
/// resource arithmetic cannot drift into distinct definitions of the same physical tick.
pub(crate) fn resolve_survival_tick_resource_cost(
    physiology: PhysiologyDefinition,
    exertion: SurvivalExertion,
) -> Result<SurvivalTickResourceCost, SurvivalTickResourceCostError> {
    let metabolic_energy = physiology
        .basal_energy_cost_per_tick()
        .checked_add(exertion.energy_cost_per_tick())
        .ok_or(SurvivalTickResourceCostError::EnergyOverflow)?;
    let hydration = physiology
        .hydration_loss_per_tick()
        .checked_add(exertion.hydration_loss_per_tick())
        .ok_or(SurvivalTickResourceCostError::HydrationOverflow)?;
    Ok(SurvivalTickResourceCost {
        metabolic_energy,
        hydration,
    })
}

#[cfg(test)]
#[path = "resource_cost_tests.rs"]
mod tests;
