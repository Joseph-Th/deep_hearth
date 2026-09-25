//! Powered-ore mass planning envelope representation and monotonic bound queries.

use crate::core::quantity::{Energy, Mass, MassFlow, MassSpecificEnergy, Power};
use crate::core::throughput::calculate_mass_flow_duration_ceiling;
use crate::core::time::{PhysicalTickDuration, TickSpan};
use crate::energy::{
    calculate_mass_specific_energy, calculate_mass_specific_energy_capacity,
    calculate_power_duration_ceiling,
};
use crate::maintenance::{Condition, maximum_active_ticks_above_condition_floor};

use crate::ore_processing::powered_physics::powered_ore_mass_capacity_for_active_ticks;

/// First shared scale constraint that rejects a requested powered ore batch.
///
/// Ordering matches canonical powered-ore resolution after process-specific input validation:
/// condition-adjusted equipment capacity, finite stored energy, then active condition lifetime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PoweredOreMassConstraint {
    EquipmentCapacity,
    StoredEnergy,
    ConditionLifetime,
}

/// First physical scale constraint that still applies after this same energy store is replenished.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PoweredOreReplenishmentConstraint {
    EquipmentCapacity,
    StoreCapacity,
    ConditionLifetime,
}

/// Current physical mass envelope for one powered ore process/provider/supply combination.
///
/// The envelope intentionally excludes process-specific feed and output rules. A mass inside this
/// bound can still fail canonical resolution because the selected matter is the wrong form,
/// composition, particle state, or otherwise invalid for that process. Exact operation resolution
/// remains the legality authority.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoweredOreMassEnvelope {
    pub(super) equipment_capacity: Mass,
    pub(super) stored_energy_capacity: Mass,
    pub(super) replenished_energy_capacity: Mass,
    pub(super) condition_lifetime_capacity: Mass,
    pub(super) available_energy: Energy,
    pub(super) processing_rate: MassFlow,
    pub(super) available_power: Power,
    pub(super) specific_energy: MassSpecificEnergy,
    pub(super) condition_before: Condition,
    pub(super) wear_ppm_per_active_tick: u32,
    pub(super) physical_tick_duration: PhysicalTickDuration,
}

impl PoweredOreMassEnvelope {
    #[must_use]
    pub const fn equipment_capacity(self) -> Mass {
        self.equipment_capacity
    }

    /// Exact active production duration for `requested` if this same store can be replenished first.
    ///
    /// This keeps current condition-adjusted throughput, output power, and the authored physical tick
    /// duration authoritative while excluding only the store's current finite charge. `None` means
    /// the requested mass already exceeds a non-energy constraint or a duration cannot be represented.
    #[must_use]
    pub fn duration_for_mass_with_replenished_energy(self, requested: Mass) -> Option<TickSpan> {
        if requested > self.maximum_mass_with_replenished_energy() {
            return None;
        }
        let throughput_duration = calculate_mass_flow_duration_ceiling(
            self.processing_rate,
            requested,
            self.physical_tick_duration,
        )
        .ok()?;
        let required_energy = calculate_mass_specific_energy(requested, self.specific_energy);
        let energy_duration = calculate_power_duration_ceiling(
            self.available_power,
            required_energy,
            self.physical_tick_duration,
        )
        .ok()?;
        Some(std::cmp::max(throughput_duration, energy_duration))
    }

    /// Greatest mass that could be run if this same currently available supply were replenished.
    ///
    /// This keeps equipment capacity, total store capacity, output power, and condition lifetime
    /// authoritative while excluding only the store's current finite charge. It does not prove how
    /// replenishment is acquired and does not reserve any resource.
    #[must_use]
    pub fn maximum_mass_with_replenished_energy(self) -> Mass {
        self.equipment_capacity
            .min(self.replenished_energy_capacity)
            .min(self.condition_lifetime_capacity)
    }

    /// Returns the first physical scale constraint that rejects `requested` after replenishment.
    #[must_use]
    pub fn replenishment_constraint_for(
        self,
        requested: Mass,
    ) -> Option<PoweredOreReplenishmentConstraint> {
        if requested > self.equipment_capacity {
            Some(PoweredOreReplenishmentConstraint::EquipmentCapacity)
        } else if requested > self.replenished_energy_capacity {
            Some(PoweredOreReplenishmentConstraint::StoreCapacity)
        } else if requested > self.condition_lifetime_capacity {
            Some(PoweredOreReplenishmentConstraint::ConditionLifetime)
        } else {
            None
        }
    }

