//! Local relocation of a living juvenile, without synthetic saplings or hidden biomass.
//!
//! The tree retains its generated seed origin and growth clock. Unlike a seed-sowing
//! mechanic this operation moves an entire existing, small sapling and all of its wood.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::player::{PlayerActionGuard, PlayerActionValidationError, validate_player_action};
use crate::registry::Registries;
use crate::spatial::{VoxelCoord, WorldPoint};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaplingTransplantError {
    PlayerNotAdmitted,
    PlayerDead,
    PlayerBusy,
    PlayerMoving,
    PlayerAirborne,
    PreparedStrikeActive,
    SourceUnavailable { root: VoxelCoord },
    TooMature { root: VoxelCoord },
    SourceRemote { root: VoxelCoord },
    DestinationRemote { root: VoxelCoord },
    DestinationBlocked { root: VoxelCoord },
    WorldRevisionExhausted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaplingTransplantCommitError {
    TickChanged,
    WorldChanged,
    LogisticsChanged,
    PlayerMoved,
    SurvivalChanged,
    PlayerWorkChanged,
    CombatChanged,
    SurfaceChanged,
    StructuresChanged,
    PhysicalConditionsChanged,
}

#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaplingTransplantOutcome {
    from: VoxelCoord,
    to: VoxelCoord,
    wood: Mass,
}

impl SaplingTransplantOutcome {
    #[must_use]
    pub const fn from(self) -> VoxelCoord {
        self.from
    }
    #[must_use]
    pub const fn to(self) -> VoxelCoord {
        self.to
    }
    #[must_use]
    pub const fn wood(self) -> Mass {
        self.wood
    }
}

#[must_use]
pub struct ValidatedSaplingTransplant {
    from: VoxelCoord,
    to: VoxelCoord,
    wood: Mass,
    expected_tick: SimulationTick,
    expected_world_revision: u64,
    next_world_revision: u64,
    expected_logistics_revision: u64,
    expected_player_work_revision: u64,
    expected_combat_revision: u64,
    expected_surface_revision: u64,
    expected_structure_revision: u64,
    expected_foot: WorldPoint,
    player_action: PlayerActionGuard,
}

impl ValidatedSaplingTransplant {
    /// Every actor, surface and obstruction fact is checked again before mutating the world.
    pub fn commit(
        self,
        registries: &Registries,
        state: &mut AppState,
    ) -> Result<SaplingTransplantOutcome, SaplingTransplantCommitError> {
        if state.tick() != self.expected_tick {
            return Err(SaplingTransplantCommitError::TickChanged);
        }
        if state.world().revision() != self.expected_world_revision {
            return Err(SaplingTransplantCommitError::WorldChanged);
        }
        if state.logistics().revision() != self.expected_logistics_revision {
            return Err(SaplingTransplantCommitError::LogisticsChanged);
        }
        if state.player_work().revision() != self.expected_player_work_revision {
            return Err(SaplingTransplantCommitError::PlayerWorkChanged);
        }
        if state.combat().revision() != self.expected_combat_revision {
            return Err(SaplingTransplantCommitError::CombatChanged);
        }
        if state.surface().revision() != self.expected_surface_revision {
            return Err(SaplingTransplantCommitError::SurfaceChanged);
        }
        if state.structures().revision() != self.expected_structure_revision {
            return Err(SaplingTransplantCommitError::StructuresChanged);
        }
        if state
            .logistics()
            .player()
            .map(|player| player.body().foot_position())
            != Some(self.expected_foot)
        {
            return Err(SaplingTransplantCommitError::PlayerMoved);
        }
        self.player_action
            .precheck(state)
            .map_err(|_| SaplingTransplantCommitError::SurvivalChanged)?;
        if validate_transplant_juvenile(registries, state, self.from, self.to).is_err() {
            return Err(SaplingTransplantCommitError::PhysicalConditionsChanged);
        }
        let state_tick = state.tick();
        state.world_state_mut().transplant_tree(
            self.from,
            self.to,
            self.expected_world_revision,
            self.next_world_revision,
            state_tick,
        );
        Ok(SaplingTransplantOutcome {
            from: self.from,
            to: self.to,
            wood: self.wood,
        })
    }
}

