//! Root serializable runtime state and cheap invariant enforcement for the simulation.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::core::quantity::Mass;
use crate::inventory::{
    InventoryState, InventoryValidationError, MaterialLotId, StockpileId, validate_loaded_inventory,
};
use crate::material::{CommodityKey, MaterialId};
use crate::production::{
    ProcessId, ProductionJobId, ProductionState, ProductionValidationError, sum_lot_spec_mass,
    validate_loaded_production,
};
use crate::registry::Registries;

use super::rng::{RandomState, RandomStateValidationError, RngStreamId};
use super::time::{SimulationTick, WorldSeed};

/// Mutable runtime state that must survive execution and restart boundaries.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppState {
    world_seed: WorldSeed,
    clock: ClockState,
    random: RandomState,
    inventory: InventoryState,
    production: ProductionState,
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
            inventory: InventoryState::new(),
            production: ProductionState::new(),
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

    /// Returns read-only authoritative stockpile state.
    #[must_use]
    pub const fn inventory(&self) -> &InventoryState {
        &self.inventory
    }

    pub(crate) const fn inventory_state(&self) -> &InventoryState {
        &self.inventory
    }

    pub(crate) fn inventory_state_mut(&mut self) -> &mut InventoryState {
        &mut self.inventory
    }

    /// Returns read-only authoritative production scheduling state.
    #[must_use]
    pub const fn production(&self) -> &ProductionState {
        &self.production
    }

    pub(crate) const fn production_state(&self) -> &ProductionState {
        &self.production
    }

    pub(crate) fn production_state_mut(&mut self) -> &mut ProductionState {
        &mut self.production
    }

    pub(crate) fn random_state_mut(&mut self) -> &mut RandomState {
        &mut self.random
    }
}

/// Error returned when decoded runtime state violates a required persistent invariant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StateValidationError {
    Random(RandomStateValidationError),
    RandomWorldSeedMismatch {
        world_seed: WorldSeed,
        random_seed: WorldSeed,
    },
    Inventory(InventoryValidationError),
    Production(ProductionValidationError),
    UnknownStoredCommodity {
        stockpile: StockpileId,
        commodity: CommodityKey,
    },
    LotCreatedInFuture {
        lot: MaterialLotId,
        created_at: SimulationTick,
        current: SimulationTick,
    },
    LotProvenanceInFuture {
        lot: MaterialLotId,
        latest_created_at: SimulationTick,
        current: SimulationTick,
    },
    UnknownLotCompositionMaterial {
        lot: MaterialLotId,
        material: MaterialId,
    },
    UnknownJobProcess {
        job: ProductionJobId,
        process: ProcessId,
    },
    UnknownJobSource {
        job: ProductionJobId,
        stockpile: StockpileId,
    },
    UnknownJobDestination {
        job: ProductionJobId,
        stockpile: StockpileId,
    },
    JobAlreadyDue {
        job: ProductionJobId,
        current: SimulationTick,
        due: SimulationTick,
    },
    ReservedMassOverflow {
        stockpile: StockpileId,
    },
    UnknownJobOutputCommodity {
        job: ProductionJobId,
        commodity: CommodityKey,
    },
    UnknownJobOutputCompositionMaterial {
        job: ProductionJobId,
        material: MaterialId,
    },
    UnknownJobConsumedCommodity {
        job: ProductionJobId,
        commodity: CommodityKey,
    },
    UnknownJobConsumedCompositionMaterial {
        job: ProductionJobId,
        material: MaterialId,
    },
    JobOutputMassOverflow {
        job: ProductionJobId,
    },
    ReservedInboundMismatch {
        stockpile: StockpileId,
        reserved: Mass,
        expected: Mass,
    },
}

