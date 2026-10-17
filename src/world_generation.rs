//! Stateless deterministic creation of the currently implemented initial wilderness opportunity.
//!
//! Generated loose matter and finite opening water immediately enter their canonical persistent
//! owners. This module owns only fresh-world authorization and deterministic authored generation
//! inputs; it is not a terrain, chunk, ecology, hydrology, or movement owner.

mod opening;

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::content::{
    FLUID_WATER, FORM_FOOD, FORM_LOG, FORM_LUMP, MATERIAL_BERRIES, MATERIAL_STONE, MATERIAL_WOOD,
};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::AppState;
use crate::core::time::{ControlTick, SimulationTick};
use crate::fluid::{
    FluidGenerationCommitError, FluidGenerationError, FluidStoreId, GeneratedFluidStoreSpec,
    ValidatedFluidGeneration, validate_generate_fluid_store,
};
use crate::logistics::TraversalEdge;
use crate::material::{CommodityKey, MaterialComposition};
use crate::player::{PlayerAdmissionState, player_admission_state};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::{
    SurfaceResourceCreationCommitError, SurfaceResourceCreationError, SurfaceResourceKind,
    SurfaceResourceSpec, SurfaceResourceSpecError, SurfaceResourceValidationError,
    ValidatedSurfaceResourceCreation, validate_create_surface_resources,
};
use crate::world::{BlockCollisionShape, BlockStateId, WorldGenerationSeed};

use opening::{
    OPENING_RESOURCE_ORDER_SALT, generated_mass, generated_opening_layout,
    generated_opening_layout_matching, generated_volume, local_entropy,
};

const WILDERNESS_TEMPERATURE: Temperature = Temperature::from_millikelvin(293_150);
const STONE_BASE_MASS_MG: u64 = 3_000_000;
const STONE_VARIATION_MG: u64 = 1_500_000;
// One local fallen-timber opportunity should be a reserve, not a pocket-sized pickup. The player
// can still harvest only the amount current work justifies, but even the smallest discovered stand
// exceeds the unaided body-carrying envelope so storage, repeated hauling, and later woodworking
// can compete for the same known source.
const TIMBER_BASE_MASS_MG: u64 = 30_000_000;
const TIMBER_VARIATION_MG: u64 = 10_000_000;
// The opening food and water opportunities are deliberately larger than the actor's first quick
// serving. At minimum they each represent more than one resting day of authored physiology, so a
// fresh player can choose to secure a little, return later, or invest in storage instead of being
// forced back into subsistence every few in-game hours.
const BERRIES_BASE_MASS_MG: u64 = 4_000_000;
const BERRIES_VARIATION_MG: u64 = 2_000_000;
const OPENING_WATER_BASE_VOLUME_UL: u64 = 4_000_000;
const OPENING_WATER_VARIATION_UL: u64 = 2_000_000;
const OPENING_WATER_VOLUME_SALT: u64 = 0x5741_5445_525F_5350;

/// Number of persisted traversal edges in the built-in opening opportunity graph.
pub const INITIAL_WILDERNESS_ACCESS_EDGES: usize = 5;
/// Minimum opening-graph legs to visit both material leaves from spawn and return to spawn.
///
/// The optional water branch is not part of this material-acquisition route.
pub const INITIAL_WILDERNESS_MATERIAL_ROUND_TRIP_LEGS: u64 = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InitialWildernessGenerationError {
    NotInitialControlTick {
        control_tick: ControlTick,
    },
    NotInitialTick {
        tick: SimulationTick,
    },
    PlayerAlreadyInitialized {
        survival_initialized: bool,
        logistics_initialized: bool,
    },
    SurfaceAlreadyGenerated,
    FluidAlreadyGenerated,
    LogisticsAlreadyPopulated,
    SurfaceIdExhausted,
    SurfaceRevisionExhausted,
    FluidStoreIdExhausted,
    FluidRevisionExhausted,
    LogisticsRevisionExhausted,
    NoTraversableTerrainLayout,
    InvalidGeneratedResource(SurfaceResourceValidationError),
    InvalidGeneratedWater,
}

