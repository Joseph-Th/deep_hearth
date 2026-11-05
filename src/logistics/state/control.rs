//! Buffered direct-control intent and fractionally accrued locomotion exertion.

use serde::{Deserialize, Serialize};

use crate::core::quantity::{Energy, Mass, Volume};
use crate::core::time::{CONTROL_TICKS_PER_SIMULATION_TICK, ControlTick};
use crate::spatial::{ViewDirection, WorldPoint};
use crate::structural::StructuralElementId;
use crate::survival::SurvivalExertion;

use super::PlayerActiveSelection;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LocomotionExertionAccumulator {
    pending_energy: Energy,
    pending_hydration: Volume,
    energy_remainder: u8,
    hydration_remainder: u8,
}

/// Persisted one-shot structural-break input waiting for the next slow-simulation boundary.
///
/// The exact structural identity and active selection are retained so delayed admission cannot
/// fall through to terrain excavation, a replacement member, or a different selected tool.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlayerPendingPrimaryBreak {
    requested_at: ControlTick,
    position: WorldPoint,
    element: StructuralElementId,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_required_optional")]
    selection: Option<PlayerActiveSelection>,
    direction_x_ppm: i32,
    direction_y_ppm: i32,
    direction_z_ppm: i32,
}

impl PlayerPendingPrimaryBreak {
    pub(crate) const fn new(
        requested_at: ControlTick,
        position: WorldPoint,
        element: StructuralElementId,
        selection: Option<PlayerActiveSelection>,
        direction: ViewDirection,
    ) -> Self {
        Self {
            requested_at,
            position,
            element,
            selection,
            direction_x_ppm: direction.x_parts_per_million(),
            direction_y_ppm: direction.y_parts_per_million(),
            direction_z_ppm: direction.z_parts_per_million(),
        }
    }

    #[must_use]
    pub(crate) const fn requested_at(self) -> ControlTick {
        self.requested_at
    }

    #[must_use]
    pub(crate) const fn position(self) -> WorldPoint {
        self.position
    }

    #[must_use]
    pub(crate) const fn element(self) -> StructuralElementId {
        self.element
    }

    #[must_use]
    pub(crate) const fn selection(self) -> Option<PlayerActiveSelection> {
        self.selection
    }

    #[must_use]
    pub(crate) fn direction(self) -> Option<ViewDirection> {
        ViewDirection::new(
            self.direction_x_ppm,
            self.direction_y_ppm,
            self.direction_z_ppm,
        )
        .ok()
    }
}

/// Persisted one-shot contextual-use input waiting for the next slow-simulation boundary.
///
/// The record retains the exact body origin and selection from the press. The eventual action
/// resolves against current owner state only if that origin and selection still match, so a
/// small movement within one voxel cannot silently retarget the old gesture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlayerPendingSecondaryUse {
    requested_at: ControlTick,
    position: WorldPoint,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_required_optional")]
    selection: Option<PlayerActiveSelection>,
    direction_x_ppm: i32,
    direction_y_ppm: i32,
    direction_z_ppm: i32,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_required_optional")]
    surface_request: Option<(crate::surface::SurfaceResourceId, Mass)>,
}

impl PlayerPendingSecondaryUse {
    pub(crate) const fn new(
        requested_at: ControlTick,
        position: WorldPoint,
        selection: Option<PlayerActiveSelection>,
        direction: ViewDirection,
    ) -> Self {
        Self {
            requested_at,
            position,
            selection,
            direction_x_ppm: direction.x_parts_per_million(),
            direction_y_ppm: direction.y_parts_per_million(),
            direction_z_ppm: direction.z_parts_per_million(),
            surface_request: None,
        }
    }

    pub(crate) const fn with_surface_request(
        mut self,
        request: Option<(crate::surface::SurfaceResourceId, Mass)>,
    ) -> Self {
        self.surface_request = request;
        self
    }

    #[must_use]
    pub(crate) const fn surface_request(self) -> Option<(crate::surface::SurfaceResourceId, Mass)> {
        self.surface_request
    }

    #[must_use]
    pub(crate) const fn requested_at(self) -> ControlTick {
        self.requested_at
    }

    #[must_use]
    pub(crate) const fn position(self) -> WorldPoint {
        self.position
    }

    #[must_use]
    pub(crate) const fn selection(self) -> Option<PlayerActiveSelection> {
        self.selection
    }

    #[must_use]
    pub(crate) fn direction(self) -> Option<ViewDirection> {
        ViewDirection::new(
            self.direction_x_ppm,
            self.direction_y_ppm,
            self.direction_z_ppm,
        )
        .ok()
    }
}
impl LocomotionExertionAccumulator {
    pub(crate) const ZERO: Self = Self {
        pending_energy: Energy::ZERO,
        pending_hydration: Volume::ZERO,
        energy_remainder: 0,
        hydration_remainder: 0,
    };

    pub(crate) fn checked_accrue(self, exertion: SurvivalExertion) -> Option<Self> {
        let denominator = u128::from(CONTROL_TICKS_PER_SIMULATION_TICK);
        let energy_numerator = exertion
            .energy_cost_per_tick()
            .nanojoules()
            .checked_add(u128::from(self.energy_remainder))?;
        let hydration_numerator = u128::from(exertion.hydration_loss_per_tick().microliters())
            .checked_add(u128::from(self.hydration_remainder))?;
        let energy_whole = energy_numerator / denominator;
        let hydration_whole = hydration_numerator / denominator;
        let energy_remainder = u8::try_from(energy_numerator % denominator).ok()?;
        let hydration_remainder = u8::try_from(hydration_numerator % denominator).ok()?;
        Some(Self {
            pending_energy: self
                .pending_energy
                .checked_add(Energy::from_nanojoules(energy_whole))?,
            pending_hydration: self
                .pending_hydration
                .checked_add(Volume::from_microliters(
                    u64::try_from(hydration_whole).ok()?,
                ))?,
            energy_remainder,
            hydration_remainder,
        })
    }

    /// Adds one discrete locomotion event whose authored cost is already a whole event total.
    ///
    /// Continuous body control uses [`Self::checked_accrue`] because its exertion is a slow-tick
    /// rate sampled at control frequency. Impulses such as jump launch must not be divided by the
    /// control/slow-tick ratio or their one-shot cost would depend on input sampling frequency.
    pub(crate) fn checked_accrue_impulse(self, exertion: SurvivalExertion) -> Option<Self> {
        Some(Self {
            pending_energy: self
                .pending_energy
                .checked_add(exertion.energy_cost_per_tick())?,
            pending_hydration: self
                .pending_hydration
                .checked_add(exertion.hydration_loss_per_tick())?,
            energy_remainder: self.energy_remainder,
            hydration_remainder: self.hydration_remainder,
        })
    }

    pub(crate) const fn pending(self) -> SurvivalExertion {
        SurvivalExertion::new(self.pending_energy, self.pending_hydration)
    }

    pub(crate) const fn settled(self) -> Self {
        Self {
            pending_energy: Energy::ZERO,
            pending_hydration: Volume::ZERO,
            energy_remainder: self.energy_remainder,
            hydration_remainder: self.hydration_remainder,
        }
    }

    pub(crate) const fn has_valid_remainders(self) -> bool {
        self.energy_remainder < CONTROL_TICKS_PER_SIMULATION_TICK as u8
            && self.hydration_remainder < CONTROL_TICKS_PER_SIMULATION_TICK as u8
    }
}
