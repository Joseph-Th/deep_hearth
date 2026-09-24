//! Read-only current-state planning bounds shared by powered ore-processing families.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::CapabilityEvaluationError;
use crate::core::quantity::{Energy, Mass, MassFlow, MassSpecificEnergy, Power};
use crate::core::state::AppState;
use crate::core::throughput::calculate_mass_flow_duration_ceiling;
use crate::core::time::{PhysicalTickDuration, TickSpan};
use crate::energy::{
    EnergyCarrier, EnergyStoreId, EnergySupplyError, assess_energy_supply_access,
    calculate_mass_specific_energy, calculate_mass_specific_energy_capacity,
    calculate_power_duration_ceiling,
};
use crate::equipment::{EquipmentId, EquipmentProviderError, resolve_available_equipment_provider};
use crate::maintenance::{
    Condition, maximum_active_ticks_above_condition_floor, maximum_usable_active_ticks,
};
use crate::production::ProcessId;
use crate::registry::{ProcessExecutionFamily, Registries};

use super::PoweredOreProcessProfile;
use super::powered_physics::{
    PoweredOreEquipmentError, powered_ore_mass_capacity_for_active_ticks,
    resolve_powered_ore_equipment_limits, validate_powered_ore_process_capabilities,
};

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

/// Current physical mass envelope for one powered ore process/provider/supply combination.
///
/// The envelope intentionally excludes process-specific feed and output rules. A mass inside this
/// bound can still fail canonical resolution because the selected matter is the wrong form,
/// composition, particle state, or otherwise invalid for that process. Exact operation resolution
/// remains the legality authority.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoweredOreMassEnvelope {
    equipment_capacity: Mass,
    stored_energy_capacity: Mass,
    condition_lifetime_capacity: Mass,
    available_energy: Energy,
    processing_rate: MassFlow,
    available_power: Power,
    specific_energy: MassSpecificEnergy,
    condition_before: Condition,
    wear_ppm_per_active_tick: u32,
    physical_tick_duration: PhysicalTickDuration,
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
    /// This keeps equipment capacity, output power, and condition lifetime authoritative while
    /// excluding only the store's current finite charge. It does not prove destination capacity for
    /// replenishment and does not reserve any resource.
    #[must_use]
    pub fn maximum_mass_with_replenished_energy(self) -> Mass {
        std::cmp::min(self.equipment_capacity, self.condition_lifetime_capacity)
    }

    /// Additional stored work needed to make `requested` physically possible on this same provider
    /// and supply, ignoring only the store's current finite charge.
    ///
    /// `None` means replenishing this store cannot make the requested mass fit because equipment,
    /// output-power, or condition lifetime is already the limiting constraint. Exact process input
    /// and output legality still belongs to the process-specific resolver.
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

/// Failure to derive a shared powered-ore mass envelope from current observable owners.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PoweredOreMassEnvelopeError {
    UnknownPoweredProcess {
        process: ProcessId,
    },
    Equipment(EquipmentProviderError),
    Capability(CapabilityEvaluationError),
    Energy(EnergySupplyError),
    MissingMassFlowCapability,
    MissingMaximumBatchMassCapability,
    WrongEnergyCarrier {
        required: EnergyCarrier,
        provided: EnergyCarrier,
    },
}

impl Display for PoweredOreMassEnvelopeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownPoweredProcess { process } => write!(
                formatter,
                "process {} has no authored powered ore-processing profile",
                process.value()
            ),
            Self::Equipment(error) => {
                write!(formatter, "powered ore provider unavailable: {error}")
            }
            Self::Capability(error) => {
                write!(formatter, "powered ore provider capability failed: {error}")
            }
            Self::Energy(error) => {
                write!(formatter, "powered ore energy supply unavailable: {error}")
            }
            Self::MissingMassFlowCapability => {
                formatter.write_str("powered ore provider lacks its authored mass-flow capability")
            }
            Self::MissingMaximumBatchMassCapability => formatter
                .write_str("powered ore provider lacks its authored maximum-batch capability"),
            Self::WrongEnergyCarrier { required, provided } => write!(
                formatter,
                "powered ore process requires {required:?} energy but supply provides {provided:?}"
            ),
        }
    }
}

impl Error for PoweredOreMassEnvelopeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Equipment(error) => Some(error),
            Self::Capability(error) => Some(error),
            Self::Energy(error) => Some(error),
            Self::UnknownPoweredProcess { .. }
            | Self::MissingMassFlowCapability
            | Self::MissingMaximumBatchMassCapability
            | Self::WrongEnergyCarrier { .. } => None,
        }
    }
}

