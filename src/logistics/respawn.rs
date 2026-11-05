//! Respawn logistics transition from dead-player custody to a recoverable local death cache.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::equipment::EquipmentId;
use crate::inventory::allocation::{
    EmptyStockpileAllocationCommitError, EmptyStockpileAllocationError,
    ValidatedEmptyStockpileAllocation, validate_empty_stockpile_allocation,
};
use crate::inventory::{StockpileId, StockpileStorageProfile};
use crate::registry::Registries;
use crate::spatial::{HorizontalVelocity, VerticalVelocity, VoxelCoord};

use super::{
    PlayerBodyState, PlayerLogisticsRecord, first_player_body_collision,
    first_player_structure_collision, first_player_temple_caste_collision,
    player_body_fits_continuous_world, player_body_is_grounded_at,
};

/// A destroyed spawn floor or an airborne corpse should not force a revived
/// player back into the same fall when supported open ground is immediately
/// nearby. Search only a small, deterministic, generated-world neighborhood.
const SAFE_RESPAWN_OFFSETS: [(i64, i64); 8] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
];
const SAFE_RESPAWN_HEIGHT_OFFSETS: [i64; 3] = [0, -1, 1];

fn respawn_body_is_clear(registries: &Registries, state: &AppState, body: PlayerBodyState) -> bool {
    let locomotion = registries.survival().physiology().locomotion();
    player_body_fits_continuous_world(body, locomotion)
        && first_player_body_collision(registries.world(), state.world(), body, locomotion)
            .is_none()
        && first_player_temple_caste_collision(registries, state, body).is_none()
        && first_player_structure_collision(registries, state, body).is_none()
}

fn grounded_respawn_body_at(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
) -> Option<PlayerBodyState> {
    let body = PlayerBodyState::centered_on_voxel_floor(position)?;
    (respawn_body_is_clear(registries, state, body)
        && (!state.world().has_generated_terrain_at(position)
            || player_body_is_grounded_at(registries, state, body)))
    .then_some(body)
}

fn nearby_supported_respawn_body(
    registries: &Registries,
    state: &AppState,
    origin: VoxelCoord,
) -> Option<PlayerBodyState> {
    // A new block or floor may have raised the safe standing height without
    // changing the original spawn column. Prefer its raised surface before
    // moving horizontally, but never climb down a mined-out hole while a
    // supported neighbor remains at the actor's original height.
    if let Some(y) = origin.y().checked_add(1) {
        let candidate = VoxelCoord::new(origin.x(), y, origin.z());
        if state.world().has_generated_terrain_at(candidate)
            && let Some(body) = grounded_respawn_body_at(registries, state, candidate)
        {
            return Some(body);
        }
    }
    for height in SAFE_RESPAWN_HEIGHT_OFFSETS {
        let Some(y) = origin.y().checked_add(height) else {
            continue;
        };
        for (dx, dz) in SAFE_RESPAWN_OFFSETS {
            let (Some(x), Some(z)) = (origin.x().checked_add(dx), origin.z().checked_add(dz))
            else {
                continue;
            };
            let candidate = VoxelCoord::new(x, y, z);
            if !state.world().has_generated_terrain_at(candidate) {
                continue;
            }
            if let Some(body) = grounded_respawn_body_at(registries, state, candidate) {
                return Some(body);
            }
        }
    }
    // If neither the raised anchor nor its neighbors provide safe support,
    // an intact lower floor in the same column is still preferable to a
    // respawn that immediately resumes the fall at the death location.
    if let Some(y) = origin.y().checked_sub(1) {
        let candidate = VoxelCoord::new(origin.x(), y, origin.z());
        if state.world().has_generated_terrain_at(candidate)
            && let Some(body) = grounded_respawn_body_at(registries, state, candidate)
        {
            return Some(body);
        }
    }
    None
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlayerLogisticsRespawnError {
    PlayerNotInitialized,
    ActivePlayerWork,
    PendingDirectAction,
    StockpileIdExhausted,
    InventoryRevisionExhausted,
    LogisticsRevisionExhausted,
}

impl Display for PlayerLogisticsRespawnError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerNotInitialized => {
                formatter.write_str("player logistics is not initialized")
            }
            Self::ActivePlayerWork => {
                formatter.write_str("player cannot respawn while work is active")
            }
            Self::PendingDirectAction => {
                formatter.write_str("player cannot respawn with buffered direct input")
            }
            Self::StockpileIdExhausted => {
                formatter.write_str("stockpile identifier space is exhausted")
            }
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted")
            }
            Self::LogisticsRevisionExhausted => {
                formatter.write_str("logistics revision space is exhausted")
            }
        }
    }
}

