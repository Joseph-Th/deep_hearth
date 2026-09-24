//! Read-only long-order projection for replenished powered ore processing.

use crate::core::quantity::{Energy, Mass};
use crate::core::time::TickSpan;
use crate::energy::{
    EnergyStoreDefinition, EnergyStoreDefinitionId, calculate_mass_specific_energy,
    calculate_mass_specific_energy_capacity,
};
use crate::equipment::{EquipmentDefinition, EquipmentDefinitionId};
use crate::maintenance::{Condition, MaintenanceBand, maximum_usable_active_ticks};
use crate::production::ProcessId;
use crate::registry::Registries;

use super::definitions::PoweredOreProcessProfile;
use super::planning::powered_profile;
use super::powered_physics::{
    PoweredOreEquipmentError, PoweredOreTimingError, powered_ore_mass_capacity_for_active_ticks,
    resolve_powered_ore_equipment_limits, resolve_powered_ore_timing,
    validate_powered_ore_process_capabilities,
};

mod errors;

pub use errors::PoweredOreOrderError;

/// Optional equipment-service policy applied between projected powered-ore batches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PoweredOreOrderMaintenancePolicy {
    /// Carry condition until the order completes or physics can no longer admit another batch.
    Unserviced,
    /// Before each batch, service equipment already in its authored critical band back to the
    /// authored maintenance target. This projects physical continuity only; it does not promise
    /// replacement material, player attention, or service authorization.
    ServiceAtCritical,
}

/// Immutable inputs for a bounded replenished powered-ore work order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoweredOreOrderRequest {
    condition_before: Condition,
    requested_mass: Mass,
    max_batches: u64,
    maintenance_policy: PoweredOreOrderMaintenancePolicy,
}

impl PoweredOreOrderRequest {
    #[must_use]
    pub const fn new(
        condition_before: Condition,
        requested_mass: Mass,
        max_batches: u64,
        maintenance_policy: PoweredOreOrderMaintenancePolicy,
    ) -> Self {
        Self {
            condition_before,
            requested_mass,
            max_batches,
            maintenance_policy,
        }
    }
}

/// One projected replenished-energy batch in a powered ore order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoweredOreOrderBatch {
    mass: Mass,
    required_energy: Energy,
    duration: TickSpan,
    condition_before: Condition,
    condition_after: Condition,
}

impl PoweredOreOrderBatch {
    #[must_use]
    pub const fn mass(self) -> Mass {
        self.mass
    }

    #[must_use]
    pub const fn required_energy(self) -> Energy {
        self.required_energy
    }

    #[must_use]
    pub const fn duration(self) -> TickSpan {
        self.duration
    }

    #[must_use]
    pub const fn condition_before(self) -> Condition {
        self.condition_before
    }

    #[must_use]
    pub const fn condition_after(self) -> Condition {
        self.condition_after
    }
}

/// Physical long-order projection with exact replenishment events and carried equipment wear.
///
/// This is a planning result, not an authorization token. It assumes each projected batch begins
/// with the selected energy-store definition replenished to the exact energy required by that
/// batch. It does not inspect feed legality, inventory, output capacity, replacement stock,
/// survival reserves, occupancy, support, or future world supply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PoweredOreOrderResolution {
    batches: Vec<PoweredOreOrderBatch>,
    services: u64,
    active_duration: TickSpan,
    condition_after: Condition,
}

impl PoweredOreOrderResolution {
    #[must_use]
    pub fn batches(&self) -> &[PoweredOreOrderBatch] {
        &self.batches
    }

    #[must_use]
    pub const fn services(&self) -> u64 {
        self.services
    }

    #[must_use]
    pub const fn active_duration(&self) -> TickSpan {
        self.active_duration
    }

    #[must_use]
    pub const fn condition_after(&self) -> Condition {
        self.condition_after
    }
}

