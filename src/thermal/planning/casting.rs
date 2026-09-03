//! Current-state homogeneous-lot casting mass projection.

use crate::core::quantity::{Energy, Mass, Power};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::energy::{
    EnergyStoreId, ValidatedEnergySinkAccess, calculate_power_duration_ceiling,
    validate_energy_sink_access,
};
use crate::equipment::EquipmentId;
use crate::inventory::{MaterialLotSelection, StockpileId};
use crate::maintenance::maximum_usable_active_ticks;
use crate::material::CommodityKey;
use crate::production::ProcessId;
use crate::registry::Registries;

use super::{
    ThermalEquipmentEnvelopeError, ThermalPlanningCapabilities, integrated_energy,
    mass_capacity_from_energy, mass_capacity_from_integrated_power, resolve_phase_change_lot_offer,
    resolve_thermal_equipment_envelope,
};
use crate::thermal::phase_change_batch::{
    PurePhaseChangeBatchError, PurePhaseChangeDirection, resolve_phase_change_trace_material,
    resolve_phase_change_trace_physics,
};
use crate::thermal::{CastingResolutionError, calculate_phase_sensible_heat};

/// Current-state inputs for homogeneous-lot casting mass assessment.
#[derive(Clone, Copy, Debug)]
pub struct CastingLotMassRequest {
    process: ProcessId,
    source: StockpileId,
    selection: MaterialLotSelection,
    equipment: EquipmentId,
    energy_sink: EnergyStoreId,
}

impl CastingLotMassRequest {
    #[must_use]
    pub const fn new(
        process: ProcessId,
        source: StockpileId,
        selection: MaterialLotSelection,
        equipment: EquipmentId,
        energy_sink: EnergyStoreId,
    ) -> Self {
        Self {
            process,
            source,
            selection,
            equipment,
            energy_sink,
        }
    }
}

/// First canonical scale constraint preventing the full offered casting mass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CastingLotMassConstraint {
    EquipmentCapacity,
    TransferEnergyRange,
    ConditionLifetime,
    ThermalSinkCapacity,
}

/// Exact feasible mass range for casting one homogeneous pure-liquid offered lot profile.
///
/// Sink capacity is not assumed monotonic in mass. Longer casts can gain additional passive sink
/// recovery before completion, so `maximum_mass` is derived over exact transfer-duration buckets
/// using the same completion-before-passive-loss projection as canonical sink admission.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CastingLotMassEnvelope {
    offered_mass: Mass,
    equipment_capacity: Mass,
    transfer_energy_capacity: Mass,
    condition_lifetime_capacity: Mass,
    maximum_mass: Mass,
}

impl CastingLotMassEnvelope {
    #[must_use]
    pub const fn offered_mass(self) -> Mass {
        self.offered_mass
    }

    #[must_use]
    pub const fn equipment_capacity(self) -> Mass {
        self.equipment_capacity
    }

    #[must_use]
    pub const fn transfer_energy_capacity(self) -> Mass {
        self.transfer_energy_capacity
    }

    #[must_use]
    pub const fn condition_lifetime_capacity(self) -> Mass {
        self.condition_lifetime_capacity
    }

    /// Greatest exactly sink-feasible mass at or below the offered mass.
    #[must_use]
    pub const fn maximum_mass(self) -> Mass {
        self.maximum_mass
    }

    /// First canonical scale constraint that prevents casting the full offered mass.
    #[must_use]
    pub fn limiting_constraint(self) -> Option<CastingLotMassConstraint> {
        if self.offered_mass > self.equipment_capacity {
            Some(CastingLotMassConstraint::EquipmentCapacity)
        } else if self.offered_mass > self.transfer_energy_capacity {
            Some(CastingLotMassConstraint::TransferEnergyRange)
        } else if self.offered_mass > self.condition_lifetime_capacity {
            Some(CastingLotMassConstraint::ConditionLifetime)
        } else if self.maximum_mass < self.offered_mass {
            Some(CastingLotMassConstraint::ThermalSinkCapacity)
        } else {
            None
        }
    }
}