impl Display for StateValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Random(error) => write!(formatter, "invalid random state: {error}"),
            Self::RandomWorldSeedMismatch {
                world_seed,
                random_seed,
            } => write!(
                formatter,
                "world seed {} disagrees with random-state root seed {}",
                world_seed.value(),
                random_seed.value()
            ),
            Self::Inventory(error) => write!(formatter, "invalid inventory state: {error}"),
            Self::Production(error) => write!(formatter, "invalid production state: {error}"),
            Self::UnknownStoredCommodity {
                stockpile,
                commodity,
            } => write!(
                formatter,
                "stockpile {} references unknown material {} or form {}",
                stockpile.value(),
                commodity.material().value(),
                commodity.form().value()
            ),
            Self::LotCreatedInFuture {
                lot,
                created_at,
                current,
            } => write!(
                formatter,
                "material lot {} was created at tick {} after current tick {}",
                lot.value(),
                created_at.value(),
                current.value()
            ),
            Self::LotProvenanceInFuture {
                lot,
                latest_created_at,
                current,
            } => write!(
                formatter,
                "material lot {} contains provenance through tick {} after current tick {}",
                lot.value(),
                latest_created_at.value(),
                current.value()
            ),
            Self::UnknownLotCompositionMaterial { lot, material } => write!(
                formatter,
                "material lot {} composition references unknown material {}",
                lot.value(),
                material.value()
            ),
            Self::UnknownJobConsumedCommodity { job, commodity } => write!(
                formatter,
                "production job {} consumed unknown material {} or form {}",
                job.value(),
                commodity.material().value(),
                commodity.form().value()
            ),
            Self::UnknownJobConsumedCompositionMaterial { job, material } => write!(
                formatter,
                "production job {} consumed-input composition references unknown material {}",
                job.value(),
                material.value()
            ),
            Self::UnknownJobProcess { job, process } => write!(
                formatter,
                "production job {} references unknown process {}",
                job.value(),
                process.value()
            ),
            Self::UnknownJobSource { job, stockpile } => write!(
                formatter,
                "production job {} references missing source stockpile {}",
                job.value(),
                stockpile.value()
            ),
            Self::UnknownJobDestination { job, stockpile } => write!(
                formatter,
                "production job {} references missing destination stockpile {}",
                job.value(),
                stockpile.value()
            ),
            Self::JobAlreadyDue { job, current, due } => write!(
                formatter,
                "production job {} is due at tick {} but current tick is {}",
                job.value(),
                due.value(),
                current.value()
            ),
            Self::ReservedMassOverflow { stockpile } => write!(
                formatter,
                "expected inbound reservations overflow stockpile {} mass accounting",
                stockpile.value()
            ),
            Self::UnknownJobOutputCommodity { job, commodity } => write!(
                formatter,
                "production job {} promises unknown material {} or form {}",
                job.value(),
                commodity.material().value(),
                commodity.form().value()
            ),
            Self::UnknownJobOutputCompositionMaterial { job, material } => write!(
                formatter,
                "production job {} output composition references unknown material {}",
                job.value(),
                material.value()
            ),
            Self::JobOutputMassOverflow { job } => write!(
                formatter,
                "production job {} output mass overflows authoritative quantity storage",
                job.value()
            ),
            Self::ReservedInboundMismatch {
                stockpile,
                reserved,
                expected,
            } => write!(
                formatter,
                "stockpile {} reserves {} mg inbound but active jobs require {} mg",
                stockpile.value(),
                reserved.milligrams(),
                expected.milligrams()
            ),
        }
    }
}

impl Error for StateValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Random(error) => Some(error),
            Self::Inventory(error) => Some(error),
            Self::Production(error) => Some(error),
            Self::RandomWorldSeedMismatch { .. }
            | Self::UnknownStoredCommodity { .. }
            | Self::LotCreatedInFuture { .. }
            | Self::LotProvenanceInFuture { .. }
            | Self::UnknownLotCompositionMaterial { .. }
            | Self::UnknownJobProcess { .. }
            | Self::UnknownJobSource { .. }
            | Self::UnknownJobDestination { .. }
            | Self::JobAlreadyDue { .. }
            | Self::ReservedMassOverflow { .. }
            | Self::UnknownJobOutputCommodity { .. }
            | Self::UnknownJobOutputCompositionMaterial { .. }
            | Self::UnknownJobConsumedCommodity { .. }
            | Self::UnknownJobConsumedCompositionMaterial { .. }
            | Self::JobOutputMassOverflow { .. }
            | Self::ReservedInboundMismatch { .. } => None,
        }
    }
}