impl Error for PlayerLogisticsRespawnError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlayerLogisticsRespawnCommitError {
    SimulationTick {
        expected: SimulationTick,
        actual: SimulationTick,
    },
    LogisticsRevision {
        expected: u64,
        actual: u64,
    },
    InventoryRevision {
        expected: u64,
        actual: u64,
    },
    WorldRevision {
        expected: u64,
        actual: u64,
    },
    StructureRevision {
        expected: u64,
        actual: u64,
    },
}

impl Display for PlayerLogisticsRespawnCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SimulationTick { expected, actual } => write!(
                formatter,
                "respawn expected simulation tick {} but current tick is {}",
                expected.value(),
                actual.value()
            ),
            Self::LogisticsRevision { expected, actual } => write!(
                formatter,
                "respawn expected logistics revision {expected} but current revision is {actual}"
            ),
            Self::InventoryRevision { expected, actual } => write!(
                formatter,
                "respawn expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::WorldRevision { expected, actual } => write!(
                formatter,
                "respawn expected world revision {expected} but current revision is {actual}"
            ),
            Self::StructureRevision { expected, actual } => write!(
                formatter,
                "respawn expected structure revision {expected} but current revision is {actual}"
            ),
        }
    }
}

impl Error for PlayerLogisticsRespawnCommitError {}

#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ValidatedPlayerLogisticsRespawn {
    expected_tick: SimulationTick,
    expected_logistics_revision: u64,
    next_logistics_revision: u64,
    expected_world_revision: u64,
    expected_structure_revision: u64,
    expected_player: PlayerLogisticsRecord,
    respawn_body: PlayerBodyState,
    used_spawn_anchor: bool,
    death_position: VoxelCoord,
    death_cache: StockpileId,
    dropped_equipment: Vec<EquipmentId>,
    carried: ValidatedEmptyStockpileAllocation,
}

impl ValidatedPlayerLogisticsRespawn {
    pub(crate) fn precheck(
        &self,
        state: &AppState,
    ) -> Result<(), PlayerLogisticsRespawnCommitError> {
        let actual_tick = state.tick();
        if actual_tick != self.expected_tick {
            return Err(PlayerLogisticsRespawnCommitError::SimulationTick {
                expected: self.expected_tick,
                actual: actual_tick,
            });
        }
        let actual_logistics = state.logistics().revision();
        if actual_logistics != self.expected_logistics_revision {
            return Err(PlayerLogisticsRespawnCommitError::LogisticsRevision {
                expected: self.expected_logistics_revision,
                actual: actual_logistics,
            });
        }
        let actual_world = state.world().revision();
        if actual_world != self.expected_world_revision {
            return Err(PlayerLogisticsRespawnCommitError::WorldRevision {
                expected: self.expected_world_revision,
                actual: actual_world,
            });
        }
        let actual_structure = state.structures().revision();
        if actual_structure != self.expected_structure_revision {
            return Err(PlayerLogisticsRespawnCommitError::StructureRevision {
                expected: self.expected_structure_revision,
                actual: actual_structure,
            });
        }
        self.carried.precheck(state).map_err(|error| match error {
            EmptyStockpileAllocationCommitError::StaleInventoryRevision { expected, actual } => {
                PlayerLogisticsRespawnCommitError::InventoryRevision { expected, actual }
            }
        })
    }

