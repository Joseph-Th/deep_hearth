//! Atomic standing-tree felling into finite loose wood custody.

use crate::content::{FORM_LOG, MATERIAL_WOOD};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::AppState;
use crate::material::{CommodityKey, MaterialComposition};
use crate::player::{PlayerActionGuard, PlayerActionValidationError, validate_player_action};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::{
    SurfaceResourceCreationCommitError, SurfaceResourceCreationError, SurfaceResourceId,
    SurfaceResourceKind, SurfaceResourceSpec, SurfaceResourceValidationError,
    ValidatedSurfaceResourceCreation, validate_create_surface_resources,
};

/// Felling requires a living player next to a known standing trunk, not remote world access.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeFellingError {
    PlayerNotAdmitted,
    PlayerDead,
    PlayerBusy,
    TreeUnavailable {
        root: VoxelCoord,
    },
    TooFar {
        root: VoxelCoord,
        player: VoxelCoord,
    },
    WorldRevisionExhausted,
    AtmosphericInputOverflow,
    SurfaceIdExhausted,
    SurfaceRevisionExhausted,
    SurfacePlacement(SurfaceResourceValidationError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeFellingCommitError {
    StaleWorldRevision { expected: u64, actual: u64 },
    StaleLogisticsRevision { expected: u64, actual: u64 },
    StaleSurvivalRevision { expected: u64, actual: u64 },
    PlayerBecameBusy,
    SurfaceTickChanged,
    SurfaceRevisionChanged,
    StructureRevisionChanged,
    SurfaceResourceIdChanged,
}

#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreeFellingOutcome {
    root: VoxelCoord,
    wood: Mass,
    drop: SurfaceResourceId,
}

impl TreeFellingOutcome {
    #[must_use]
    pub const fn root(self) -> VoxelCoord {
        self.root
    }
    #[must_use]
    pub const fn wood(self) -> Mass {
        self.wood
    }
    #[must_use]
    pub const fn drop(self) -> SurfaceResourceId {
        self.drop
    }
}

#[must_use]
pub struct ValidatedTreeFelling {
    expected_world_revision: u64,
    next_world_revision: u64,
    expected_logistics_revision: u64,
    player_action: PlayerActionGuard,
    surface_creation: ValidatedSurfaceResourceCreation,
    outcome: TreeFellingOutcome,
}

impl ValidatedTreeFelling {
    /// Precheck all owners before the first mutation. No partial felling or matter duplication.
    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<TreeFellingOutcome, TreeFellingCommitError> {
        let actual = state.world().revision();
        if actual != self.expected_world_revision {
            return Err(TreeFellingCommitError::StaleWorldRevision {
                expected: self.expected_world_revision,
                actual,
            });
        }
        let actual = state.logistics().revision();
        if actual != self.expected_logistics_revision {
            return Err(TreeFellingCommitError::StaleLogisticsRevision {
                expected: self.expected_logistics_revision,
                actual,
            });
        }
        self.player_action.precheck(state).map_err(|stale| {
            TreeFellingCommitError::StaleSurvivalRevision {
                expected: stale.expected,
                actual: stale.actual,
            }
        })?;
        if state.player_work().active().is_some() {
            return Err(TreeFellingCommitError::PlayerBecameBusy);
        }
        self.surface_creation
            .precheck(state)
            .map_err(|error| match error {
                SurfaceResourceCreationCommitError::Tick { .. } => {
                    TreeFellingCommitError::SurfaceTickChanged
                }
                SurfaceResourceCreationCommitError::Revision { .. } => {
                    TreeFellingCommitError::SurfaceRevisionChanged
                }
                SurfaceResourceCreationCommitError::WorldRevision { expected, actual } => {
                    TreeFellingCommitError::StaleWorldRevision { expected, actual }
                }
                SurfaceResourceCreationCommitError::StructureRevision { .. } => {
                    TreeFellingCommitError::StructureRevisionChanged
                }
                SurfaceResourceCreationCommitError::NextResourceId { .. } => {
                    TreeFellingCommitError::SurfaceResourceIdChanged
                }
            })?;
        let current_tick = state.tick();
        state.world_state_mut().remove_felled_tree(
            self.outcome.root,
            self.expected_world_revision,
            self.next_world_revision,
            current_tick,
        );
        self.surface_creation.apply(state);
        Ok(self.outcome)
    }
}

/// Turn embodied standing timber into one recoverable, physically conserved log pile.
///
/// This is the felling/custody boundary. Cutting duration and tools are not yet
/// modeled, so it is deliberately not wired to an instant primary-control swing.
pub fn validate_fell_tree(
    registries: &Registries,
    state: &AppState,
    root: VoxelCoord,
) -> Result<ValidatedTreeFelling, TreeFellingError> {
    let player_action = validate_player_action(state).map_err(|error| match error {
        PlayerActionValidationError::PlayerNotAdmitted => TreeFellingError::PlayerNotAdmitted,
        PlayerActionValidationError::PlayerDead => TreeFellingError::PlayerDead,
    })?;
    let player = state
        .logistics()
        .player()
        .expect("validated player action requires a logistics player");
    if state.player_work().active().is_some() {
        return Err(TreeFellingError::PlayerBusy);
    }
    let tree = state
        .world()
        .tree_at(root)
        .ok_or(TreeFellingError::TreeUnavailable { root })?;
    let at = player.position();
    if !(at.y() == root.y()
        && at
            .x()
            .abs_diff(root.x())
            .checked_add(at.z().abs_diff(root.z()))
            == Some(1))
    {
        return Err(TreeFellingError::TooFar { root, player: at });
    }
    if state.world().assimilated_wood_at(state.tick()).is_none() {
        return Err(TreeFellingError::AtmosphericInputOverflow);
    }
    let expected_world_revision = state.world().revision();
    let next_world_revision = expected_world_revision
        .checked_add(1)
        .ok_or(TreeFellingError::WorldRevisionExhausted)?;
    let wood = tree.wood_at(state.tick());
    let spec = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        root,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        wood,
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_WOOD),
    )
    .expect("a positive valid tree always yields a valid wood parcel");
    let surface_creation = validate_create_surface_resources(registries, state, [spec]).map_err(
        |error| match error {
            SurfaceResourceCreationError::Empty => {
                unreachable!("one tree always creates one wood parcel")
            }
            SurfaceResourceCreationError::IdExhausted => TreeFellingError::SurfaceIdExhausted,
            SurfaceResourceCreationError::RevisionExhausted => {
                TreeFellingError::SurfaceRevisionExhausted
            }
            SurfaceResourceCreationError::InvalidResource(error) => {
                TreeFellingError::SurfacePlacement(error)
            }
        },
    )?;
    Ok(ValidatedTreeFelling {
        expected_world_revision,
        next_world_revision,
        expected_logistics_revision: state.logistics().revision(),
        player_action,
        outcome: TreeFellingOutcome {
            root,
            wood,
            drop: surface_creation.first_resource_id(),
        },
        surface_creation,
    })
}
