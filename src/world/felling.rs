//! Atomic standing-tree felling into finite loose wood custody.

use crate::content::{FORM_LOG, MATERIAL_WOOD};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::AppState;
use crate::core::time::{SimulationTick, TickSpan};
use crate::labor::{
    PlayerWork, PlayerWorkStartError, TreeFellingWork, ValidatedPlayerWorkStart,
    validate_player_work_start_control_rate,
};
use crate::logistics::{PlayerActiveSelection, validate_player_equipment_access};
use crate::material::{CommodityKey, MaterialComposition};
use crate::player::{PlayerActionGuard, PlayerActionValidationError, validate_player_action};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::{
    SurfaceResourceCreationCommitError, SurfaceResourceCreationError, SurfaceResourceId,
    SurfaceResourceKind, SurfaceResourceSpec, SurfaceResourceValidationError,
    ValidatedSurfaceResourceCreation, validate_create_surface_resources_after_tree_felling_at,
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
    PlayerPositionChanged,
    StaleSurvivalRevision { expected: u64, actual: u64 },
    PlayerBecameBusy,
    SurfaceTickChanged,
    SurfaceRevisionChanged,
    StructureRevisionChanged,
    SurfaceResourceIdChanged,
    WorkChanged,
    StaleSurfaceRevision { expected: u64, actual: u64 },
    StaleSurfaceResourceId { expected: u32, actual: u32 },
    StaleEquipmentRevision { expected: u64, actual: u64 },
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
    expected_player_foot: crate::spatial::WorldPoint,
    player_action: PlayerActionGuard,
    surface_creation: ValidatedSurfaceResourceCreation,
    outcome: TreeFellingOutcome,
    expected_work: Option<TreeFellingWork>,
    at: SimulationTick,
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
        if state
            .logistics()
            .player()
            .map(|player| player.body().foot_position())
            != Some(self.expected_player_foot)
        {
            return Err(TreeFellingCommitError::PlayerPositionChanged);
        }
        self.player_action.precheck(state).map_err(|stale| {
            TreeFellingCommitError::StaleSurvivalRevision {
                expected: stale.expected,
                actual: stale.actual,
            }
        })?;
        if state.player_work().active()
            != self
                .expected_work
                .map(|work| PlayerWork::TreeFelling { work })
        {
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
        state.world_state_mut().remove_felled_tree(
            self.outcome.root,
            self.expected_world_revision,
            self.next_world_revision,
            self.at,
        );
        self.surface_creation.apply(state);
        Ok(self.outcome)
    }
}

/// A player can only fell standing woodland with a carried selected woodcutting adze.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeFellingStartError {
    Tree(TreeFellingError),
    NoActiveChoppingTool,
    UnsupportedChoppingTool,
    ChoppingToolUnavailable,
    ChoppingToolTooWorn,
    EquipmentRevisionExhausted,
    CompletionTickOverflow,
    Work(PlayerWorkStartError),
}

pub(crate) const fn tree_felling_exertion() -> crate::survival::SurvivalExertion {
    crate::survival::SurvivalExertion::new(
        crate::core::quantity::Energy::from_nanojoules(1_800_000_000_000),
        crate::core::quantity::Volume::from_microliters(350),
    )
}

pub(crate) fn tree_felling_duration(
    wood: Mass,
    definition: crate::equipment::EquipmentDefinitionId,
) -> TickSpan {
    let rate: u64 = if definition == crate::content::EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE {
        3_000_000
    } else {
        1_500_000
    };
    TickSpan::new(wood.milligrams().div_ceil(rate).max(8))
}

pub(crate) fn tree_felling_wear_ppm_per_tick(
    definition: crate::equipment::EquipmentDefinitionId,
) -> u32 {
    if definition == crate::content::EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE {
        55
    } else {
        120
    }
}

pub(crate) fn tree_felling_condition_after(
    trace: crate::equipment::EquipmentOperationTrace,
    duration: TickSpan,
) -> crate::maintenance::Condition {
    crate::maintenance::calculate_condition_after_active_ticks(
        tree_felling_wear_ppm_per_tick(trace.definition()),
        trace.condition(),
        duration,
    )
}

/// Tool wear is settled once per completed or fatally interrupted chopping interval.
#[must_use]
pub(crate) struct TreeFellingWearPlan {
    equipment: crate::equipment::EquipmentId,
    before: crate::maintenance::Condition,
    after: crate::maintenance::Condition,
}

impl TreeFellingWearPlan {
    pub(crate) const fn equipment_revision_steps(&self) -> u64 {
        1
    }
}

