//! Public observable result of one canonical simulation tick.

use crate::combat::{PrimitiveAttackError, PrimitiveAttackOutcome};
use crate::core::time::{CONTROL_TICKS_PER_SIMULATION_TICK, ControlTick, SimulationTick};
use crate::energy::{
    CombustionError, EnergyStoreAssemblyError, EnergyStoreDefinitionId, EnergyStoreId,
};
use crate::equipment::{EquipmentId, EquipmentMaintenanceOutcome};
use crate::geology::FieldProspectingOutcome;
use crate::inventory::{
    PlayerStoragePlacementError, StockpileId, StorageDefinitionId,
    StorageEnclosureDismantlingOutcome,
};
use crate::kobolds::{KoboldAlarmPulse, KoboldScoutEvent};
use crate::labor::{ManualPowerOutcome, StructuralConstructionWork, SurfaceGatheringWork};
use crate::logistics::{
    PlayerActiveSelection, PlayerBodyMotionOutcome, PlayerEquipmentCarryError,
    PlayerPendingPrimaryBreakError, PlayerPendingSecondaryUseError,
    PlayerQuickAccessSelectionError, PlayerQuickAccessSlot,
};
use crate::mining::MiningOutputReceipt;
use crate::production::{ProcessCompletion, ProductionAvailabilityChange};
use crate::spatial::{CubeFace, VoxelCoord};
use crate::structural::{
    PlayerGroundedStructuralConstructionError, PlayerStructuralDismantlingError,
    StructuralDismantlingOutcome, StructuralDismantlingStartOutcome, StructuralElementId,
};
use crate::surface::{
    PlayerMaterialDropError, PlayerMaterialDropOutcome, SurfaceDropPickupOutcome,
    SurfaceGatheringError, SurfaceGatheringOutcome,
};
use crate::survival::{
    DrinkOutcome, DrinkStoreToTargetError, EatOutcome, EatStackToTargetError, SurvivalAssessment,
};
use crate::world::{
    BlockExcavationError, BlockExcavationOutcome, BlockExcavationStartOutcome, BlockStateId,
    PlayerBlockTargetError,
};

/// Observable result of an attempted primary action on one authoritative control step.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrimaryActionOutcome {
    PlayerNotAdmitted,
    PlayerDead,
    Shove(PrimitiveAttackOutcome),
    ShoveRejected(PrimitiveAttackError),
    Strike(PrimitiveAttackOutcome),
    StrikeRejected(PrimitiveAttackError),
    BlockExcavationStarted(BlockExcavationStartOutcome),
    BlockExcavationRejected(BlockExcavationError),
    StructuralDismantlingStarted(StructuralDismantlingStartOutcome),
    StructuralDismantlingRejected(PlayerStructuralDismantlingError),
    AmbiguousStructuralTarget { count: usize },
    QueuedUntilSimulationBoundary,
    BufferedContextChanged,
    BufferRejected(PlayerPendingPrimaryBreakError),
}

/// Observable result of a direct-control quick-access slot switch.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QuickAccessSelectionOutcome {
    Selected {
        slot: PlayerQuickAccessSlot,
        selection: Option<PlayerActiveSelection>,
    },
    Rejected(PlayerQuickAccessSelectionError),
}