fn map_equipment_error(
    process: ProcessId,
    error: ThermalEquipmentEnvelopeError,
) -> CastingResolutionError {
    match error {
        ThermalEquipmentEnvelopeError::UnknownProcess => {
            CastingResolutionError::UnknownThermalProcess { process }
        }
        ThermalEquipmentEnvelopeError::Equipment(error) => CastingResolutionError::Equipment(error),
        ThermalEquipmentEnvelopeError::Capability(error) => {
            CastingResolutionError::Capability(error)
        }
        ThermalEquipmentEnvelopeError::MissingTransferPower { capability } => {
            CastingResolutionError::MissingCoolingPower { capability }
        }
        ThermalEquipmentEnvelopeError::MissingMaximumTemperature { capability } => {
            CastingResolutionError::MissingMaximumTemperature { capability }
        }
        ThermalEquipmentEnvelopeError::MissingMaximumBatchMass { capability } => {
            CastingResolutionError::MissingMaximumBatchMass { capability }
        }
    }
}

/// Assesses the greatest current mass that can be cast from one homogeneous offered liquid lot.
///
/// The returned maximum is exact even when passive sink recovery makes feasibility non-monotonic
/// across adjacent masses. No mutation or reservation occurs.
pub fn assess_casting_lot_mass_envelope(
    registries: &Registries,
    state: &AppState,
    request: CastingLotMassRequest,
) -> Result<CastingLotMassEnvelope, CastingResolutionError> {
    let definition = registries.thermal().get_casting(request.process).ok_or(
        CastingResolutionError::UnknownThermalProcess {
            process: request.process,
        },
    )?;
    let offer = resolve_phase_change_lot_offer(
        registries,
        state,
        request.process,
        request.source,
        request.selection,
    )
    .map_err(CastingResolutionError::Input)?;
    let equipment = resolve_thermal_equipment_envelope(
        registries,
        state,
        request.process,
        request.equipment,
        ThermalPlanningCapabilities::new(
            definition.cooling_power_capability(),
            definition.max_temperature_capability(),
            definition.max_batch_mass_capability(),
        ),
    )
    .map_err(|error| map_equipment_error(request.process, error))?;
    let material = resolve_phase_change_trace_material(
        registries.materials(),
        definition.material(),
        &[definition.liquid_form()],
        PurePhaseChangeDirection::Solidify,
        &offer.trace,
    )
    .map_err(CastingResolutionError::Batch)?;
    let phase = resolve_phase_change_trace_physics(
        registries.materials(),
        PurePhaseChangeDirection::Solidify,
        &offer.trace,
        Mass::from_milligrams(1),
        material,
    )
    .map_err(CastingResolutionError::Batch)?;
    let solid_cooling = calculate_phase_sensible_heat(
        registries.materials(),
        Mass::from_milligrams(1),
        CommodityKey::new(material, definition.solid_form()),
        offer.trace.profile().composition(),
        phase.melting_point,
        definition.output_temperature(),
    )
    .map_err(|error| {
        CastingResolutionError::Batch(PurePhaseChangeBatchError::SolidCooling { material, error })
    })?;
    let unit_energy = phase
        .transfer_energy
        .checked_add(solid_cooling.energy())
        .ok_or(CastingResolutionError::Batch(
            PurePhaseChangeBatchError::EnergyOverflow,
        ))?;
    if offer.trace.profile().temperature() > equipment.maximum_temperature {
        return Err(
            CastingResolutionError::InputTemperatureExceedsEquipmentMaximum {
                input: offer.trace.profile().temperature(),
                maximum: equipment.maximum_temperature,
            },
        );
    }
    let sink = validate_energy_sink_access(registries, state, request.energy_sink)
        .map_err(CastingResolutionError::EnergySink)?;
    if sink.carrier() != definition.energy_carrier() {
        return Err(CastingResolutionError::WrongEnergyCarrier {
            required: definition.energy_carrier(),
            provided: sink.carrier(),
        });
    }
    let transfer_power = equipment.transfer_power.min(sink.max_input_power());
    let transfer_energy_capacity =
        mass_capacity_from_energy(Energy::from_nanojoules(u128::MAX), unit_energy);
    let lifetime_ticks = maximum_usable_active_ticks(
        definition.condition_wear_ppm_per_active_tick(),
        equipment.condition,
    );
    let condition_lifetime_capacity = mass_capacity_from_integrated_power(
        transfer_power,
        lifetime_ticks,
        registries,
        unit_energy,
    );
    let upper_mass = [
        offer.mass,
        equipment.batch_mass_capacity,
        transfer_energy_capacity,
        condition_lifetime_capacity,
    ]
    .into_iter()
    .min()
    .unwrap_or(Mass::ZERO);
    let maximum_mass =
        maximum_sink_feasible_mass(registries, sink, transfer_power, unit_energy, upper_mass)
            .map_err(CastingResolutionError::Duration)?;
    Ok(CastingLotMassEnvelope {
        offered_mass: offer.mass,
        equipment_capacity: equipment.batch_mass_capacity,
        transfer_energy_capacity,
        condition_lifetime_capacity,
        maximum_mass,
    })
}