pub(crate) fn decide_tree_felling_wear(
    state: &AppState,
    next_tick: SimulationTick,
    player_dead_after_tick: bool,
) -> Option<TreeFellingWearPlan> {
    let work = state.player_work().active()?.tree_felling()?;
    if work.completes_at() != next_tick && !player_dead_after_tick {
        return None;
    }
    let elapsed = next_tick
        .checked_duration_since(work.started_at())
        .expect("active tree chopping cannot settle before starting");
    let before = work.equipment_trace().condition();
    let after = tree_felling_condition_after(work.equipment_trace(), elapsed);
    Some(TreeFellingWearPlan {
        equipment: work.equipment(),
        before,
        after,
    })
}

pub(crate) fn apply_tree_felling_wear(state: &mut AppState, plan: Option<TreeFellingWearPlan>) {
    let Some(plan) = plan else {
        return;
    };
    let next_revision = state
        .equipment()
        .revision()
        .checked_add(1)
        .expect("prebudgeted tree chopping equipment revision exhausted");
    state.equipment_state_mut().apply_condition_change(
        plan.equipment,
        plan.before,
        plan.after,
        next_revision,
    );
}

#[must_use]
pub struct ValidatedTreeFellingStart {
    expected_world_revision: u64,
    expected_logistics_revision: u64,
    expected_surface_revision: u64,
    expected_next_resource_id: u32,
    expected_equipment_revision: u64,
    work_start: ValidatedPlayerWorkStart,
    work: TreeFellingWork,
}

impl ValidatedTreeFellingStart {
    pub fn commit(self, state: &mut AppState) -> Result<TreeFellingWork, TreeFellingCommitError> {
        if state.world().revision() != self.expected_world_revision {
            return Err(TreeFellingCommitError::StaleWorldRevision {
                expected: self.expected_world_revision,
                actual: state.world().revision(),
            });
        }
        if state.logistics().revision() != self.expected_logistics_revision {
            return Err(TreeFellingCommitError::StaleLogisticsRevision {
                expected: self.expected_logistics_revision,
                actual: state.logistics().revision(),
            });
        }
        if state.surface().revision() != self.expected_surface_revision {
            return Err(TreeFellingCommitError::StaleSurfaceRevision {
                expected: self.expected_surface_revision,
                actual: state.surface().revision(),
            });
        }
        if state.surface().next_resource_id() != self.expected_next_resource_id {
            return Err(TreeFellingCommitError::StaleSurfaceResourceId {
                expected: self.expected_next_resource_id,
                actual: state.surface().next_resource_id(),
            });
        }
        if state.equipment().revision() != self.expected_equipment_revision {
            return Err(TreeFellingCommitError::StaleEquipmentRevision {
                expected: self.expected_equipment_revision,
                actual: state.equipment().revision(),
            });
        }
        self.work_start
            .precheck(state)
            .map_err(|_| TreeFellingCommitError::WorkChanged)?;
        self.work_start.apply(state);
        Ok(self.work)
    }
}

/// Start exclusive, exertive chopping on any control frame. Wood stays in the living
/// tree until the scheduled slow-tick completion transfers it to the surface.
pub fn validate_start_player_tree_felling(
    registries: &Registries,
    state: &AppState,
    root: VoxelCoord,
) -> Result<ValidatedTreeFellingStart, TreeFellingStartError> {
    let _ = validate_fell_tree(registries, state, root).map_err(TreeFellingStartError::Tree)?;
    if !state.can_spend_equipment_revisions(1) {
        return Err(TreeFellingStartError::EquipmentRevisionExhausted);
    }
    if !state.can_spend_world_revisions(1) {
        return Err(TreeFellingStartError::Tree(
            TreeFellingError::WorldRevisionExhausted,
        ));
    }
    if !state.can_spend_surface_revisions(1) {
        return Err(TreeFellingStartError::Tree(
            TreeFellingError::SurfaceRevisionExhausted,
        ));
    }
    if !state.has_surface_resource_id_headroom(1) {
        return Err(TreeFellingStartError::Tree(
            TreeFellingError::SurfaceIdExhausted,
        ));
    }
    let equipment = match state.logistics().active_selection() {
        Some(PlayerActiveSelection::Equipment(equipment)) => equipment,
        _ => return Err(TreeFellingStartError::NoActiveChoppingTool),
    };
    validate_player_equipment_access(state, equipment)
        .map_err(|_| TreeFellingStartError::ChoppingToolUnavailable)?;
    if crate::equipment::equipment_occupancy(state, equipment).is_some() {
        return Err(TreeFellingStartError::ChoppingToolUnavailable);
    }
    let record = state
        .equipment()
        .get_equipment(equipment)
        .ok_or(TreeFellingStartError::ChoppingToolUnavailable)?;
    let definition = record.definition();
    if definition != crate::content::EQUIPMENT_STONE_WOODWORKING_ADZE
        && definition != crate::content::EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE
    {
        return Err(TreeFellingStartError::UnsupportedChoppingTool);
    }
    let wood = state
        .world()
        .tree_at(root)
        .expect("validated standing tree remains present")
        .wood_at(state.tick());
    let duration = tree_felling_duration(wood, definition);
    let trace =
        crate::equipment::EquipmentOperationTrace::new(equipment, definition, record.condition());
    if tree_felling_condition_after(trace, duration) == crate::maintenance::Condition::FAILED {
        return Err(TreeFellingStartError::ChoppingToolTooWorn);
    }
    let completes_at = state
        .tick()
        .checked_add_span(duration)
        .ok_or(TreeFellingStartError::CompletionTickOverflow)?;
    let work = TreeFellingWork::new(root, trace, state.tick(), completes_at);
    let work_start = validate_player_work_start_control_rate(
        registries,
        state,
        PlayerWork::TreeFelling { work },
        duration,
        tree_felling_exertion(),
    )
    .map_err(TreeFellingStartError::Work)?;
    Ok(ValidatedTreeFellingStart {
        expected_world_revision: state.world().revision(),
        expected_logistics_revision: state.logistics().revision(),
        expected_surface_revision: state.surface().revision(),
        expected_next_resource_id: state.surface().next_resource_id(),
        expected_equipment_revision: state.equipment().revision(),
        work_start,
        work,
    })
}

