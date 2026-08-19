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
pub struct AppState {
    world_seed: WorldSeed,
    clock: ClockState,
    random: RandomState,
    systems: SystemState,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
pub(crate) fn make_test_state_at_tick(world_seed: WorldSeed, tick: SimulationTick) -> AppState {
    let mut state = AppState::new(world_seed);
    state.clock.tick = tick;
    state
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::build_registries;
    #[cfg(feature = "test-soak")]
    use crate::registry::Registries;

    #[cfg(feature = "test-soak")]
    use crate::content::{
        FORM_LOG, FORM_LUMP, MATERIAL_CHARCOAL, MATERIAL_WOOD,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION, make_test_registries_with_process,
    };

    #[cfg(feature = "test-soak")]
    use crate::core::quantity::{Area, Force, Mass, Temperature};
    use crate::core::rng::RngAlgorithm;

    #[cfg(feature = "test-soak")]
    use crate::inventory::{
        add_solid_stockpile_for_test, deposit_bulk_for_test, validate_material_transfer_for_test,
    };

    #[cfg(feature = "test-soak")]
    use crate::material::{CommodityKey, MaterialInputSpec, MaterialLotSpec};

    #[cfg(feature = "test-soak")]
    use crate::matter::calculate_matter_accounting;

    #[cfg(feature = "test-soak")]
    use crate::production::{
        ProcessDefinition, ProcessId, ProcessResolution, make_test_process_resolution,
        validate_process_inputs, validate_start_process,
    };

    #[cfg(feature = "test-soak")]
    use crate::simulation::advance_tick;

    #[cfg(feature = "test-soak")]
    use crate::spatial::{VoxelBounds, VoxelCoord};

    #[cfg(feature = "test-soak")]
    use crate::structural::{
        StructuralElementId, StructuralLoadKind, StructuralMutationOutcome,
        ValidatedStructuralMutation, add_structural_element,
        materialize_structural_element_for_test, validate_activate_structural_element,
        validate_link_support, validate_set_structural_load,
    };

    #[cfg(feature = "test-soak")]
    const SOAK_PROCESS: ProcessId = ProcessId::new(900_201);

    #[cfg(feature = "test-soak")]
    fn make_test_soak_process() -> ProcessDefinition {
        ProcessDefinition::new(
            SOAK_PROCESS,
            "soak material transform",
            vec![MaterialInputSpec::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
                Mass::from_milligrams(10),
            )],
            Vec::new(),
        )
    }

    #[cfg(feature = "test-soak")]
    fn make_test_soak_resolution(
        registries: &Registries,
        state: &AppState,
        source: crate::inventory::StockpileId,
    ) -> ProcessResolution {
        let inputs = match validate_process_inputs(registries, state, SOAK_PROCESS, source) {
            Ok(inputs) => inputs,
            Err(error) => panic!("soak process input binding failed: {error}"),
        };
        make_test_process_resolution(
            inputs,
            29,
            vec![MaterialLotSpec::new(
                CommodityKey::new(MATERIAL_CHARCOAL, FORM_LUMP),
                Mass::from_milligrams(10),
                Temperature::from_millikelvin(450_000),
            )],
        )
    }

    #[cfg(feature = "test-soak")]
    fn add_soak_stockpile(state: &mut AppState, capacity: u64) -> crate::inventory::StockpileId {
        match add_solid_stockpile_for_test(state, Mass::from_milligrams(capacity)) {
            Ok(id) => id,
            Err(error) => panic!("soak stockpile allocation failed: {error}"),
        }
    }

    #[cfg(feature = "test-soak")]
    fn make_soak_structural_bounds(x: i64, y: i64) -> VoxelBounds {
        match VoxelBounds::new(VoxelCoord::new(x, y, 0), VoxelCoord::new(x + 1, y + 1, 1)) {
            Ok(bounds) => bounds,
            Err(error) => panic!("soak structural bounds failed: {error}"),
        }
    }

