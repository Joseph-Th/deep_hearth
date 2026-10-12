//! Stateless deterministic creation of the currently implemented initial wilderness opportunity.
//!
//! Generated loose matter and finite opening water immediately enter their canonical persistent
//! owners. This module owns only fresh-world authorization and deterministic authored generation
//! inputs; it is not a terrain, chunk, ecology, hydrology, or movement owner.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::content::{
    FLUID_WATER, FORM_FOOD, FORM_LOG, FORM_LUMP, MATERIAL_BERRIES, MATERIAL_STONE, MATERIAL_WOOD,
};
use crate::core::quantity::{Mass, Temperature, Volume};
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
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
    GeneratedSurfaceResourceSpec, GeneratedSurfaceResourceSpecError, SurfaceGenerationCommitError,
    SurfaceGenerationError, SurfaceResourceValidationError, ValidatedSurfaceGeneration,
    validate_generate_surface_resources,
};

const WILDERNESS_TEMPERATURE: Temperature = Temperature::from_millikelvin(293_150);
const STONE_BASE_MASS_MG: u64 = 3_000_000;
const STONE_VARIATION_MG: u64 = 1_500_000;
const TIMBER_BASE_MASS_MG: u64 = 8_000_000;
const TIMBER_VARIATION_MG: u64 = 4_000_000;
const BERRIES_BASE_MASS_MG: u64 = 500_000;
const BERRIES_VARIATION_MG: u64 = 300_000;
const OPENING_WATER_VOLUME_UL: u64 = 1_500_000;
const OPENING_WATER_DIRECTION_SALT: u64 = 0x5741_5445_525F_4144;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WorldGenerationSeed(u64);

impl WorldGenerationSeed {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InitialWildernessGenerationError {
    NotInitialTick {
        tick: SimulationTick,
    },
    PlayerAlreadyInitialized {
        survival_initialized: bool,
        logistics_initialized: bool,
    },
    SurfaceAlreadyGenerated,
    FluidAlreadyGenerated,
    TraversalAccessAlreadyGenerated,
    SurfaceIdExhausted,
    SurfaceRevisionExhausted,
    FluidStoreIdExhausted,
    FluidRevisionExhausted,
    LogisticsRevisionExhausted,
    InvalidGeneratedResource(SurfaceResourceValidationError),
    InvalidGeneratedWater,
}

impl Display for InitialWildernessGenerationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
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
            Self::TraversalAccessAlreadyGenerated => formatter.write_str(
                "initial wilderness generation requires pristine traversal-access state",
            ),
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
            Self::NotInitialTick { .. }
            | Self::PlayerAlreadyInitialized { .. }
            | Self::SurfaceAlreadyGenerated
            | Self::FluidAlreadyGenerated
            | Self::TraversalAccessAlreadyGenerated
            | Self::SurfaceIdExhausted
            | Self::SurfaceRevisionExhausted
            | Self::FluidStoreIdExhausted
            | Self::FluidRevisionExhausted
            | Self::LogisticsRevisionExhausted
            | Self::InvalidGeneratedWater => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitialWildernessGenerationCommitError {
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
        write!(
            formatter,
            "initial wilderness generation became stale: {self:?}"
        )
    }
}

impl Error for InitialWildernessGenerationCommitError {}

