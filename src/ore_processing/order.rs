//! Read-only long-order projection for replenished powered ore processing.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::CapabilityEvaluationError;
use crate::core::quantity::{Energy, Mass};
use crate::core::throughput::{MassFlowDurationError, calculate_mass_flow_capacity};
use crate::core::time::TickSpan;
use crate::energy::{
    EnergyCarrier, EnergyStoreDefinition, EnergyStoreDefinitionId, PowerDurationError,
    calculate_mass_specific_energy, calculate_mass_specific_energy_capacity,
    integrate_power_or_saturate,
};
use crate::equipment::{EquipmentDefinition, EquipmentDefinitionId};
use crate::maintenance::{
    ActiveConditionDurationError, Condition, MaintenanceBand, maximum_usable_active_ticks,
};
use crate::production::ProcessId;
use crate::registry::Registries;

use super::definitions::PoweredOreProcessProfile;
use super::planning::powered_profile;
use super::powered_physics::{
    PoweredOreEquipmentError, PoweredOreTimingError, resolve_powered_ore_equipment_limits,
    resolve_powered_ore_timing, validate_powered_ore_process_capabilities,
};

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

/// Failure to project a bounded powered-ore order from immutable authored physics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PoweredOreOrderError {
    ZeroRequestedMass,
    BatchLimitExceeded {
        maximum: u64,
    },
    UnknownPoweredProcess {
        process: ProcessId,
    },
    UnknownEquipment {
        equipment: EquipmentDefinitionId,
    },
    UnknownEnergyStore {
        store: EnergyStoreDefinitionId,
    },
    WrongEnergyCarrier {
        required: EnergyCarrier,
        provided: EnergyCarrier,
    },
    Capability {
        batch: u64,
        error: CapabilityEvaluationError,
    },
    MissingMassFlowCapability {
        batch: u64,
    },
    MissingMaximumBatchMassCapability {
        batch: u64,
    },
    NoBatchCapacity {
        batch: u64,
        condition: Condition,
    },
    MaintenanceUnavailable {
        equipment: EquipmentDefinitionId,
    },
    ThroughputDuration {
        batch: u64,
        error: MassFlowDurationError,
    },
    EnergyDuration {
        batch: u64,
        error: PowerDurationError,
    },
    ConditionDuration {
        batch: u64,
        error: ActiveConditionDurationError,
    },
    DurationOverflow,
}

impl Display for PoweredOreOrderError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroRequestedMass => {
                formatter.write_str("powered ore order mass must be nonzero")
            }
            Self::BatchLimitExceeded { maximum } => write!(
                formatter,
                "powered ore order exceeds the caller's {maximum}-batch projection bound"
            ),
            Self::UnknownPoweredProcess { process } => write!(
                formatter,
                "process {} has no authored powered ore-processing profile",
                process.value()
            ),
            Self::UnknownEquipment { equipment } => {
                write!(
                    formatter,
                    "unknown equipment definition {}",
                    equipment.value()
                )
            }
            Self::UnknownEnergyStore { store } => {
                write!(
                    formatter,
                    "unknown energy-store definition {}",
                    store.value()
                )
            }
            Self::WrongEnergyCarrier { required, provided } => write!(
                formatter,
                "powered ore process requires {required:?} energy but replenished store provides {provided:?}"
            ),
            Self::Capability { batch, error } => {
                write!(
                    formatter,
                    "powered ore order batch {batch} capability failed: {error}"
                )
            }
            Self::MissingMassFlowCapability { batch } => write!(
                formatter,
                "powered ore order batch {batch} lacks its authored mass-flow capability"
            ),
            Self::MissingMaximumBatchMassCapability { batch } => write!(
                formatter,
                "powered ore order batch {batch} lacks its authored maximum-batch capability"
            ),
            Self::NoBatchCapacity { batch, condition } => write!(
                formatter,
                "powered ore order batch {batch} has no positive capacity at {} ppm condition",
                condition.parts_per_million()
            ),
            Self::MaintenanceUnavailable { equipment } => write!(
                formatter,
                "equipment definition {} entered its critical band without an authored maintenance profile",
                equipment.value()
            ),
            Self::ThroughputDuration { batch, error } => {
                write!(
                    formatter,
                    "powered ore order batch {batch} throughput duration failed: {error}"
                )
            }
            Self::EnergyDuration { batch, error } => {
                write!(
                    formatter,
                    "powered ore order batch {batch} energy duration failed: {error}"
                )
            }
            Self::ConditionDuration { batch, error } => {
                write!(
                    formatter,
                    "powered ore order batch {batch} condition failed: {error}"
                )
            }
            Self::DurationOverflow => {
                formatter.write_str("powered ore order active duration exceeds tick range")
            }
        }
    }
}

impl Error for PoweredOreOrderError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Capability { error, .. } => Some(error),
            Self::ThroughputDuration { error, .. } => Some(error),
            Self::EnergyDuration { error, .. } => Some(error),
            Self::ConditionDuration { error, .. } => Some(error),
            Self::ZeroRequestedMass
            | Self::BatchLimitExceeded { .. }
            | Self::UnknownPoweredProcess { .. }
            | Self::UnknownEquipment { .. }
            | Self::UnknownEnergyStore { .. }
            | Self::WrongEnergyCarrier { .. }
            | Self::MissingMassFlowCapability { .. }
            | Self::MissingMaximumBatchMassCapability { .. }
            | Self::NoBatchCapacity { .. }
            | Self::MaintenanceUnavailable { .. }
            | Self::DurationOverflow => None,
        }
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
        let throughput = calculate_mass_flow_capacity(
            processing_rate,
            ticks,
            self.registries.core().physical_tick_duration(),
        );
        let integrated = integrate_power_or_saturate(
            self.store_definition.max_output_power(),
            ticks,
            self.registries.core().physical_tick_duration(),
        );
        throughput.min(calculate_mass_specific_energy_capacity(
            integrated,
            self.profile.specific_energy(),
        ))
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
        if u64::try_from(batches.len()).unwrap_or(u64::MAX) >= request.max_batches {
            return Err(PoweredOreOrderError::BatchLimitExceeded {
                maximum: request.max_batches,
            });
        }
        let (prepared_condition, serviced) = context.prepare_condition(condition)?;
        condition = prepared_condition;
        if serviced {
            services = services
                .checked_add(1)
                .ok_or(PoweredOreOrderError::DurationOverflow)?;
        }
        let batch_number = u64::try_from(batches.len())
            .unwrap_or(u64::MAX)
            .saturating_add(1);
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