/// Observable result of one contextual-use request on the active quick-access selection.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SecondaryActionOutcome {
    EatingStarted(EatOutcome),
    DrinkingStarted(DrinkOutcome),
    EnergyStoreAssembled {
        definition: EnergyStoreDefinitionId,
        store: EnergyStoreId,
    },
    EnergyStoreAssemblyRejected {
        definition: EnergyStoreDefinitionId,
        error: EnergyStoreAssemblyError,
    },
    CombustionFuelAdded {
        store: EnergyStoreId,
        released: crate::core::quantity::Energy,
    },
    CombustionFuelRejected {
        store: EnergyStoreId,
        error: CombustionError,
    },
    SurfaceGatheringStarted(SurfaceGatheringWork),
    StructuralConstructionStarted(StructuralConstructionWork),
    StoragePlaced {
        definition: StorageDefinitionId,
        stockpile: StockpileId,
    },
    StoragePlacementRejected {
        definition: StorageDefinitionId,
        error: PlayerStoragePlacementError,
    },
    StorageTargetRejected(PlayerBlockTargetError),
    NoStoragePlacementTarget,
    StoragePlacementCoordinateOverflow,
    StoragePlacementWorldUnavailable {
        position: VoxelCoord,
    },
    StoragePlacementOccupied {
        position: VoxelCoord,
        state: BlockStateId,
    },
    StoragePlacementIntersectsPlayer {
        position: VoxelCoord,
    },
    StoragePlacementLocationOccupied {
        position: VoxelCoord,
    },
    StoragePlacementUnsupported {
        position: VoxelCoord,
    },
    StructuralTargetRejected(PlayerBlockTargetError),
    NoStructuralPlacementTarget,
    StructuralPlacementCoordinateOverflow,
    StructuralPlacementRequiresTopFace {
        face: CubeFace,
    },
    StructuralPlacementWorldUnavailable {
        position: VoxelCoord,
    },
    StructuralPlacementOccupied {
        position: VoxelCoord,
        state: BlockStateId,
    },
    StructuralPlacementIntersectsPlayer {
        position: VoxelCoord,
    },
    StructuralPlacementLocationOccupied {
        position: VoxelCoord,
    },
    AmbiguousStructuralPlacementTarget {
        count: usize,
    },
    StructuralPlacementRejected(PlayerGroundedStructuralConstructionError),
    AlreadySatisfied,
    QueuedUntilSimulationBoundary,
    BufferedContextChanged,
    BufferRejected(PlayerPendingSecondaryUseError),
    DeferredUntilSimulationBoundary,
    PlayerNotAdmitted,
    PlayerDead,
    NoActiveSelection,
    ActiveEquipmentNotContextual,
    MaterialUnavailable,
    AmbiguousMaterialStacks {
        count: usize,
    },
    AmbiguousStorageDefinitions {
        count: usize,
    },
    AmbiguousLocalCombustionStores {
        count: usize,
    },
    AmbiguousCombustionDefinitions {
        count: usize,
    },
    AmbiguousLocalDrinkSources {
        count: usize,
    },
    AmbiguousLocalSurfaceResources {
        count: usize,
    },
    AmbiguousSurfaceGatheringMethods {
        count: usize,
    },
    AmbiguousEmptyHandContext {
        drink_sources: usize,
        surface_resources: usize,
    },
    MaterialNotContextual,
    EatingRejected(EatStackToTargetError),
    DrinkingRejected(DrinkStoreToTargetError),
    SurfaceGatheringRejected(SurfaceGatheringError),
}

/// Observable result of a one-step request to drop the active quick-access selection.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActiveSelectionDropOutcome {
    PlayerNotAdmitted,
    PlayerDead,
    EquipmentDropped {
        equipment: EquipmentId,
        position: VoxelCoord,
    },
    MaterialDropped(PlayerMaterialDropOutcome),
    NoActiveSelection,
    EquipmentRejected(PlayerEquipmentCarryError),
    MaterialRejected(PlayerMaterialDropError),
}

#[must_use]
pub(super) struct DirectActionOutcomes {
    quick_access: Option<QuickAccessSelectionOutcome>,
    primary: Option<PrimaryActionOutcome>,
    secondary: Option<SecondaryActionOutcome>,
    drop: Option<ActiveSelectionDropOutcome>,
}

impl DirectActionOutcomes {
    pub(super) fn new(
        quick_access: Option<QuickAccessSelectionOutcome>,
        primary: Option<PrimaryActionOutcome>,
        secondary: Option<SecondaryActionOutcome>,
        drop: Option<ActiveSelectionDropOutcome>,
    ) -> Self {
        Self {
            quick_access,
            primary,
            secondary,
            drop,
        }
    }
}

/// Result of one canonical 20 Hz control/kinematic tick.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlTickOutcome {
    control_tick: ControlTick,
    simulation: Option<TickOutcome>,
    body_motion: Option<PlayerBodyMotionOutcome>,
    surface_drop_pickup: Option<SurfaceDropPickupOutcome>,
    block_excavation: Option<BlockExcavationOutcome>,
    quick_access_selection: Option<QuickAccessSelectionOutcome>,
    primary_action: Option<PrimaryActionOutcome>,
    secondary_action: Option<SecondaryActionOutcome>,
    active_selection_drop: Option<ActiveSelectionDropOutcome>,
}

impl ControlTickOutcome {
    pub(super) fn new(
        control_tick: ControlTick,
        simulation: Option<TickOutcome>,
        body_motion: Option<PlayerBodyMotionOutcome>,
        surface_drop_pickup: Option<SurfaceDropPickupOutcome>,
        block_excavation: Option<BlockExcavationOutcome>,
        direct_actions: DirectActionOutcomes,
    ) -> Self {
        let DirectActionOutcomes {
            quick_access: quick_access_selection,
            primary: primary_action,
            secondary: secondary_action,
            drop: active_selection_drop,
        } = direct_actions;
        Self {
            control_tick,
            simulation,
            body_motion,
            surface_drop_pickup,
            block_excavation,
            quick_access_selection,
            primary_action,
            secondary_action,
            active_selection_drop,
        }
    }

