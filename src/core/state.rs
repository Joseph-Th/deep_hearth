//! Root serializable runtime state; child validation audits persistence and cheap runtime invariants.

use serde::{Deserialize, Serialize};

use crate::energy::EnergyState;
use crate::equipment::EquipmentState;
use crate::fluid::FluidState;
use crate::geology::{GeologicalKnowledgeState, GeologyState};
use crate::inventory::InventoryState;
use crate::labor::PlayerWorkState;
use crate::mining::MiningState;
use crate::production::ProductionState;
use crate::structural::StructureState;
use crate::survival::SurvivalState;

use super::rng::{RandomState, RngStreamId};
use super::time::{SimulationTick, WorldSeed};

/// Mutable runtime state that must survive execution and restart boundaries.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppState {
    world_seed: WorldSeed,
    clock: ClockState,
    random: RandomState,
    systems: SystemState,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SystemState {
    energy: EnergyState,
    fluid: FluidState,
    equipment: EquipmentState,
    structures: StructureState,
    geology: GeologyState,
    geological_knowledge: GeologicalKnowledgeState,
    inventory: InventoryState,
    production: ProductionState,
    mining: MiningState,
    player_work: PlayerWorkState,
    survival: SurvivalState,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClockState {
    tick: SimulationTick,
}

impl AppState {
    /// Builds a fresh deterministic runtime state for one world.
    #[must_use]
    pub fn new(world_seed: WorldSeed) -> Self {
        Self {
            world_seed,
            clock: ClockState {
                tick: SimulationTick::ZERO,
            },
            random: RandomState::new(world_seed),
            systems: SystemState {
                energy: EnergyState::new(),
                fluid: FluidState::new(),
                equipment: EquipmentState::new(),
                structures: StructureState::new(),
                geology: GeologyState::new(),
                geological_knowledge: GeologicalKnowledgeState::new(),
                inventory: InventoryState::new(),
                production: ProductionState::new(),
                mining: MiningState::new(),
                player_work: PlayerWorkState::new(),
                survival: SurvivalState::new(),
            },
        }
    }

    /// Returns the immutable world seed.
    #[must_use]
    pub const fn world_seed(&self) -> WorldSeed {
        self.world_seed
    }

    /// Returns the current authoritative simulation tick.
    #[must_use]
    pub const fn tick(&self) -> SimulationTick {
        self.clock.tick
    }

    /// Returns the persisted random algorithm identity without exposing mutable PRNG state.
    #[must_use]
    pub fn rng_algorithm(&self) -> super::rng::RngAlgorithm {
        match self.random.stream_algorithm(RngStreamId::CORE) {
            Some(algorithm) => algorithm,
            None => panic!("runtime invariant broken: core random stream is missing"),
        }
    }

    /// Returns read-only authoritative finite-energy state.
    #[must_use]
    pub const fn energy(&self) -> &EnergyState {
        &self.systems.energy
    }

    pub(crate) fn energy_state_mut(&mut self) -> &mut EnergyState {
        &mut self.systems.energy
    }

    /// Returns read-only authoritative finite fluid state.
    #[must_use]
    pub const fn fluid(&self) -> &FluidState {
        &self.systems.fluid
    }

    pub(crate) fn fluid_state_mut(&mut self) -> &mut FluidState {
        &mut self.systems.fluid
    }

    /// Returns read-only authoritative equipment state.
    #[must_use]
    pub const fn equipment(&self) -> &EquipmentState {
        &self.systems.equipment
    }

    pub(crate) fn equipment_state_mut(&mut self) -> &mut EquipmentState {
        &mut self.systems.equipment
    }

    /// Returns read-only authoritative structural state.
    #[must_use]
    pub const fn structures(&self) -> &StructureState {
        &self.systems.structures
    }

    pub(crate) fn structure_state_mut(&mut self) -> &mut StructureState {
        &mut self.systems.structures
    }

    /// Returns authoritative geological truth to owning core systems only.
    ///
    /// Player-facing adapters must use `geological_knowledge()` rather than enumerating hidden
    /// deposit records directly.
    #[must_use]
    pub(crate) const fn geology(&self) -> &GeologyState {
        &self.systems.geology
    }

    pub(crate) fn geology_state_mut(&mut self) -> &mut GeologyState {
        &mut self.systems.geology
    }

    /// Returns acquired geological evidence without exposing it as authoritative world truth.
    #[must_use]
    pub const fn geological_knowledge(&self) -> &GeologicalKnowledgeState {
        &self.systems.geological_knowledge
    }

    pub(crate) fn geological_knowledge_state_mut(&mut self) -> &mut GeologicalKnowledgeState {
        &mut self.systems.geological_knowledge
    }

    /// Returns read-only authoritative stockpile state.
    #[must_use]
    pub const fn inventory(&self) -> &InventoryState {
        &self.systems.inventory
    }

    pub(crate) fn inventory_state_mut(&mut self) -> &mut InventoryState {
        &mut self.systems.inventory
    }

    pub(crate) fn rebuild_derived_indexes(&mut self) {
        self.systems.inventory.rebuild_derived_indexes();
        self.systems.equipment.rebuild_derived_indexes();
        self.systems.fluid.rebuild_derived_indexes();
        self.systems.structures.rebuild_derived_indexes();
        self.systems.geological_knowledge.rebuild_derived_indexes();
        self.systems.production.rebuild_derived_indexes();
        self.systems.mining.rebuild_derived_indexes();
    }

    /// Returns read-only authoritative production scheduling state.
    #[must_use]
    pub const fn production(&self) -> &ProductionState {
        &self.systems.production
    }

    pub(crate) fn production_state_mut(&mut self) -> &mut ProductionState {
        &mut self.systems.production
    }

    /// Returns read-only durable geological extraction work.
    #[must_use]
    pub const fn mining(&self) -> &MiningState {
        &self.systems.mining
    }

    pub(crate) fn mining_state_mut(&mut self) -> &mut MiningState {
        &mut self.systems.mining
    }

    /// Returns the local player's exclusive active-work owner.
    #[must_use]
    pub const fn player_work(&self) -> &PlayerWorkState {
        &self.systems.player_work
    }

    pub(crate) fn player_work_state_mut(&mut self) -> &mut PlayerWorkState {
        &mut self.systems.player_work
    }

    /// Returns read-only authoritative player survival state.
    #[must_use]
    pub const fn survival(&self) -> &SurvivalState {
        &self.systems.survival
    }

    pub(crate) fn survival_state_mut(&mut self) -> &mut SurvivalState {
        &mut self.systems.survival
    }
}

mod validation;

pub use validation::{StateValidationError, validate_invariants, validate_loaded_state};

pub(crate) fn apply_clock_advance(state: &mut AppState, next_tick: SimulationTick) {
    state.clock.tick = next_tick;
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