    #[cfg(feature = "test-soak")]
    fn add_soak_structural_element(
        registries: &Registries,
        state: &mut AppState,
        x: i64,
        y: i64,
        is_grounded: bool,
    ) -> StructuralElementId {
        let element = match add_structural_element(
            registries,
            state,
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            crate::structural::make_test_structural_geometry(
                make_soak_structural_bounds(x, y),
                crate::core::quantity::Length::from_micrometers(1),
                Area::from_square_millimeters(1_000),
            ),
            is_grounded,
        ) {
            Ok(element) => element,
            Err(error) => panic!("soak structural element allocation failed: {error}"),
        };
        materialize_structural_element_for_test(registries, state, element, FORM_LOG);
        element
    }

    #[cfg(feature = "test-soak")]
    fn commit_soak_structural_mutation(
        token: ValidatedStructuralMutation,
        state: &mut AppState,
    ) -> StructuralMutationOutcome {
        match token.commit(state) {
            Ok(outcome) => outcome,
            Err(error) => panic!("soak structural mutation failed: {error}"),
        }
    }

    #[cfg(feature = "test-soak")]
    fn build_soak_structure(registries: &Registries, state: &mut AppState) -> StructuralElementId {
        let left = add_soak_structural_element(registries, state, 0, 0, true);
        let right = add_soak_structural_element(registries, state, 2, 0, true);
        let deck = add_soak_structural_element(registries, state, 1, 1, false);

        for element in [left, right] {
            let token = match validate_activate_structural_element(registries, state, element) {
                Ok(token) => token,
                Err(error) => panic!("soak structural support activation failed: {error}"),
            };
            commit_soak_structural_mutation(token, state);
        }
        for support in [left, right] {
            let token = match validate_link_support(registries, state, deck, support) {
                Ok(token) => token,
                Err(error) => panic!("soak structural support link failed: {error}"),
            };
            commit_soak_structural_mutation(token, state);
        }
        let activation = match validate_activate_structural_element(registries, state, deck) {
            Ok(token) => token,
            Err(error) => panic!("soak deck activation failed: {error}"),
        };
        commit_soak_structural_mutation(activation, state);

        // Begin the soak with visible persistent damage but below post-crack failure capacity.
        let initial_load = match validate_set_structural_load(
            registries,
            state,
            deck,
            StructuralLoadKind::Snow,
            Force::from_millinewtons(35_000_000),
        ) {
            Ok(token) => token,
            Err(error) => panic!("soak initial structural load failed: {error}"),
        };
        let outcome = commit_soak_structural_mutation(initial_load, state);
        assert_eq!(outcome.analysis().damage_events().len(), 1);
        deck
    }

    #[cfg(feature = "test-soak")]
    fn vary_soak_structural_load(
        registries: &Registries,
        state: &mut AppState,
        deck: StructuralElementId,
        step: u64,
    ) {
        let load = if (step / 19).is_multiple_of(2) {
            Force::from_millinewtons(20_000_000)
        } else {
            Force::from_millinewtons(35_000_000)
        };
        let token = match validate_set_structural_load(
            registries,
            state,
            deck,
            StructuralLoadKind::Snow,
            load,
        ) {
            Ok(token) => token,
            Err(error) => panic!("soak structural load validation failed at step {step}: {error}"),
        };
        let outcome = commit_soak_structural_mutation(token, state);
        assert!(
            outcome.analysis().damage_events().is_empty(),
            "soak structural load generated unexpected new damage at step {step}"
        );
    }

    #[cfg(feature = "test-soak")]
    fn schedule_soak_process(
        registries: &Registries,
        state: &mut AppState,
        source: crate::inventory::StockpileId,
        processing: crate::inventory::StockpileId,
        wood: CommodityKey,
    ) {
        let available = match state.inventory().get_stockpile(source) {
            Some(record) => record.get_mass(wood),
            None => panic!("soak source disappeared"),
        };
        if available < Mass::from_milligrams(10) {
            return;
        }
        let resolution = make_test_soak_resolution(registries, state, source);
        let token = match validate_start_process(registries, state, &resolution, source, processing)
        {
            Ok(token) => token,
            Err(error) => panic!("soak process validation failed: {error}"),
        };
        if let Err(error) = token.commit(state) {
            panic!("soak process commit failed: {error}");
        }
    }