/// Validates decoded persistent state before it can re-enter the runtime.
pub fn validate_loaded_state(
    registries: &Registries,
    state: &AppState,
) -> Result<(), StateValidationError> {
    state
        .random
        .validate()
        .map_err(StateValidationError::Random)?;
    if state.random.root_seed() != state.world_seed {
        return Err(StateValidationError::RandomWorldSeedMismatch {
            world_seed: state.world_seed,
            random_seed: state.random.root_seed(),
        });
    }

    validate_loaded_inventory(&state.inventory).map_err(StateValidationError::Inventory)?;
    validate_loaded_production(&state.production).map_err(StateValidationError::Production)?;

    for stockpile in state.inventory.stockpiles() {
        for (commodity, _) in stockpile.contents() {
            if !registries.materials().has_commodity(commodity) {
                return Err(StateValidationError::UnknownStoredCommodity {
                    stockpile: stockpile.id(),
                    commodity,
                });
            }
        }
    }
    for lot in state.inventory.lots() {
        if lot.created_at() > state.tick() {
            return Err(StateValidationError::LotCreatedInFuture {
                lot: lot.id(),
                created_at: lot.created_at(),
                current: state.tick(),
            });
        }
        if lot.latest_created_at() > state.tick() {
            return Err(StateValidationError::LotProvenanceInFuture {
                lot: lot.id(),
                latest_created_at: lot.latest_created_at(),
                current: state.tick(),
            });
        }
        for component in lot.composition().components() {
            if registries
                .materials()
                .get_material(component.material())
                .is_none()
            {
                return Err(StateValidationError::UnknownLotCompositionMaterial {
                    lot: lot.id(),
                    material: component.material(),
                });
            }
        }
    }

    let mut expected_reservations = BTreeMap::<StockpileId, Mass>::new();
    for job in state.production.jobs() {
        if registries.production().get_process(job.process()).is_none() {
            return Err(StateValidationError::UnknownJobProcess {
                job: job.id(),
                process: job.process(),
            });
        }
        if state.inventory.get_stockpile(job.source()).is_none() {
            return Err(StateValidationError::UnknownJobSource {
                job: job.id(),
                stockpile: job.source(),
            });
        }
        if state.inventory.get_stockpile(job.destination()).is_none() {
            return Err(StateValidationError::UnknownJobDestination {
                job: job.id(),
                stockpile: job.destination(),
            });
        }
        if job.completes_at() <= state.tick() {
            return Err(StateValidationError::JobAlreadyDue {
                job: job.id(),
                current: state.tick(),
                due: job.completes_at(),
            });
        }

        for trace in job.consumed_inputs() {
            let commodity = trace.profile().commodity();
            if !registries.materials().has_commodity(commodity) {
                return Err(StateValidationError::UnknownJobConsumedCommodity {
                    job: job.id(),
                    commodity,
                });
            }
            for component in trace.profile().composition().components() {
                if registries
                    .materials()
                    .get_material(component.material())
                    .is_none()
                {
                    return Err(
                        StateValidationError::UnknownJobConsumedCompositionMaterial {
                            job: job.id(),
                            material: component.material(),
                        },
                    );
                }
            }
        }

        for output in job.outputs() {
            if !registries.materials().has_commodity(output.commodity()) {
                return Err(StateValidationError::UnknownJobOutputCommodity {
                    job: job.id(),
                    commodity: output.commodity(),
                });
            }
            for component in output.composition().components() {
                if registries
                    .materials()
                    .get_material(component.material())
                    .is_none()
                {
                    return Err(StateValidationError::UnknownJobOutputCompositionMaterial {
                        job: job.id(),
                        material: component.material(),
                    });
                }
            }
        }
        let output_mass = sum_lot_spec_mass(job.outputs())
            .ok_or(StateValidationError::JobOutputMassOverflow { job: job.id() })?;
        let current = expected_reservations
            .get(&job.destination())
            .copied()
            .unwrap_or(Mass::ZERO);
        let expected =
            current
                .checked_add(output_mass)
                .ok_or(StateValidationError::ReservedMassOverflow {
                    stockpile: job.destination(),
                })?;
        expected_reservations.insert(job.destination(), expected);
    }

    for stockpile in state.inventory.stockpiles() {
        let expected = expected_reservations
            .get(&stockpile.id())
            .copied()
            .unwrap_or(Mass::ZERO);
        if stockpile.reserved_inbound() != expected {
            return Err(StateValidationError::ReservedInboundMismatch {
                stockpile: stockpile.id(),
                reserved: stockpile.reserved_inbound(),
                expected,
            });
        }
    }

    Ok(())
}