fn beside(player: VoxelCoord, target: VoxelCoord) -> bool {
    player.y() == target.y()
        && player
            .x()
            .abs_diff(target.x())
            .checked_add(player.z().abs_diff(target.z()))
            == Some(1)
}

fn destination_clear(registries: &Registries, state: &AppState, root: VoxelCoord) -> bool {
    if !state
        .world()
        .has_standing_clearance(registries.world(), root)
    {
        return false;
    }
    let Some(head_y) = root.y().checked_add(1) else {
        return false;
    };
    [root, VoxelCoord::new(root.x(), head_y, root.z())]
        .into_iter()
        .all(|voxel| {
            !state.structures().has_embodied_member_at(voxel)
                && !state.logistics().has_stationary_occupant_at(voxel)
                && state
                    .surface()
                    .available_resources_at(Some(voxel))
                    .next()
                    .is_none()
                && !crate::logistics::player_body_occupies_voxel(registries, state, voxel)
                && !crate::kobolds::opening_kobold_occupies_voxel(state, voxel)
                && !crate::kobolds::temple_caste_occupies_voxel(registries, state, voxel)
        })
}

/// Transplant an existing juvenile (less than 10 kg dry wood) across the player's
/// immediate standing position. No seed or sapling mass can appear ex nihilo.
///
/// This is a bounded capability; travel/carry and a normal planting gesture are not yet modeled.
pub fn validate_transplant_juvenile(
    registries: &Registries,
    state: &AppState,
    from: VoxelCoord,
    to: VoxelCoord,
) -> Result<ValidatedSaplingTransplant, SaplingTransplantError> {
    let player_action = validate_player_action(state).map_err(|error| match error {
        PlayerActionValidationError::PlayerNotAdmitted => SaplingTransplantError::PlayerNotAdmitted,
        PlayerActionValidationError::PlayerDead => SaplingTransplantError::PlayerDead,
    })?;
    if state.player_work().active().is_some() {
        return Err(SaplingTransplantError::PlayerBusy);
    }
    if state.combat().has_prepared_strike() {
        return Err(SaplingTransplantError::PreparedStrikeActive);
    }
    if !crate::logistics::player_body_is_grounded(registries, state) {
        return Err(SaplingTransplantError::PlayerAirborne);
    }
    let player = state
        .logistics()
        .player()
        .expect("admitted player must have a logistics body");
    if player.body().horizontal_velocity() != crate::spatial::HorizontalVelocity::ZERO
        || player.body().vertical_velocity().nanometers_per_second() != 0
    {
        return Err(SaplingTransplantError::PlayerMoving);
    }
    let tree = state
        .world()
        .tree_at(from)
        .ok_or(SaplingTransplantError::SourceUnavailable { root: from })?;
    let wood = tree.wood_at(state.tick());
    if wood.milligrams() >= 10_000_000 {
        return Err(SaplingTransplantError::TooMature { root: from });
    }
    if !beside(player.position(), from) {
        return Err(SaplingTransplantError::SourceRemote { root: from });
    }
    if !beside(player.position(), to) {
        return Err(SaplingTransplantError::DestinationRemote { root: to });
    }
    if !destination_clear(registries, state, to) {
        return Err(SaplingTransplantError::DestinationBlocked { root: to });
    }
    if !state.can_spend_world_revisions(1) {
        return Err(SaplingTransplantError::WorldRevisionExhausted);
    }
    let expected_world_revision = state.world().revision();
    Ok(ValidatedSaplingTransplant {
        from,
        to,
        wood,
        expected_tick: state.tick(),
        expected_world_revision,
        next_world_revision: expected_world_revision + 1,
        expected_logistics_revision: state.logistics().revision(),
        expected_player_work_revision: state.player_work().revision(),
        expected_combat_revision: state.combat().revision(),
        expected_surface_revision: state.surface().revision(),
        expected_structure_revision: state.structures().revision(),
        expected_foot: player.body().foot_position(),
        player_action,
    })
}
