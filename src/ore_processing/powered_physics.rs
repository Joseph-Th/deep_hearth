//! Shared condition-adjusted physics for finite-energy ore-processing batches.

use crate::capability::{CapabilityEvaluationError, CapabilityId, CapabilityValue};
use crate::core::quantity::{Energy, Mass, MassFlow, MassSpecificEnergy, Power};
use crate::core::state::AppState;
use crate::core::throughput::{calculate_mass_flow_capacity, calculate_mass_flow_duration_ceiling};
use crate::core::time::{PhysicalTickDuration, TickSpan};
use crate::energy::{
    EnergyStoreId, ValidatedEnergySupply, assess_energy_supply_access,
    calculate_mass_specific_energy, calculate_mass_specific_energy_capacity,
    calculate_power_duration_ceiling, integrate_power_or_saturate, validate_energy_supply_request,
};
use crate::equipment::{
    EquipmentDefinition, EquipmentId, ResolvedEquipmentProvider, ValidatedEquipmentUse,
    evaluate_equipment_capabilities_at_condition, resolve_equipment_capability,
    resolve_equipment_provider,
};
use crate::maintenance::{Condition, calculate_usable_condition_after_active_ticks};
use crate::production::ProcessId;
use crate::registry::Registries;

use super::PoweredOreProcessProfile;

mod errors;
mod validation;

pub(super) use errors::{
    PoweredOreEquipmentError, PoweredOreProviderError, PoweredOreSupplyError, PoweredOreTimingError,
};
pub use validation::PoweredOreJobValidationError;
pub(super) use validation::{resolve_powered_ore_job_replay, validate_powered_ore_job_replay};

/// Physical rate constraint that determines one powered ore-processing duration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PoweredOreBottleneck {
    Throughput,
    EnergyDelivery,
    Balanced,
}

/// Shared provider admission for one exact powered ore batch.
#[derive(Clone, Copy, Debug)]
pub(super) struct ResolvedPoweredOreProvider<'state> {
    provider: ResolvedEquipmentProvider<'state>,
    processing_rate: MassFlow,
}

impl ResolvedPoweredOreProvider<'_> {
    #[must_use]
    pub(super) const fn id(self) -> EquipmentId {
        self.provider.id()
    }

    #[must_use]
    pub(super) const fn condition_before(self) -> Condition {
        self.provider.condition()
    }

    #[must_use]
    pub(super) const fn processing_rate(self) -> MassFlow {
        self.processing_rate
    }

    pub(super) const fn validated_use(self) -> ValidatedEquipmentUse {
        self.provider.validated_use()
    }
}

/// Shared finite-energy and active-time resolution for an admitted powered ore batch.
#[derive(Clone, Copy, Debug)]
pub(super) struct ResolvedPoweredOreSupply {
    energy_supply: ValidatedEnergySupply,
    required_energy: Energy,
    timing: PoweredOreTiming,
}

impl ResolvedPoweredOreSupply {
    pub(super) const fn energy_supply(self) -> ValidatedEnergySupply {
        self.energy_supply
    }

    #[must_use]
    pub(super) const fn required_energy(self) -> Energy {
        self.required_energy
    }

    #[must_use]
    pub(super) const fn available_power(self) -> Power {
        self.energy_supply.max_output_power()
    }

    #[must_use]
    pub(super) const fn throughput_duration(self) -> TickSpan {
        self.timing.throughput_duration()
    }

    #[must_use]
    pub(super) const fn energy_duration(self) -> TickSpan {
        self.timing.energy_duration()
    }

    #[must_use]
    pub(super) fn duration(self) -> TickSpan {
        self.timing.duration()
    }

    #[must_use]
    pub(super) const fn condition_after(self) -> Condition {
        self.timing.condition_after()
    }
}

/// Resolves the common provider, capability, and batch-limit stage for powered ore processing.
///
/// The process-specific caller has already resolved the ore definition. Registry construction
/// guarantees its production cross-reference, so this shared stage treats that link as an
/// invariant and evaluates the generic capability requirements exactly once.
///
/// Process-specific output physics deliberately run after this stage and before finite-energy
/// admission so all three process families retain the same fail-closed error ordering.
pub(super) fn resolve_powered_ore_provider<'state>(
    registries: &'state Registries,
    state: &'state AppState,
    process: ProcessId,
    equipment: EquipmentId,
    profile: PoweredOreProcessProfile,
    selected_mass: Mass,
) -> Result<ResolvedPoweredOreProvider<'state>, PoweredOreProviderError> {
    let provider = resolve_equipment_provider(registries, state, equipment)
        .map_err(PoweredOreProviderError::Provider)?;
    validate_powered_ore_process_capabilities(
        registries,
        process,
        provider.definition(),
        provider.condition(),
    )
    .map_err(PoweredOreProviderError::Capability)?;
    let powered_equipment = resolve_powered_ore_equipment(
        provider.definition(),
        provider.condition(),
        profile.mass_flow_capability(),
        profile.max_batch_mass_capability(),
        selected_mass,
    )
    .map_err(PoweredOreProviderError::Equipment)?;
    Ok(ResolvedPoweredOreProvider {
        provider,
        processing_rate: powered_equipment.processing_rate(),
    })
}