    pub(crate) fn apply(self, state: &mut AppState) -> PlayerLogisticsRespawnOutcome {
        let carried_stockpile = self.carried.apply(state);
        state.logistics_state_mut().apply_player_respawn(
            self.expected_logistics_revision,
            self.next_logistics_revision,
            self.expected_player,
            self.respawn_body,
            super::state::PlayerRespawnCustody {
                fresh_carried_stockpile: carried_stockpile,
                death_position: self.death_position,
                dropped_equipment: &self.dropped_equipment,
            },
        );
        PlayerLogisticsRespawnOutcome {
            position: self.respawn_body.voxel_position(),
            used_spawn_anchor: self.used_spawn_anchor,
            death_position: self.death_position,
            death_cache: self.death_cache,
            carried_stockpile,
            dropped_equipment: self.dropped_equipment,
        }
    }
}

#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PlayerLogisticsRespawnOutcome {
    pub(crate) position: VoxelCoord,
    pub(crate) used_spawn_anchor: bool,
    pub(crate) death_position: VoxelCoord,
    pub(crate) death_cache: StockpileId,
    pub(crate) carried_stockpile: StockpileId,
    pub(crate) dropped_equipment: Vec<EquipmentId>,
}

pub(crate) fn validate_player_logistics_respawn(
    registries: &Registries,
    state: &AppState,
) -> Result<ValidatedPlayerLogisticsRespawn, PlayerLogisticsRespawnError> {
    let player = state
        .logistics()
        .player()
        .copied()
        .ok_or(PlayerLogisticsRespawnError::PlayerNotInitialized)?;
    if state.player_work().active().is_some() {
        return Err(PlayerLogisticsRespawnError::ActivePlayerWork);
    }
    if state.logistics().has_pending_direct_action() {
        return Err(PlayerLogisticsRespawnError::PendingDirectAction);
    }
    if !state.can_spend_inventory_revisions(1) {
        return Err(PlayerLogisticsRespawnError::InventoryRevisionExhausted);
    }
    let carried = validate_empty_stockpile_allocation(
        state.inventory(),
        registries.survival().physiology().carried_mass_capacity(),
        StockpileStorageProfile::unbounded_solid_only(),
    )
    .map_err(|error| match error {
        EmptyStockpileAllocationError::ZeroCapacity => {
            unreachable!("authored body carrying capacity is nonzero")
        }
        EmptyStockpileAllocationError::IdExhausted => {
            PlayerLogisticsRespawnError::StockpileIdExhausted
        }
        EmptyStockpileAllocationError::RevisionExhausted => {
            PlayerLogisticsRespawnError::InventoryRevisionExhausted
        }
    })?;
    let expected_logistics_revision = state.logistics().revision();
    let next_logistics_revision = expected_logistics_revision
        .checked_add(1)
        .filter(|_| state.can_spend_logistics_revisions(1))
        .ok_or(PlayerLogisticsRespawnError::LogisticsRevisionExhausted)?;
    let spawn_body = grounded_respawn_body_at(registries, state, player.respawn_position());
    let (respawn_body, used_spawn_anchor) = spawn_body.map_or_else(
        || {
            let body = player.body();
            // Preserve the exact corpse position when it is still a supported
            // recovery site. Generated-world airborne deaths or an excavated
            // death floor instead seek a nearby clear floor, first around the
            // original anchor and then near the death cache. No unknown world
            // space is treated as safe or materialized by this search.
            let fallback = body.with_motion(
                body.foot_position(),
                HorizontalVelocity::ZERO,
                VerticalVelocity::ZERO,
                body.stance(),
            );
            let needs_ground = state.world().has_generated_terrain_at(player.position());
            if !needs_ground || player_body_is_grounded_at(registries, state, fallback) {
                return (fallback, false);
            }
            (
                nearby_supported_respawn_body(registries, state, player.respawn_position())
                    .or_else(|| nearby_supported_respawn_body(registries, state, player.position()))
                    .unwrap_or(fallback),
                false,
            )
        },
        |body| (body, true),
    );
    Ok(ValidatedPlayerLogisticsRespawn {
        expected_tick: state.tick(),
        expected_logistics_revision,
        next_logistics_revision,
        expected_world_revision: state.world().revision(),
        expected_structure_revision: state.structures().revision(),
        expected_player: player,
        respawn_body,
        used_spawn_anchor,
        death_position: player.position(),
        death_cache: player.carried_stockpile(),
        dropped_equipment: state.logistics().carried_equipment().collect(),
        carried,
    })
}