struct PoweredOreOrderContext<'a> {
    registries: &'a Registries,
    process: ProcessId,
    equipment: EquipmentDefinitionId,
    equipment_definition: &'a EquipmentDefinition,
    store_definition: &'a EnergyStoreDefinition,
    profile: PoweredOreProcessProfile,
    maintenance_policy: PoweredOreOrderMaintenancePolicy,
}

impl<'a> PoweredOreOrderContext<'a> {
    fn resolve(
        registries: &'a Registries,
        process: ProcessId,
        equipment: EquipmentDefinitionId,
        store: EnergyStoreDefinitionId,
        maintenance_policy: PoweredOreOrderMaintenancePolicy,
    ) -> Result<Self, PoweredOreOrderError> {
        let profile = powered_profile(registries, process)
            .ok_or(PoweredOreOrderError::UnknownPoweredProcess { process })?;
        let equipment_definition = registries
            .equipment()
            .get_equipment(equipment)
            .ok_or(PoweredOreOrderError::UnknownEquipment { equipment })?;
        let store_definition = registries
            .energy()
            .get_store(store)
            .ok_or(PoweredOreOrderError::UnknownEnergyStore { store })?;
        if store_definition.carrier() != profile.energy_carrier() {
            return Err(PoweredOreOrderError::WrongEnergyCarrier {
                required: profile.energy_carrier(),
                provided: store_definition.carrier(),
            });
        }
        Ok(Self {
            registries,
            process,
            equipment,
            equipment_definition,
            store_definition,
            profile,
            maintenance_policy,
        })
    }

    fn prepare_condition(
        &self,
        condition: Condition,
    ) -> Result<(Condition, bool), PoweredOreOrderError> {
        if self.maintenance_policy != PoweredOreOrderMaintenancePolicy::ServiceAtCritical
            || self
                .equipment_definition
                .maintenance_thresholds()
                .classify(condition)
                != MaintenanceBand::Critical
        {
            return Ok((condition, false));
        }
        let maintenance = self.equipment_definition.maintenance_profile().ok_or(
            PoweredOreOrderError::MaintenanceUnavailable {
                equipment: self.equipment,
            },
        )?;
        Ok((maintenance.restored_condition(), true))
    }

    fn condition_lifetime_mass(
        &self,
        processing_rate: crate::core::quantity::MassFlow,
        condition: Condition,
    ) -> Mass {
        let ticks = maximum_usable_active_ticks(
            self.profile.condition_wear_ppm_per_active_tick(),
            condition,
        );
        powered_ore_mass_capacity_for_active_ticks(
            processing_rate,
            self.store_definition.max_output_power(),
            self.profile.specific_energy(),
            ticks,
            self.registries.core().physical_tick_duration(),
        )
    }

    fn project_batch(
        &self,
        batch: u64,
        condition: Condition,
        remaining: Mass,
    ) -> Result<PoweredOreOrderBatch, PoweredOreOrderError> {
        validate_powered_ore_process_capabilities(
            self.registries,
            self.process,
            self.equipment_definition,
            condition,
        )
        .map_err(|error| PoweredOreOrderError::Capability { batch, error })?;
        let limits = resolve_powered_ore_equipment_limits(
            self.equipment_definition,
            condition,
            self.profile.mass_flow_capability(),
            self.profile.max_batch_mass_capability(),
        )
        .map_err(|error| map_equipment_error(batch, error))?;
        let energy_capacity = calculate_mass_specific_energy_capacity(
            self.store_definition.capacity(),
            self.profile.specific_energy(),
        );
        let mass = remaining
            .min(limits.maximum_batch_mass())
            .min(energy_capacity)
            .min(self.condition_lifetime_mass(limits.processing_rate(), condition));
        if mass.is_zero() {
            return Err(PoweredOreOrderError::NoBatchCapacity { batch, condition });
        }
        let required_energy = calculate_mass_specific_energy(mass, self.profile.specific_energy());
        let timing = resolve_powered_ore_timing(
            self.registries,
            limits.processing_rate(),
            mass,
            required_energy,
            self.store_definition.max_output_power(),
            self.profile.condition_wear_ppm_per_active_tick(),
            condition,
        )
        .map_err(|error| map_timing_error(batch, error))?;
        Ok(PoweredOreOrderBatch {
            mass,
            required_energy,
            duration: timing.duration(),
            condition_before: condition,
            condition_after: timing.condition_after(),
        })
    }
}