impl Display for InitialWildernessGenerationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotInitialControlTick { control_tick } => write!(
                formatter,
                "initial wilderness generation requires control tick 0, current control tick is {}",
                control_tick.value()
            ),
            Self::NotInitialTick { tick } => write!(
                formatter,
                "initial wilderness generation requires tick 0, current tick is {}",
                tick.value()
            ),
            Self::PlayerAlreadyInitialized {
                survival_initialized,
                logistics_initialized,
            } => write!(
                formatter,
                "initial wilderness generation requires an unadmitted player: survival initialized={survival_initialized}, logistics initialized={logistics_initialized}"
            ),
            Self::SurfaceAlreadyGenerated => formatter.write_str(
                "initial wilderness surface generation requires pristine surface-resource state",
            ),
            Self::FluidAlreadyGenerated => formatter
                .write_str("initial wilderness water generation requires pristine fluid state"),
            Self::LogisticsAlreadyPopulated => formatter
                .write_str("initial wilderness generation requires pristine logistics state"),
            Self::SurfaceIdExhausted => {
                formatter.write_str("surface resource identifier space is exhausted")
            }
            Self::SurfaceRevisionExhausted => {
                formatter.write_str("surface resource revision space is exhausted")
            }
            Self::FluidStoreIdExhausted => {
                formatter.write_str("fluid store identifier space is exhausted")
            }
            Self::FluidRevisionExhausted => {
                formatter.write_str("fluid revision space is exhausted")
            }
            Self::LogisticsRevisionExhausted => {
                formatter.write_str("logistics revision space is exhausted")
            }
            Self::NoTraversableTerrainLayout => formatter.write_str(
                "generated terrain has no standing-clearance layout for the bounded wilderness opening",
            ),
            Self::InvalidGeneratedResource(error) => {
                write!(
                    formatter,
                    "built-in wilderness surface resource is invalid: {error}"
                )
            }
            Self::InvalidGeneratedWater => {
                formatter.write_str("built-in wilderness water source is invalid")
            }
        }
    }
}

impl Error for InitialWildernessGenerationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidGeneratedResource(error) => Some(error),
            Self::NotInitialControlTick { .. }
            | Self::NotInitialTick { .. }
            | Self::PlayerAlreadyInitialized { .. }
            | Self::SurfaceAlreadyGenerated
            | Self::FluidAlreadyGenerated
            | Self::LogisticsAlreadyPopulated
            | Self::SurfaceIdExhausted
            | Self::SurfaceRevisionExhausted
            | Self::FluidStoreIdExhausted
            | Self::FluidRevisionExhausted
            | Self::LogisticsRevisionExhausted
            | Self::NoTraversableTerrainLayout
            | Self::InvalidGeneratedWater => None,
        }
    }
}