    #[cfg(feature = "test-soak")]
    fn transfer_soak_output(
        registries: &Registries,
        state: &mut AppState,
        processing: crate::inventory::StockpileId,
        archive: crate::inventory::StockpileId,
        charcoal: CommodityKey,
    ) {
        let available = match state.inventory().get_stockpile(processing) {
            Some(record) => record.get_mass(charcoal),
            None => panic!("soak processing stockpile disappeared"),
        };
        if available < Mass::from_milligrams(1) {
            return;
        }
        let token = match validate_material_transfer_for_test(
            registries,
            state,
            processing,
            archive,
            charcoal,
            Mass::from_milligrams(1),
        ) {
            Ok(token) => token,
            Err(error) => panic!("soak transfer validation failed: {error}"),
        };
        if let Err(error) = token.commit(state) {
            panic!("soak transfer commit failed: {error}");
        }
    }

    #[cfg(feature = "test-soak")]
    fn run_test_soak(seed: WorldSeed) -> AppState {
        let registries = make_test_registries_with_process(make_test_soak_process());
        let mut state = AppState::new(seed);
        let source = add_soak_stockpile(&mut state, 30_000);
        let processing = add_soak_stockpile(&mut state, 10_000);
        let archive = add_soak_stockpile(&mut state, 10_000);
        let structural_deck = build_soak_structure(&registries, &mut state);
        let wood = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
        let charcoal = CommodityKey::new(MATERIAL_CHARCOAL, FORM_LUMP);
        if let Err(error) = deposit_bulk_for_test(
            &registries,
            &mut state,
            source,
            wood,
            Mass::from_milligrams(20_000),
        ) {
            panic!("soak source deposit failed: {error}");
        }
        let initial_matter = match calculate_matter_accounting(&state) {
            Ok(accounting) => accounting.total(),
            Err(error) => panic!("soak initial matter accounting failed: {error}"),
        };

        for step in 0_u64..10_000 {
            if step % 11 == 0 {
                schedule_soak_process(&registries, &mut state, source, processing, wood);
            }
            if step % 17 == 0 {
                transfer_soak_output(&registries, &mut state, processing, archive, charcoal);
            }
            if step % 19 == 0 {
                vary_soak_structural_load(&registries, &mut state, structural_deck, step);
            }
            if let Err(error) = advance_tick(&registries, &mut state) {
                panic!("soak tick {step} failed: {error}");
            }
            if step % 257 == 0
                && let Err(error) = validate_loaded_state(&registries, &state)
            {
                panic!("soak exhaustive audit failed at step {step}: {error}");
            }
            if step % 257 == 0 {
                let accounted = match calculate_matter_accounting(&state) {
                    Ok(accounting) => accounting.total(),
                    Err(error) => panic!("soak matter accounting failed at step {step}: {error}"),
                };
                assert_eq!(
                    accounted, initial_matter,
                    "soak matter ownership changed at step {step}"
                );
            }
        }

        if let Err(error) = validate_loaded_state(&registries, &state) {
            panic!("soak final state failed validation: {error}");
        }
        let final_matter = match calculate_matter_accounting(&state) {
            Ok(accounting) => accounting.total(),
            Err(error) => panic!("soak final matter accounting failed: {error}"),
        };
        assert_eq!(final_matter, initial_matter);
        state
    }

    #[test]
    fn new_state_starts_at_zero_with_versioned_rng() {
        let registries = build_registries();
        let state = AppState::new(WorldSeed::new(42));

        assert_eq!(state.world_seed(), WorldSeed::new(42));
        assert_eq!(state.tick(), SimulationTick::ZERO);
        assert_eq!(state.rng_algorithm(), RngAlgorithm::Xoshiro256StarStarV1);
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }

    #[cfg(feature = "test-soak")]
    #[test]
    #[ignore = "long-horizon soak"]
    fn test_headless_mixed_system_soak_preserves_invariants_and_determinism() {
        let seed = WorldSeed::new(0x5A0C_D37E_4D11_0001);
        let first = run_test_soak(seed);
        let second = run_test_soak(seed);

        assert_eq!(first, second);
        assert_eq!(first.tick(), SimulationTick::new(10_000));
        assert!(
            first.inventory().lots().count() <= 8,
            "soak generated unbounded material-lot fragmentation"
        );
    }
}