    /// Greatest mass possible with an exact amount of energy currently available in this supply.
    ///
    /// The caller owns how that available-energy state could be reached. This projection only
    /// combines it with the already-resolved store, equipment, and condition limits.
    #[must_use]
    pub fn maximum_mass_with_available_energy(self, available: Energy) -> Mass {
        let energy_capacity =
            calculate_mass_specific_energy_capacity(available, self.specific_energy);
        self.maximum_mass_with_replenished_energy()
            .min(energy_capacity)
    }

    /// Exact stored energy required to process `requested` within this replenished envelope.
    #[must_use]
    pub fn required_energy_for(self, requested: Mass) -> Option<Energy> {
        (requested <= self.maximum_mass_with_replenished_energy())
            .then(|| calculate_mass_specific_energy(requested, self.specific_energy))
    }

    /// Additional stored work needed to make `requested` physically possible on this same provider
    /// and supply, ignoring only the store's current finite charge.
    ///
    /// `None` means replenishing this store cannot make the requested mass fit because equipment,
    /// total store capacity, output-power, or condition lifetime is already limiting. Exact process
    /// input and output legality still belongs to the process-specific resolver.
    #[must_use]
    pub fn additional_energy_required_for(self, requested: Mass) -> Option<Energy> {
        if requested > self.maximum_mass_with_replenished_energy() {
            return None;
        }
        let required = calculate_mass_specific_energy(requested, self.specific_energy);
        Some(
            required
                .checked_sub(self.available_energy)
                .unwrap_or(Energy::ZERO),
        )
    }

    #[must_use]
    pub const fn stored_energy_capacity(self) -> Mass {
        self.stored_energy_capacity
    }

    #[must_use]
    pub const fn condition_lifetime_capacity(self) -> Mass {
        self.condition_lifetime_capacity
    }

    /// Greatest mass admitted by all shared powered-ore scale constraints.
    #[must_use]
    pub fn maximum_mass(self) -> Mass {
        self.equipment_capacity
            .min(self.stored_energy_capacity)
            .min(self.condition_lifetime_capacity)
    }

    /// Returns the first shared canonical scale constraint that rejects `requested`.
    #[must_use]
    pub fn constraint_for(self, requested: Mass) -> Option<PoweredOreMassConstraint> {
        if requested > self.equipment_capacity {
            Some(PoweredOreMassConstraint::EquipmentCapacity)
        } else if requested > self.stored_energy_capacity {
            Some(PoweredOreMassConstraint::StoredEnergy)
        } else if requested > self.condition_lifetime_capacity {
            Some(PoweredOreMassConstraint::ConditionLifetime)
        } else {
            None
        }
    }

    /// Greatest mass that also leaves resulting equipment condition strictly above `floor`.
    ///
    /// The caller chooses the floor. This keeps maintenance policy outside ore physics while
    /// avoiding repeated nearby resolution attempts solely to discover a monotonic condition bound.
    #[must_use]
    pub fn maximum_mass_preserving_condition_above(self, floor: Condition) -> Mass {
        let safe_ticks = maximum_active_ticks_above_condition_floor(
            self.wear_ppm_per_active_tick,
            self.condition_before,
            floor,
        );
        self.maximum_mass_for_active_ticks(safe_ticks)
    }

    /// Upper bound on cumulative mass that can be processed before reaching `floor` if this same
    /// energy supply can be replenished between batches.
    ///
    /// Unlike the single-batch envelope, this intentionally excludes current finite charge and the
    /// equipment's per-batch mass cap because neither limits cumulative work across repeated legal
    /// batches. It retains the currently resolved throughput, output-power, wear, and physical tick
    /// duration. Authored condition curves are validated to never improve as condition degrades, so
    /// a remaining work order larger than this bound cannot finish above `floor` without maintenance.
    /// A work order inside the bound is not guaranteed to finish there because later batches may
    /// resolve lower capabilities; callers should reassess after each completed batch.
    #[must_use]
    pub fn cumulative_mass_preserving_condition_above_with_replenished_energy(
        self,
        floor: Condition,
    ) -> Mass {
        let safe_ticks = maximum_active_ticks_above_condition_floor(
            self.wear_ppm_per_active_tick,
            self.condition_before,
            floor,
        );
        powered_ore_mass_capacity_for_active_ticks(
            self.processing_rate,
            self.available_power,
            self.specific_energy,
            safe_ticks,
            self.physical_tick_duration,
        )
    }

    fn maximum_mass_for_active_ticks(self, ticks: TickSpan) -> Mass {
        let active_time_capacity = powered_ore_mass_capacity_for_active_ticks(
            self.processing_rate,
            self.available_power,
            self.specific_energy,
            ticks,
            self.physical_tick_duration,
        );
        self.equipment_capacity
            .min(self.stored_energy_capacity)
            .min(active_time_capacity)
    }
}