fn terrain_opening_position_is_standable(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
) -> bool {
    let Some(floor_y) = position.y().checked_sub(1) else {
        return false;
    };
    let Some(head_y) = position.y().checked_add(1) else {
        return false;
    };
    let floor = VoxelCoord::new(position.x(), floor_y, position.z());
    let head = VoxelCoord::new(position.x(), head_y, position.z());
    let Some(floor_state) = state.world().block_at(floor) else {
        return false;
    };
    registries
        .world()
        .block_for_state(floor_state)
        .is_some_and(|block| block.collision() == BlockCollisionShape::FullCube)
        && state.world().block_at(position) == Some(BlockStateId::AIR)
        && state.world().block_at(head) == Some(BlockStateId::AIR)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitialWildernessGenerationCommitError {
    ControlTickChanged {
        expected: ControlTick,
        actual: ControlTick,
    },
    TickChanged {
        expected: SimulationTick,
        actual: SimulationTick,
    },
    PlayerInitialized {
        survival_initialized: bool,
        logistics_initialized: bool,
    },
    StaleSurfaceRevision {
        expected: u64,
        actual: u64,
    },
    StaleSurfaceIdentity {
        expected: u32,
        actual: u32,
    },
    StaleFluidRevision {
        expected: u64,
        actual: u64,
    },
    StaleFluidIdentity {
        expected: u64,
        actual: u64,
    },
    StaleLogisticsRevision {
        expected: u64,
        actual: u64,
    },
    WaterAlreadyLocated {
        store: FluidStoreId,
    },
}

impl Display for InitialWildernessGenerationCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ControlTickChanged { expected, actual } => write!(
                formatter,
                "initial wilderness generation expected control tick {} but state advanced to {} before commit",
                expected.value(),
                actual.value()
            ),
            Self::TickChanged { expected, actual } => write!(
                formatter,
                "initial wilderness generation expected tick {} but state advanced to {} before commit",
                expected.value(),
                actual.value()
            ),
            Self::PlayerInitialized {
                survival_initialized,
                logistics_initialized,
            } => write!(
                formatter,
                "player admission changed before wilderness commit: survival initialized={survival_initialized}, logistics initialized={logistics_initialized}"
            ),
            Self::StaleSurfaceRevision { expected, actual } => write!(
                formatter,
                "surface revision changed from {expected} to {actual} before wilderness commit"
            ),
            Self::StaleSurfaceIdentity { expected, actual } => write!(
                formatter,
                "next surface resource id changed from {expected} to {actual} before wilderness commit"
            ),
            Self::StaleFluidRevision { expected, actual } => write!(
                formatter,
                "fluid revision changed from {expected} to {actual} before wilderness commit"
            ),
            Self::StaleFluidIdentity { expected, actual } => write!(
                formatter,
                "next fluid store id changed from {expected} to {actual} before wilderness commit"
            ),
            Self::StaleLogisticsRevision { expected, actual } => write!(
                formatter,
                "logistics revision changed from {expected} to {actual} before wilderness commit"
            ),
            Self::WaterAlreadyLocated { store } => write!(
                formatter,
                "generated water store {} acquired a logistics location before wilderness commit",
                store.value()
            ),
        }
    }
}

impl Error for InitialWildernessGenerationCommitError {}

#[must_use]
pub struct ValidatedInitialWildernessGeneration {
    expected_control_tick: ControlTick,
    expected_tick: SimulationTick,
    surface_generation: ValidatedSurfaceResourceCreation,
    water_generation: ValidatedFluidGeneration,
    water_position: VoxelCoord,
    traversal_edges: Vec<TraversalEdge>,
    expected_logistics_revision: u64,
    next_logistics_revision: u64,
}

impl ValidatedInitialWildernessGeneration {
    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<(), InitialWildernessGenerationCommitError> {
        let actual_tick = state.tick();
        if actual_tick != self.expected_tick {
            return Err(InitialWildernessGenerationCommitError::TickChanged {
                expected: self.expected_tick,
                actual: actual_tick,
            });
        }
        let actual_control_tick = state.control_tick();
        if actual_control_tick != self.expected_control_tick {
            return Err(InitialWildernessGenerationCommitError::ControlTickChanged {
                expected: self.expected_control_tick,
                actual: actual_control_tick,
            });
        }
        if player_admission_state(state) != PlayerAdmissionState::Absent {
            let (survival_initialized, logistics_initialized) = player_initialization_flags(state);
            return Err(InitialWildernessGenerationCommitError::PlayerInitialized {
                survival_initialized,
                logistics_initialized,
            });
        }
        self.surface_generation
            .precheck(state)
            .map_err(|error| match error {
                SurfaceResourceCreationCommitError::Tick { expected, actual } => {
                    InitialWildernessGenerationCommitError::TickChanged { expected, actual }
                }
                SurfaceResourceCreationCommitError::Revision { expected, actual } => {
                    InitialWildernessGenerationCommitError::StaleSurfaceRevision {
                        expected,
                        actual,
                    }
                }
                SurfaceResourceCreationCommitError::NextResourceId { expected, actual } => {
                    InitialWildernessGenerationCommitError::StaleSurfaceIdentity {
                        expected,
                        actual,
                    }
                }
            })?;
        self.water_generation
            .precheck(state)
            .map_err(|error| match error {
                FluidGenerationCommitError::Tick { expected, actual } => {
                    InitialWildernessGenerationCommitError::TickChanged { expected, actual }
                }
                FluidGenerationCommitError::Revision { expected, actual } => {
                    InitialWildernessGenerationCommitError::StaleFluidRevision { expected, actual }
                }
                FluidGenerationCommitError::NextStoreId { expected, actual } => {
                    InitialWildernessGenerationCommitError::StaleFluidIdentity { expected, actual }
                }
            })?;
        let actual_logistics_revision = state.logistics().revision();
        if actual_logistics_revision != self.expected_logistics_revision {
            return Err(
                InitialWildernessGenerationCommitError::StaleLogisticsRevision {
                    expected: self.expected_logistics_revision,
                    actual: actual_logistics_revision,
                },
            );
        }
        let water_store = self.water_generation.store();
        if state
            .logistics()
            .fluid_store_position(water_store)
            .is_some()
        {
            return Err(
                InitialWildernessGenerationCommitError::WaterAlreadyLocated { store: water_store },
            );
        }

        self.surface_generation.apply(state);
        self.water_generation.apply(state);
        state.logistics_state_mut().apply_generated_opening_access(
            self.expected_logistics_revision,
            self.next_logistics_revision,
            water_store,
            self.water_position,
            self.traversal_edges,
        );
        Ok(())
    }
}