    #[must_use]
    pub const fn control_tick(&self) -> ControlTick {
        self.control_tick
    }

    /// Slow-simulation outcome when this control tick crossed its 72-step boundary.
    #[must_use]
    pub const fn simulation(&self) -> Option<&TickOutcome> {
        self.simulation.as_ref()
    }

    /// Continuous player-body motion committed on this 50 ms control step, if any.
    #[must_use]
    pub const fn body_motion(&self) -> Option<PlayerBodyMotionOutcome> {
        self.body_motion
    }

    /// Player-created loose drop mass transferred into carried custody on this control step.
    #[must_use]
    pub const fn surface_drop_pickup(&self) -> Option<SurfaceDropPickupOutcome> {
        self.surface_drop_pickup
    }

    /// Block-world matter completed on this control step, including off-boundary completions.
    #[must_use]
    pub const fn block_excavation(&self) -> Option<BlockExcavationOutcome> {
        self.block_excavation
    }

    /// Quick-access slot switch requested on this control step, if any.
    #[must_use]
    pub const fn quick_access_selection(&self) -> Option<&QuickAccessSelectionOutcome> {
        self.quick_access_selection.as_ref()
    }

    /// Primary interaction attempted on this control step, if any.
    #[must_use]
    pub const fn primary_action(&self) -> Option<&PrimaryActionOutcome> {
        self.primary_action.as_ref()
    }

    /// Contextual secondary interaction attempted on this control step, if any.
    #[must_use]
    pub const fn secondary_action(&self) -> Option<&SecondaryActionOutcome> {
        self.secondary_action.as_ref()
    }

    /// Earliest control steps until a queued primary or contextual-use request can be retried.
    ///
    /// A buffered press is acknowledged immediately, but slow-scheduled work can only be
    /// admitted on a simulation boundary. This is a bounded wait projection, not a guarantee
    /// that the target, survival state, or inventory will still authorize the action then.
    /// Clients can acknowledge a pending use visibly rather than appearing inert for seconds.
    #[must_use]
    pub fn queued_action_wait_control_ticks(&self) -> Option<u64> {
        if !matches!(
            self.primary_action,
            Some(PrimaryActionOutcome::QueuedUntilSimulationBoundary)
        ) && !matches!(
            self.secondary_action,
            Some(SecondaryActionOutcome::QueuedUntilSimulationBoundary)
        ) {
            return None;
        }
        let phase = self.control_tick.value() % CONTROL_TICKS_PER_SIMULATION_TICK;
        Some((CONTROL_TICKS_PER_SIMULATION_TICK - phase) % CONTROL_TICKS_PER_SIMULATION_TICK)
    }

    /// Active quick-access selection drop requested on this control step, if any.
    #[must_use]
    pub const fn active_selection_drop(&self) -> Option<&ActiveSelectionDropOutcome> {
        self.active_selection_drop.as_ref()
    }
}

/// Successful result of one canonical simulation tick.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TickOutcome {
    pub(super) tick: SimulationTick,
    pub(super) production_availability_changes: Vec<ProductionAvailabilityChange>,
    pub(super) production_completions: Vec<ProcessCompletion>,
    pub(super) mining_output: Option<MiningOutputReceipt>,
    pub(super) surface_gathering: Option<SurfaceGatheringOutcome>,
    pub(super) block_excavation: Option<BlockExcavationOutcome>,
    pub(super) manual_power: Option<ManualPowerOutcome>,
    pub(super) equipment_maintenance: Option<EquipmentMaintenanceOutcome>,
    pub(super) storage_enclosure_dismantling: Option<StorageEnclosureDismantlingOutcome>,
    pub(super) structural_construction: Option<StructuralElementId>,
    pub(super) structural_dismantling: Option<StructuralDismantlingOutcome>,
    pub(super) field_prospecting: Option<FieldProspectingOutcome>,
    pub(super) kobold_scout: Option<KoboldScoutEvent>,
    pub(super) kobold_alarm_pulses: Vec<KoboldAlarmPulse>,
    pub(super) survival: Option<SurvivalAssessment>,
}

impl TickOutcome {
    /// Returns the committed authoritative tick.
    #[must_use]
    pub const fn tick(&self) -> SimulationTick {
        self.tick
    }