/// Replays the condition-adjusted generic process requirements shared by powered ore planning,
/// live admission, and trusted-load validation.
pub(super) fn validate_powered_ore_process_capabilities(
    registries: &Registries,
    process: ProcessId,
    equipment: &EquipmentDefinition,
    condition: Condition,
) -> Result<(), CapabilityEvaluationError> {
    let process_definition = registries
        .production()
        .get_process(process)
        .unwrap_or_else(|| {
            panic!(
                "validated powered ore process {} lost its production definition",
                process.value()
            )
        });
    evaluate_equipment_capabilities_at_condition(
        registries.capabilities(),
        equipment,
        condition,
        process_definition.capability_requirements(),
    )
}

/// Resolves the common finite-energy, throughput, and condition-wear stage.
pub(super) fn resolve_powered_ore_supply(
    registries: &Registries,
    state: &AppState,
    energy_store: EnergyStoreId,
    profile: PoweredOreProcessProfile,
    selected_mass: Mass,
    processing_rate: MassFlow,
    condition_before: Condition,
) -> Result<ResolvedPoweredOreSupply, PoweredOreSupplyError> {
    let required_energy = calculate_mass_specific_energy(selected_mass, profile.specific_energy());
    let energy_access = assess_energy_supply_access(registries, state, energy_store)
        .map_err(PoweredOreSupplyError::Supply)?;
    if energy_access.carrier() != profile.energy_carrier() {
        return Err(PoweredOreSupplyError::WrongEnergyCarrier {
            required: profile.energy_carrier(),
            provided: energy_access.carrier(),
        });
    }
    let energy_supply = validate_energy_supply_request(energy_access, required_energy)
        .map_err(PoweredOreSupplyError::Supply)?;
    let timing = resolve_powered_ore_timing(
        registries,
        processing_rate,
        selected_mass,
        required_energy,
        energy_supply.max_output_power(),
        profile.condition_wear_ppm_per_active_tick(),
        condition_before,
    )
    .map_err(PoweredOreSupplyError::Timing)?;
    Ok(ResolvedPoweredOreSupply {
        energy_supply,
        required_energy,
        timing,
    })
}

/// Condition-adjusted rate and single-batch ceiling shared by resolution and planning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PoweredOreEquipmentLimits {
    processing_rate: MassFlow,
    maximum_batch_mass: Mass,
}

impl PoweredOreEquipmentLimits {
    #[must_use]
    pub(super) const fn processing_rate(self) -> MassFlow {
        self.processing_rate
    }

    #[must_use]
    pub(super) const fn maximum_batch_mass(self) -> Mass {
        self.maximum_batch_mass
    }
}

pub(super) fn classify_powered_ore_bottleneck(
    throughput_duration: TickSpan,
    energy_duration: TickSpan,
) -> PoweredOreBottleneck {
    match throughput_duration.cmp(&energy_duration) {
        std::cmp::Ordering::Greater => PoweredOreBottleneck::Throughput,
        std::cmp::Ordering::Less => PoweredOreBottleneck::EnergyDelivery,
        std::cmp::Ordering::Equal => PoweredOreBottleneck::Balanced,
    }
}

/// Condition-adjusted equipment throughput after common capability and batch-limit validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PoweredOreEquipment {
    processing_rate: MassFlow,
}

impl PoweredOreEquipment {
    #[must_use]
    pub(super) const fn processing_rate(self) -> MassFlow {
        self.processing_rate
    }
}

/// Exact active-time and wear result shared by admission and persistence replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PoweredOreTiming {
    throughput_duration: TickSpan,
    energy_duration: TickSpan,
    condition_after: Condition,
}

impl PoweredOreTiming {
    #[must_use]
    pub(super) const fn throughput_duration(self) -> TickSpan {
        self.throughput_duration
    }

    #[must_use]
    pub(super) const fn energy_duration(self) -> TickSpan {
        self.energy_duration
    }

    #[must_use]
    pub(super) fn duration(self) -> TickSpan {
        std::cmp::max(self.throughput_duration, self.energy_duration)
    }

    #[must_use]
    pub(super) const fn condition_after(self) -> Condition {
        self.condition_after
    }
}

