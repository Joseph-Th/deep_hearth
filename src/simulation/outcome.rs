//! Public observable result of one canonical simulation tick.

use crate::core::time::{ControlTick, SimulationTick};
use crate::equipment::EquipmentMaintenanceOutcome;
use crate::geology::FieldProspectingOutcome;
use crate::inventory::StorageEnclosureDismantlingOutcome;
use crate::labor::ManualPowerOutcome;
use crate::logistics::{PlayerBodyMotionOutcome, PlayerStepOutcome};
use crate::mining::MiningOutputReceipt;
use crate::production::{ProcessCompletion, ProductionAvailabilityChange};
use crate::structural::StructuralElementId;
use crate::surface::SurfaceGatheringOutcome;
use crate::survival::SurvivalAssessment;

/// Result of one canonical 20 Hz control/kinematic tick.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlTickOutcome {
    control_tick: ControlTick,
    simulation: Option<TickOutcome>,
    body_motion: Option<PlayerBodyMotionOutcome>,
}

impl ControlTickOutcome {
    pub(super) const fn new(
        control_tick: ControlTick,
        simulation: Option<TickOutcome>,
        body_motion: Option<PlayerBodyMotionOutcome>,
    ) -> Self {
        Self {
            control_tick,
            simulation,
            body_motion,
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
}

/// Successful result of one canonical simulation tick.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TickOutcome {
    pub(super) tick: SimulationTick,
    pub(super) production_availability_changes: Vec<ProductionAvailabilityChange>,
    pub(super) production_completions: Vec<ProcessCompletion>,
    pub(super) mining_output: Option<MiningOutputReceipt>,
    pub(super) player_step: Option<PlayerStepOutcome>,
    pub(super) surface_gathering: Option<SurfaceGatheringOutcome>,
    pub(super) manual_power: Option<ManualPowerOutcome>,
    pub(super) equipment_maintenance: Option<EquipmentMaintenanceOutcome>,
    pub(super) storage_enclosure_dismantling: Option<StorageEnclosureDismantlingOutcome>,
    pub(super) structural_construction: Option<StructuralElementId>,
    pub(super) field_prospecting: Option<FieldProspectingOutcome>,
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

    /// Returns the adjacent player locality change completed this tick, if any.
    #[must_use]
    pub const fn player_step(&self) -> Option<PlayerStepOutcome> {
        self.player_step
    }

    /// Returns loose surface matter gathered into ordinary inventory on this tick, if any.
    #[must_use]
    pub const fn surface_gathering(&self) -> Option<&SurfaceGatheringOutcome> {
        self.surface_gathering.as_ref()
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

    /// Returns the grounded structural member that became load-bearing this tick, if any.
    #[must_use]
    pub const fn structural_construction(&self) -> Option<StructuralElementId> {
        self.structural_construction
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
            player_step,
            surface_gathering,
            manual_power,
            equipment_maintenance,
            storage_enclosure_dismantling,
            structural_construction,
            field_prospecting,
            survival: _,
        } = self;
        [
            !production_availability_changes.is_empty(),
            !production_completions.is_empty(),
            mining_output.is_some(),
            player_step.is_some(),
            surface_gathering.is_some(),
            manual_power.is_some(),
            equipment_maintenance.is_some(),
            storage_enclosure_dismantling.is_some(),
            structural_construction.is_some(),
            field_prospecting.is_some(),
        ]
        .into_iter()
        .filter(|present| *present)
        .count()
    }
}