    /// Returns production jobs suspended or resumed because provider availability changed during
    /// this tick. The changes are ordered by stable job ID.
    #[must_use]
    pub fn production_availability_changes(&self) -> &[ProductionAvailabilityChange] {
        &self.production_availability_changes
    }

    /// Returns process jobs whose outputs became authoritative during this tick.
    #[must_use]
    pub fn production_completions(&self) -> &[ProcessCompletion] {
        &self.production_completions
    }

    /// Returns mined matter that landed in inventory custody during this tick.
    #[must_use]
    pub const fn mining_output(&self) -> Option<&MiningOutputReceipt> {
        self.mining_output.as_ref()
    }

    /// Returns loose surface matter gathered into ordinary inventory on this tick, if any.
    #[must_use]
    pub const fn surface_gathering(&self) -> Option<&SurfaceGatheringOutcome> {
        self.surface_gathering.as_ref()
    }

    /// Returns block-world matter excavated into carried or finite surface custody this tick, if any.
    #[must_use]
    pub const fn block_excavation(&self) -> Option<BlockExcavationOutcome> {
        self.block_excavation
    }

    /// Returns direct player-powered energy generation that completed during this tick.
    #[must_use]
    pub const fn manual_power(&self) -> Option<ManualPowerOutcome> {
        self.manual_power
    }

    /// Returns equipment condition recovery completed by direct maintenance this tick.
    #[must_use]
    pub const fn equipment_maintenance(&self) -> Option<EquipmentMaintenanceOutcome> {
        self.equipment_maintenance
    }

    /// Returns enclosure matter recovered by direct dismantling completed during this tick.
    #[must_use]
    pub fn storage_enclosure_dismantling(&self) -> Option<&StorageEnclosureDismantlingOutcome> {
        self.storage_enclosure_dismantling.as_ref()
    }

    /// Returns the structural member that became load-bearing this tick, if any.
    #[must_use]
    pub const fn structural_construction(&self) -> Option<StructuralElementId> {
        self.structural_construction
    }

    /// Returns the structural member and exact recovered lots dismantled this tick, if any.
    #[must_use]
    pub const fn structural_dismantling(&self) -> Option<&StructuralDismantlingOutcome> {
        self.structural_dismantling.as_ref()
    }

    /// Returns the geological observation acquired by field prospecting on this tick, if any.
    #[must_use]
    pub const fn field_prospecting(&self) -> Option<FieldProspectingOutcome> {
        self.field_prospecting
    }

    /// Returns the post-tick player survival projection when survival has been initialized.
    #[must_use]
    pub const fn survival(&self) -> Option<SurvivalAssessment> {
        self.survival
    }

    /// A locally observable kobold scout movement or material investigation, if any.
    #[must_use]
    pub const fn kobold_scout(&self) -> Option<KoboldScoutEvent> {
        self.kobold_scout
    }

    /// Local percussion impulses from real kobold apparatus on this tick.
    /// A hidden distant warning never appears in this read-side event channel.
    #[must_use]
    pub fn kobold_alarm_pulses(&self) -> &[KoboldAlarmPulse] {
        &self.kobold_alarm_pulses
    }

    /// Returns how many discrete observable event categories fired during this tick.
    ///
    /// The committed tick itself and the continuous survival assessment are not discrete events.
    /// This deliberately destructures every outcome field so adding another tick result forces the
    /// runtime owner to decide whether it belongs in the observable event surface.
    #[must_use]
    pub fn discrete_event_category_count(&self) -> usize {
        let Self {
            tick: _,
            production_availability_changes,
            production_completions,
            mining_output,
            surface_gathering,
            block_excavation,
            manual_power,
            equipment_maintenance,
            storage_enclosure_dismantling,
            structural_construction,
            structural_dismantling,
            field_prospecting,
            kobold_scout,
            kobold_alarm_pulses,
            survival: _,
        } = self;
        [
            !production_availability_changes.is_empty(),
            !production_completions.is_empty(),
            mining_output.is_some(),
            surface_gathering.is_some(),
            block_excavation.is_some(),
            manual_power.is_some(),
            equipment_maintenance.is_some(),
            storage_enclosure_dismantling.is_some(),
            structural_construction.is_some(),
            structural_dismantling.is_some(),
            field_prospecting.is_some(),
            kobold_scout.is_some(),
            !kobold_alarm_pulses.is_empty(),
        ]
        .into_iter()
        .filter(|present| *present)
        .count()
    }
}