fn generated_spec(
    position: VoxelCoord,
    commodity: CommodityKey,
    mass: Mass,
) -> Result<SurfaceResourceSpec, SurfaceResourceSpecError> {
    SurfaceResourceSpec::new(
        SurfaceResourceKind::Opportunity,
        position,
        commodity,
        mass,
        WILDERNESS_TEMPERATURE,
        MaterialComposition::pure(commodity.material()),
    )
}

fn player_initialization_flags(state: &AppState) -> (bool, bool) {
    (
        state.survival().player().is_some(),
        state.logistics().player().is_some(),
    )
}

/// Validates one deterministic opening opportunity for a fresh built-in world.
///
/// This establishes one short connected horizontal opening graph. Forage is one step from spawn;
/// from there the actor may branch to finite non-refilling drinkable water or continue to a work
/// junction whose two adjacent leaves hold loose stone/timber in seed-selected order. Persisted
/// traversal edges authorize only that graph. This does not generate terrain geometry, pathfinding,
/// routes beyond the generated graph, flow,
/// recharge, groundwater, shelter, fire, or ecology. The caller supplies the explicit seed and
/// spawn voxel; generated owner records become ordinary persistent world state.
pub fn validate_generate_initial_wilderness(
    registries: &Registries,
    state: &AppState,
    seed: WorldGenerationSeed,
    spawn: VoxelCoord,
) -> Result<ValidatedInitialWildernessGeneration, InitialWildernessGenerationError> {
    if state.tick() != SimulationTick::ZERO {
        return Err(InitialWildernessGenerationError::NotInitialTick { tick: state.tick() });
    }
    if state.control_tick() != ControlTick::ZERO {
        return Err(InitialWildernessGenerationError::NotInitialControlTick {
            control_tick: state.control_tick(),
        });
    }
    if player_admission_state(state) != PlayerAdmissionState::Absent {
        let (survival_initialized, logistics_initialized) = player_initialization_flags(state);
        return Err(InitialWildernessGenerationError::PlayerAlreadyInitialized {
            survival_initialized,
            logistics_initialized,
        });
    }
    if !state.surface().is_pristine_generation_state() {
        return Err(InitialWildernessGenerationError::SurfaceAlreadyGenerated);
    }
    if !state.fluid().is_pristine_generation_state() {
        return Err(InitialWildernessGenerationError::FluidAlreadyGenerated);
    }
    if !state.logistics().is_pristine_generation_state() {
        return Err(InitialWildernessGenerationError::LogisticsAlreadyPopulated);
    }
    let layout = if state.world().generation_profile().is_some() {
        generated_opening_layout_matching(seed, spawn, |position| {
            terrain_opening_position_is_standable(registries, state, position)
        })
        .ok_or(InitialWildernessGenerationError::NoTraversableTerrainLayout)?
    } else {
        generated_opening_layout(seed, spawn)
    };
    let stone_first = local_entropy(seed, spawn, OPENING_RESOURCE_ORDER_SALT).is_multiple_of(2);
    let stone_position = if stone_first {
        layout.material_forward
    } else {
        layout.material_branch
    };
    let timber_position = if stone_first {
        layout.material_branch
    } else {
        layout.material_forward
    };
    let specs = [
        generated_spec(
            stone_position,
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            generated_mass(
                seed,
                spawn,
                0x5354_4F4E_455F_5350,
                STONE_BASE_MASS_MG,
                STONE_VARIATION_MG,
            ),
        ),
        generated_spec(
            timber_position,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            generated_mass(
                seed,
                spawn,
                0x5449_4D42_4552_5350,
                TIMBER_BASE_MASS_MG,
                TIMBER_VARIATION_MG,
            ),
        ),
        generated_spec(
            layout.forage,
            CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
            generated_mass(
                seed,
                spawn,
                0x464F_5241_4745_5350,
                BERRIES_BASE_MASS_MG,
                BERRIES_VARIATION_MG,
            ),
        ),
    ];
    let specs = specs
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_else(|error| {
            panic!("built-in wilderness generation specification is invalid: {error}")
        });
    let surface_generation = validate_create_surface_resources(registries, state, specs).map_err(
        |error| match error {
            SurfaceResourceCreationError::Empty => {
                unreachable!("built-in wilderness generation always emits three resources")
            }
            SurfaceResourceCreationError::IdExhausted => {
                InitialWildernessGenerationError::SurfaceIdExhausted
            }
            SurfaceResourceCreationError::RevisionExhausted => {
                InitialWildernessGenerationError::SurfaceRevisionExhausted
            }
            SurfaceResourceCreationError::InvalidResource(error) => {
                InitialWildernessGenerationError::InvalidGeneratedResource(error)
            }
        },
    )?;
    let water_volume = generated_volume(
        seed,
        spawn,
        OPENING_WATER_VOLUME_SALT,
        OPENING_WATER_BASE_VOLUME_UL,
        OPENING_WATER_VARIATION_UL,
    );
    let water_generation = validate_generate_fluid_store(
        registries,
        state,
        GeneratedFluidStoreSpec::new(
            water_volume,
            FLUID_WATER,
            water_volume,
            WILDERNESS_TEMPERATURE,
        ),
    )
    .map_err(|error| match error {
        FluidGenerationError::IdExhausted => {
            InitialWildernessGenerationError::FluidStoreIdExhausted
        }
        FluidGenerationError::RevisionExhausted => {
            InitialWildernessGenerationError::FluidRevisionExhausted
        }
        FluidGenerationError::ZeroCapacity
        | FluidGenerationError::UnknownDefinition { .. }
        | FluidGenerationError::ZeroVolume
        | FluidGenerationError::ZeroTemperature
        | FluidGenerationError::VolumeExceedsCapacity { .. }
        | FluidGenerationError::BelowMeltingPoint { .. } => {
            InitialWildernessGenerationError::InvalidGeneratedWater
        }
    })?;
    let water_position = layout.water;
    let mut traversal_edges = layout
        .edge_pairs()
        .into_iter()
        .map(|(from, to)| {
            TraversalEdge::new(from, to).unwrap_or_else(|| {
                unreachable!("generated opening graph positions are face-neighbors")
            })
        })
        .collect::<Vec<_>>();
    traversal_edges.sort_unstable();
    if !state.can_spend_logistics_revisions(1) {
        return Err(InitialWildernessGenerationError::LogisticsRevisionExhausted);
    }
    let expected_logistics_revision = state.logistics().revision();
    let next_logistics_revision = expected_logistics_revision
        .checked_add(1)
        .unwrap_or_else(|| {
            unreachable!("logistics headroom check includes generated water placement")
        });
    Ok(ValidatedInitialWildernessGeneration {
        expected_control_tick: state.control_tick(),
        expected_tick: state.tick(),
        surface_generation,
        water_generation,
        water_position,
        traversal_edges,
        expected_logistics_revision,
        next_logistics_revision,
    })
}

#[cfg(test)]
#[path = "world_generation_tests.rs"]
mod tests;