/// Derives the current monotonic mass bounds shared by powered comminution, screening, and
/// constituent separation.
///
/// This projection validates current equipment support, capability, and occupancy plus current
/// energy-supply access, but it does not reserve either resource and does not inspect a material
/// selection. Callers use it to size a candidate, then invoke the process-specific resolver for
/// exact feed/output legality.
pub fn assess_powered_ore_mass_envelope(
    registries: &Registries,
    state: &AppState,
    process: ProcessId,
    equipment: EquipmentId,
    energy_store: EnergyStoreId,
) -> Result<PoweredOreMassEnvelope, PoweredOreMassEnvelopeError> {
    let profile = powered_profile(registries, process)
        .ok_or(PoweredOreMassEnvelopeError::UnknownPoweredProcess { process })?;
    let provider = resolve_available_equipment_provider(registries, state, equipment)
        .map_err(PoweredOreMassEnvelopeError::Equipment)?;
    validate_powered_ore_process_capabilities(
        registries,
        process,
        provider.definition(),
        provider.condition(),
    )
    .map_err(PoweredOreMassEnvelopeError::Capability)?;
    let equipment_limits = resolve_powered_ore_equipment_limits(
        provider.definition(),
        provider.condition(),
        profile.mass_flow_capability(),
        profile.max_batch_mass_capability(),
    )
    .map_err(|error| match error {
        PoweredOreEquipmentError::MissingMassFlowCapability => {
            PoweredOreMassEnvelopeError::MissingMassFlowCapability
        }
        PoweredOreEquipmentError::MissingMaximumBatchMassCapability => {
            PoweredOreMassEnvelopeError::MissingMaximumBatchMassCapability
        }
        PoweredOreEquipmentError::BatchMassExceeded { .. } => {
            unreachable!("planning resolves equipment limits without a selected batch")
        }
    })?;
    let energy = assess_energy_supply_access(registries, state, energy_store)
        .map_err(PoweredOreMassEnvelopeError::Energy)?;
    if energy.carrier() != profile.energy_carrier() {
        return Err(PoweredOreMassEnvelopeError::WrongEnergyCarrier {
            required: profile.energy_carrier(),
            provided: energy.carrier(),
        });
    }

    let specific_energy = profile.specific_energy();
    let stored_energy_capacity =
        calculate_mass_specific_energy_capacity(energy.available(), specific_energy);
    let condition_ticks = maximum_usable_active_ticks(
        profile.condition_wear_ppm_per_active_tick(),
        provider.condition(),
    );
    let physical_tick_duration = registries.core().physical_tick_duration();
    let condition_lifetime_capacity = powered_ore_mass_capacity_for_active_ticks(
        equipment_limits.processing_rate(),
        energy.max_output_power(),
        specific_energy,
        condition_ticks,
        physical_tick_duration,
    );

    Ok(PoweredOreMassEnvelope {
        equipment_capacity: equipment_limits.maximum_batch_mass(),
        stored_energy_capacity,
        condition_lifetime_capacity,
        available_energy: energy.available(),
        processing_rate: equipment_limits.processing_rate(),
        available_power: energy.max_output_power(),
        specific_energy,
        condition_before: provider.condition(),
        wear_ppm_per_active_tick: profile.condition_wear_ppm_per_active_tick(),
        physical_tick_duration,
    })
}

pub(super) fn powered_profile(
    registries: &Registries,
    process: ProcessId,
) -> Option<PoweredOreProcessProfile> {
    let topology = registries.process_topology(process)?;
    match topology.execution_family() {
        ProcessExecutionFamily::Comminution => Some(
            registries
                .ore_processing()
                .get_comminution(process)
                .unwrap_or_else(|| {
                    panic!(
                        "comminution topology references missing ore process {}",
                        process.value()
                    )
                })
                .operating_profile(),
        ),
        ProcessExecutionFamily::Screening => Some(
            registries
                .ore_processing()
                .get_screening(process)
                .unwrap_or_else(|| {
                    panic!(
                        "screening topology references missing ore process {}",
                        process.value()
                    )
                })
                .operating_profile(),
        ),
        ProcessExecutionFamily::ConstituentSeparation => Some(
            registries
                .ore_processing()
                .get_constituent_separation(process)
                .unwrap_or_else(|| {
                    panic!(
                        "constituent-separation topology references missing ore process {}",
                        process.value()
                    )
                })
                .operating_profile(),
        ),
        ProcessExecutionFamily::ManualCraft
        | ProcessExecutionFamily::PoweredCraft
        | ProcessExecutionFamily::ManualComminution
        | ProcessExecutionFamily::ManualSeparation
        | ProcessExecutionFamily::SensibleHeating
        | ProcessExecutionFamily::Melting
        | ProcessExecutionFamily::Casting => None,
    }
}

#[cfg(test)]
#[path = "planning_tests.rs"]
mod tests;