#[must_use]
pub struct ValidatedInitialWildernessGeneration {
    expected_tick: SimulationTick,
    surface_generation: ValidatedSurfaceGeneration,
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
                SurfaceGenerationCommitError::StaleTick { expected, actual } => {
                    InitialWildernessGenerationCommitError::TickChanged { expected, actual }
                }
                SurfaceGenerationCommitError::StaleRevision { expected, actual } => {
                    InitialWildernessGenerationCommitError::StaleSurfaceRevision {
                        expected,
                        actual,
                    }
                }
                SurfaceGenerationCommitError::StaleNextResourceId { expected, actual } => {
                    InitialWildernessGenerationCommitError::StaleSurfaceIdentity {
                        expected,
                        actual,
                    }
                }
            })?;
        self.water_generation
            .precheck(state)
            .map_err(|error| match error {
                FluidGenerationCommitError::StaleTick { expected, actual } => {
                    InitialWildernessGenerationCommitError::TickChanged { expected, actual }
                }
                FluidGenerationCommitError::StaleRevision { expected, actual } => {
                    InitialWildernessGenerationCommitError::StaleFluidRevision { expected, actual }
                }
                FluidGenerationCommitError::StaleNextStoreId { expected, actual } => {
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

fn mix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

fn generated_adjacent_water_position(seed: WorldGenerationSeed, spawn: VoxelCoord) -> VoxelCoord {
    let first = usize::try_from(local_entropy(seed, spawn, OPENING_WATER_DIRECTION_SALT) % 4)
        .unwrap_or_else(|_| unreachable!("bounded water direction fits usize"));
    for offset in 0..4 {
        let candidate = match (first + offset) % 4 {
            0 => spawn
                .x()
                .checked_add(1)
                .map(|x| VoxelCoord::new(x, spawn.y(), spawn.z())),
            1 => spawn
                .z()
                .checked_add(1)
                .map(|z| VoxelCoord::new(spawn.x(), spawn.y(), z)),
            2 => spawn
                .x()
                .checked_sub(1)
                .map(|x| VoxelCoord::new(x, spawn.y(), spawn.z())),
            3 => spawn
                .z()
                .checked_sub(1)
                .map(|z| VoxelCoord::new(spawn.x(), spawn.y(), z)),
            _ => unreachable!("water direction is reduced modulo four"),
        };
        if let Some(candidate) = candidate {
            return candidate;
        }
    }
    unreachable!("every finite voxel has at least one representable horizontal neighbor")
}

fn local_entropy(seed: WorldGenerationSeed, spawn: VoxelCoord, salt: u64) -> u64 {
    mix64(
        seed.value()
            ^ (spawn.x() as u64).wrapping_mul(0xD1B5_4A32_D192_ED03)
            ^ (spawn.y() as u64).wrapping_mul(0x94D0_49BB_1331_11EB)
            ^ (spawn.z() as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9)
            ^ salt,
    )
}

fn generated_mass(
    seed: WorldGenerationSeed,
    spawn: VoxelCoord,
    salt: u64,
    base_mg: u64,
    variation_mg: u64,
) -> Mass {
    let width = variation_mg
        .checked_add(1)
        .unwrap_or_else(|| unreachable!("bounded wilderness variation width fits u64"));
    Mass::from_milligrams(base_mg + local_entropy(seed, spawn, salt) % width)
}

fn generated_spec(
    position: VoxelCoord,
    commodity: CommodityKey,
    mass: Mass,
) -> Result<GeneratedSurfaceResourceSpec, GeneratedSurfaceResourceSpecError> {
    GeneratedSurfaceResourceSpec::new(
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
/// This establishes finite loose stone, fallen timber, and forage at spawn plus one finite,
/// non-refilling drinkable water source at a horizontally adjacent voxel and one persisted
/// traversal edge authorizing movement between them. It does not generate terrain geometry,
/// pathfinding, routes beyond that edge, flow, recharge, groundwater, shelter, fire, or ecology.
/// The caller supplies the explicit seed and spawn voxel; generated owner records become ordinary
/// persistent world state.
pub fn validate_generate_initial_wilderness(
    registries: &Registries,
    state: &AppState,
    seed: WorldGenerationSeed,
    spawn: VoxelCoord,
) -> Result<ValidatedInitialWildernessGeneration, InitialWildernessGenerationError> {
    if state.tick() != SimulationTick::ZERO {
        return Err(InitialWildernessGenerationError::NotInitialTick { tick: state.tick() });
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
    if state.logistics().traversable_edges().next().is_some() {
        return Err(InitialWildernessGenerationError::TraversalAccessAlreadyGenerated);
    }
    let specs = [
        generated_spec(
            spawn,
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
            spawn,
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
            spawn,
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
    let surface_generation = validate_generate_surface_resources(registries, state, specs)
        .map_err(|error| match error {
            SurfaceGenerationError::Empty => {
                unreachable!("built-in wilderness generation always emits three resources")
            }
            SurfaceGenerationError::IdExhausted => {
                InitialWildernessGenerationError::SurfaceIdExhausted
            }
            SurfaceGenerationError::RevisionExhausted => {
                InitialWildernessGenerationError::SurfaceRevisionExhausted
            }
            SurfaceGenerationError::InvalidResource(error) => {
                InitialWildernessGenerationError::InvalidGeneratedResource(error)
            }
        })?;
    let water_volume = Volume::from_microliters(OPENING_WATER_VOLUME_UL);
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
    let water_position = generated_adjacent_water_position(seed, spawn);
    let traversal_edge = TraversalEdge::new(spawn, water_position).unwrap_or_else(|| {
        unreachable!("generated opening water is always one horizontal face-neighbor from spawn")
    });
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
        expected_tick: state.tick(),
        surface_generation,
        water_generation,
        water_position,
        traversal_edges: vec![traversal_edge],
        expected_logistics_revision,
        next_logistics_revision,
    })
}

#[cfg(test)]
#[path = "world_generation_tests.rs"]
mod tests;