fn map_equipment_error(batch: u64, error: PoweredOreEquipmentError) -> PoweredOreOrderError {
    match error {
        PoweredOreEquipmentError::MissingMassFlowCapability => {
            PoweredOreOrderError::MissingMassFlowCapability { batch }
        }
        PoweredOreEquipmentError::MissingMaximumBatchMassCapability => {
            PoweredOreOrderError::MissingMaximumBatchMassCapability { batch }
        }
        PoweredOreEquipmentError::BatchMassExceeded { .. } => {
            unreachable!("order planning resolves limits before selecting batch mass")
        }
    }
}

fn map_timing_error(batch: u64, error: PoweredOreTimingError) -> PoweredOreOrderError {
    match error {
        PoweredOreTimingError::Throughput(error) => {
            PoweredOreOrderError::ThroughputDuration { batch, error }
        }
        PoweredOreTimingError::Energy(error) => {
            PoweredOreOrderError::EnergyDuration { batch, error }
        }
        PoweredOreTimingError::Condition(error) => {
            PoweredOreOrderError::ConditionDuration { batch, error }
        }
    }
}

/// Projects a bounded powered-ore order while carrying condition and replenishment boundaries.
pub fn project_powered_ore_order(
    registries: &Registries,
    process: ProcessId,
    equipment: EquipmentDefinitionId,
    store: EnergyStoreDefinitionId,
    request: PoweredOreOrderRequest,
) -> Result<PoweredOreOrderResolution, PoweredOreOrderError> {
    if request.requested_mass.is_zero() {
        return Err(PoweredOreOrderError::ZeroRequestedMass);
    }
    let context = PoweredOreOrderContext::resolve(
        registries,
        process,
        equipment,
        store,
        request.maintenance_policy,
    )?;

    let mut remaining = request.requested_mass;
    let mut condition = request.condition_before;
    let mut batches = Vec::new();
    let mut services = 0_u64;
    let mut total_ticks = 0_u64;
    while !remaining.is_zero() {
        let completed_batches = u64::try_from(batches.len()).unwrap_or_else(|_| {
            unreachable!("in-memory powered-ore batch count cannot exceed u64")
        });
        if completed_batches >= request.max_batches {
            return Err(PoweredOreOrderError::BatchLimitExceeded {
                maximum: request.max_batches,
            });
        }
        let (prepared_condition, serviced) = context.prepare_condition(condition)?;
        condition = prepared_condition;
        if serviced {
            services = services.checked_add(1).unwrap_or_else(|| {
                unreachable!("bounded powered-ore service count cannot exceed completed batches")
            });
        }
        let batch_number = completed_batches.checked_add(1).unwrap_or_else(|| {
            unreachable!("admitted powered-ore batch count is strictly below u64::MAX")
        });
        let batch = context.project_batch(batch_number, condition, remaining)?;
        total_ticks = total_ticks
            .checked_add(batch.duration().value())
            .ok_or(PoweredOreOrderError::DurationOverflow)?;
        condition = batch.condition_after();
        remaining = remaining
            .checked_sub(batch.mass())
            .unwrap_or_else(|| unreachable!("projected batch is bounded by remaining order mass"));
        batches.push(batch);
    }

    Ok(PoweredOreOrderResolution {
        batches,
        services,
        active_duration: TickSpan::new(total_ticks),
        condition_after: condition,
    })
}