/// Turn embodied standing timber into one recoverable, physically conserved log pile.
///
/// This lower-level custody boundary is also used by the timed chopping completion.
pub fn validate_fell_tree(
    registries: &Registries,
    state: &AppState,
    root: VoxelCoord,
) -> Result<ValidatedTreeFelling, TreeFellingError> {
    validate_fell_tree_at(registries, state, root, state.tick(), None)
}

pub(crate) fn validate_due_player_tree_felling(
    registries: &Registries,
    state: &AppState,
    next_tick: SimulationTick,
) -> Result<Option<ValidatedTreeFelling>, TreeFellingError> {
    let Some(work) = state.player_work().tree_felling_due_at(next_tick) else {
        return Ok(None);
    };
    Ok(Some(validate_fell_tree_at(
        registries,
        state,
        work.root(),
        next_tick,
        Some(work),
    )?))
}

fn validate_fell_tree_at(
    registries: &Registries,
    state: &AppState,
    root: VoxelCoord,
    at: SimulationTick,
    expected_work: Option<TreeFellingWork>,
) -> Result<ValidatedTreeFelling, TreeFellingError> {
    let player_action = validate_player_action(state).map_err(|error| match error {
        PlayerActionValidationError::PlayerNotAdmitted => TreeFellingError::PlayerNotAdmitted,
        PlayerActionValidationError::PlayerDead => TreeFellingError::PlayerDead,
    })?;
    let player = state
        .logistics()
        .player()
        .expect("validated player action requires a logistics player");
    if state.player_work().active() != expected_work.map(|work| PlayerWork::TreeFelling { work }) {
        return Err(TreeFellingError::PlayerBusy);
    }
    if let Some(work) = expected_work {
        validate_player_equipment_access(state, work.equipment())
            .map_err(|_| TreeFellingError::PlayerBusy)?;
    }
    let tree = state
        .world()
        .tree_at(root)
        .ok_or(TreeFellingError::TreeUnavailable { root })?;
    let position = player.position();
    if !(position.y() == root.y()
        && position
            .x()
            .abs_diff(root.x())
            .checked_add(position.z().abs_diff(root.z()))
            == Some(1))
    {
        return Err(TreeFellingError::TooFar {
            root,
            player: position,
        });
    }
    if state.world().assimilated_wood_at(at).is_none() {
        return Err(TreeFellingError::AtmosphericInputOverflow);
    }
    let expected_world_revision = state.world().revision();
    let next_world_revision = expected_world_revision
        .checked_add(1)
        .ok_or(TreeFellingError::WorldRevisionExhausted)?;
    let wood = tree.wood_at(at);
    let spec = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        root,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        wood,
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_WOOD),
    )
    .expect("a positive valid tree always yields a valid wood parcel");
    let surface_creation = validate_create_surface_resources_after_tree_felling_at(
        registries,
        state,
        [spec],
        at,
        root,
    )
    .map_err(|error| match error {
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
    })?;
    Ok(ValidatedTreeFelling {
        expected_world_revision,
        next_world_revision,
        expected_logistics_revision: state.logistics().revision(),
        expected_player_foot: player.body().foot_position(),
        player_action,
        outcome: TreeFellingOutcome {
            root,
            wood,
            drop: surface_creation.first_resource_id(),
        },
        surface_creation,
        expected_work,
        at,
    })
}