fn maximum_sink_feasible_mass(
    registries: &Registries,
    sink: ValidatedEnergySinkAccess,
    transfer_power: Power,
    unit_energy: Energy,
    upper_mass: Mass,
) -> Result<Mass, crate::energy::PowerDurationError> {
    if upper_mass.is_zero() {
        return Ok(Mass::ZERO);
    }
    let upper_energy = energy_for_mass(unit_energy, upper_mass);
    let maximum_duration = calculate_power_duration_ceiling(
        transfer_power,
        upper_energy,
        registries.core().physical_tick_duration(),
    )?;
    let mut duration = maximum_duration;
    loop {
        let transfer_capacity = integrated_energy(transfer_power, duration, registries);
        let previous_transfer_capacity = integrated_energy(
            transfer_power,
            TickSpan::new(duration.value() - 1),
            registries,
        );
        let minimum_mass = Mass::from_milligrams(
            mass_capacity_from_energy(previous_transfer_capacity, unit_energy)
                .milligrams()
                .checked_add(1)
                .unwrap_or_else(|| {
                    unreachable!(
                        "minimum transfer duration cannot begin above maximum represented mass"
                    )
                }),
        );
        let transfer_mass = mass_capacity_from_energy(transfer_capacity, unit_energy);
        let sink_mass = mass_capacity_from_energy(
            sink.available_capacity_at_release(registries, duration),
            unit_energy,
        );
        let candidate = [upper_mass, transfer_mass, sink_mass]
            .into_iter()
            .min()
            .unwrap_or(Mass::ZERO);
        if candidate >= minimum_mass {
            return Ok(candidate);
        }
        if candidate.is_zero() {
            return Ok(Mass::ZERO);
        }

        // Every component of `candidate` is nondecreasing with duration. Because this duration's
        // bucket starts above `candidate`, no skipped later-than-candidate duration can contain a
        // feasible mass: its transfer bucket also starts above `candidate` while its sink-limited
        // candidate cannot exceed the one just observed. Jump directly to the least duration that
        // can transfer this candidate and continue the exact backward search there.
        let candidate_duration = calculate_power_duration_ceiling(
            transfer_power,
            energy_for_mass(unit_energy, candidate),
            registries.core().physical_tick_duration(),
        )?;
        debug_assert!(candidate_duration < duration);
        duration = candidate_duration;
    }
}

fn energy_for_mass(unit_energy: Energy, mass: Mass) -> Energy {
    Energy::from_nanojoules(
        unit_energy
            .nanojoules()
            .checked_mul(u128::from(mass.milligrams()))
            .unwrap_or_else(|| unreachable!("thermal planning mass was bounded by energy range")),
    )
}