/// Asserts every cheap runtime invariant in debug builds.
pub fn validate_invariants(_registries: &Registries, state: &AppState) {
    debug_assert!(
        state.random.has_valid_core_stream(),
        "Runtime Invariant 11 (Serialization Completeness): core RNG stream must remain valid"
    );
    debug_assert!(
        state.inventory.has_valid_id_cursors(),
        "Runtime Invariant 8 (No Lost Runtime State): inventory ID cursors must remain nonzero"
    );
    debug_assert!(
        state.production.has_valid_id_cursor(),
        "Runtime Invariant 8 (No Lost Runtime State): production ID cursor must remain nonzero"
    );
    debug_assert!(
        state
            .production
            .earliest_due_tick()
            .is_none_or(|due| due > state.tick()),
        "Runtime Invariant 6 (Lifecycle Validity): no active production job may remain due"
    );
}

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
    use crate::content::{
        FORM_LOG, FORM_LUMP, MATERIAL_CHARCOAL, MATERIAL_WOOD, build_registries,
        make_test_registries_with_process,
    };
    use crate::core::quantity::{Mass, Temperature};
    use crate::core::rng::RngAlgorithm;
    use crate::inventory::{add_stockpile, deposit_bulk_for_test, validate_transfer_bulk};
    use crate::material::{CommodityKey, MaterialInputSpec, MaterialLotSpec};
    use crate::matter::calculate_matter_accounting;
    use crate::production::{
        ProcessDefinition, ProcessId, ProcessResolution, make_test_process_resolution,
        validate_start_process,
    };
    use crate::simulation::advance_tick;

    const SOAK_PROCESS: ProcessId = ProcessId::new(900_201);

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

    fn make_test_soak_resolution() -> ProcessResolution {
        make_test_process_resolution(
            SOAK_PROCESS,
            29,
            vec![MaterialLotSpec::new(
                CommodityKey::new(MATERIAL_CHARCOAL, FORM_LUMP),
                Mass::from_milligrams(10),
                Temperature::from_millikelvin(450_000),
            )],
        )
    }

    fn add_soak_stockpile(state: &mut AppState, capacity: u64) -> crate::inventory::StockpileId {
        match add_stockpile(state, Mass::from_milligrams(capacity)) {
            Ok(id) => id,
            Err(error) => panic!("soak stockpile allocation failed: {error}"),
        }
    }

    fn schedule_soak_process(
        registries: &Registries,
        resolution: &ProcessResolution,
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
        let token = match validate_start_process(registries, state, resolution, source, processing)
        {
            Ok(token) => token,
            Err(error) => panic!("soak process validation failed: {error}"),
        };
        if let Err(error) = token.commit(state) {
            panic!("soak process commit failed: {error}");
        }
    }

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
        let token = match validate_transfer_bulk(
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

    fn run_test_soak(seed: WorldSeed) -> AppState {
        let registries = make_test_registries_with_process(make_test_soak_process());
        let resolution = make_test_soak_resolution();
        let mut state = AppState::new(seed);
        let source = add_soak_stockpile(&mut state, 30_000);
        let processing = add_soak_stockpile(&mut state, 10_000);
        let archive = add_soak_stockpile(&mut state, 10_000);
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
                schedule_soak_process(
                    &registries,
                    &resolution,
                    &mut state,
                    source,
                    processing,
                    wood,
                );
            }
            if step % 17 == 0 {
                transfer_soak_output(&registries, &mut state, processing, archive, charcoal);
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

    #[test]
    fn test_headless_production_soak_preserves_invariants_and_determinism() {
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