/// Resolves common condition-adjusted equipment policy before output or energy admission.
///
/// Keeping this stage separate preserves the canonical error ordering: an impossible equipment
/// batch is rejected before process-specific output work or finite-energy validation is attempted.
pub(super) fn resolve_powered_ore_equipment(
    equipment: &EquipmentDefinition,
    condition_before: Condition,
    mass_flow_capability: CapabilityId,
    maximum_batch_mass_capability: CapabilityId,
    selected_mass: Mass,
) -> Result<PoweredOreEquipment, PoweredOreEquipmentError> {
    let limits = resolve_powered_ore_equipment_limits(
        equipment,
        condition_before,
        mass_flow_capability,
        maximum_batch_mass_capability,
    )?;
    if selected_mass > limits.maximum_batch_mass() {
        return Err(PoweredOreEquipmentError::BatchMassExceeded {
            selected: selected_mass,
            maximum: limits.maximum_batch_mass(),
        });
    }
    Ok(PoweredOreEquipment {
        processing_rate: limits.processing_rate(),
    })
}

/// Resolves the condition-adjusted physical rate and batch ceiling without choosing a batch mass.
pub(super) fn resolve_powered_ore_equipment_limits(
    equipment: &EquipmentDefinition,
    condition_before: Condition,
    mass_flow_capability: CapabilityId,
    maximum_batch_mass_capability: CapabilityId,
) -> Result<PoweredOreEquipmentLimits, PoweredOreEquipmentError> {
    let processing_rate =
        match resolve_equipment_capability(equipment, condition_before, mass_flow_capability) {
            Some(CapabilityValue::MassFlow(rate)) => rate,
            Some(
                CapabilityValue::Mass(_)
                | CapabilityValue::Temperature(_)
                | CapabilityValue::Pressure(_)
                | CapabilityValue::Power(_),
            )
            | None => return Err(PoweredOreEquipmentError::MissingMassFlowCapability),
        };
    let maximum_batch_mass = match resolve_equipment_capability(
        equipment,
        condition_before,
        maximum_batch_mass_capability,
    ) {
        Some(CapabilityValue::Mass(mass)) => mass,
        Some(
            CapabilityValue::Temperature(_)
            | CapabilityValue::Pressure(_)
            | CapabilityValue::Power(_)
            | CapabilityValue::MassFlow(_),
        )
        | None => {
            return Err(PoweredOreEquipmentError::MissingMaximumBatchMassCapability);
        }
    };
    Ok(PoweredOreEquipmentLimits {
        processing_rate,
        maximum_batch_mass,
    })
}

/// Greatest processable mass across throughput, available power, and an active-time budget.
///
/// This is the canonical powered-ore inverse used by both current-state envelopes and bounded
/// replenished-order projection. It deliberately excludes per-batch equipment ceilings and finite
/// stored charge so callers can compose those independent constraints explicitly.
pub(super) fn powered_ore_mass_capacity_for_active_ticks(
    processing_rate: MassFlow,
    available_power: Power,
    specific_energy: MassSpecificEnergy,
    ticks: TickSpan,
    physical_tick_duration: PhysicalTickDuration,
) -> Mass {
    let throughput_capacity =
        calculate_mass_flow_capacity(processing_rate, ticks, physical_tick_duration);
    let integrated = integrate_power_or_saturate(available_power, ticks, physical_tick_duration);
    throughput_capacity.min(calculate_mass_specific_energy_capacity(
        integrated,
        specific_energy,
    ))
}

/// Resolves common rate-bottleneck timing and condition wear after finite energy is validated.
pub(super) fn resolve_powered_ore_timing(
    registries: &Registries,
    processing_rate: MassFlow,
    selected_mass: Mass,
    required_energy: Energy,
    available_power: Power,
    condition_wear_ppm_per_active_tick: u32,
    condition_before: Condition,
) -> Result<PoweredOreTiming, PoweredOreTimingError> {
    let throughput_duration = calculate_mass_flow_duration_ceiling(
        processing_rate,
        selected_mass,
        registries.core().physical_tick_duration(),
    )
    .map_err(PoweredOreTimingError::Throughput)?;
    let energy_duration = calculate_power_duration_ceiling(
        available_power,
        required_energy,
        registries.core().physical_tick_duration(),
    )
    .map_err(PoweredOreTimingError::Energy)?;
    let duration = std::cmp::max(throughput_duration, energy_duration);
    let condition_after = calculate_usable_condition_after_active_ticks(
        condition_wear_ppm_per_active_tick,
        condition_before,
        duration,
    )
    .map_err(PoweredOreTimingError::Condition)?;

    Ok(PoweredOreTiming {
        throughput_duration,
        energy_duration,
        condition_after,
    })
}
