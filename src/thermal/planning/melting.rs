//! Current-state homogeneous-lot melting mass projection.

use crate::core::quantity::{Energy, Mass};
use crate::core::state::AppState;
use crate::energy::{EnergyStoreId, assess_energy_supply_access};
use crate::equipment::EquipmentId;
use crate::inventory::{MaterialLotSelection, StockpileId};
use crate::maintenance::maximum_usable_active_ticks;
use crate::production::ProcessId;
use crate::registry::Registries;

use super::{
    ThermalEquipmentEnvelopeError, ThermalPlanningCapabilities, mass_capacity_from_energy,
    mass_capacity_from_integrated_power, resolve_phase_change_lot_offer,
    resolve_thermal_equipment_envelope,
};
use crate::thermal::MeltingResolutionError;
use crate::thermal::phase_change_batch::{
    PurePhaseChangeDirection, resolve_phase_change_trace_material,
    resolve_phase_change_trace_physics,
};

/// Current-state inputs for homogeneous-lot melting mass assessment.
#[derive(Clone, Copy, Debug)]
pub struct MeltingLotMassRequest {
    process: ProcessId,
    source: StockpileId,
    selection: MaterialLotSelection,
    equipment: EquipmentId,
    energy_store: EnergyStoreId,
}

impl MeltingLotMassRequest {
    #[must_use]
    pub const fn new(
        process: ProcessId,
        source: StockpileId,
        selection: MaterialLotSelection,
        equipment: EquipmentId,
        energy_store: EnergyStoreId,
    ) -> Self {
        Self {
            process,
            source,
            selection,
            equipment,
            energy_store,
        }
    }
}

/// First canonical scale constraint preventing the full offered melting mass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeltingLotMassConstraint {
    EquipmentCapacity,
    TransferEnergyRange,
    FiniteEnergy,
    ConditionLifetime,
}

/// Exact feasible mass range for melting one homogeneous offered lot profile.
///
/// The offer is validated against current inventory, equipment, and energy owners. The result is
/// disposable planning evidence rather than authorization; the exact melting resolver must still
/// bind the selected mass before consequential work starts.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeltingLotMassEnvelope {
    offered_mass: Mass,
    equipment_capacity: Mass,
    transfer_energy_capacity: Mass,
    finite_energy_capacity: Mass,
    condition_lifetime_capacity: Mass,
}

impl MeltingLotMassEnvelope {
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
    pub const fn finite_energy_capacity(self) -> Mass {
        self.finite_energy_capacity
    }

    #[must_use]
    pub const fn condition_lifetime_capacity(self) -> Mass {
        self.condition_lifetime_capacity
    }

    /// Greatest mass at or below the offer admitted by all monotonic melting scale constraints.
    #[must_use]
    pub fn maximum_mass(self) -> Mass {
        [
            self.offered_mass,
            self.equipment_capacity,
            self.transfer_energy_capacity,
            self.finite_energy_capacity,
            self.condition_lifetime_capacity,
        ]
        .into_iter()
        .min()
        .unwrap_or(Mass::ZERO)
    }

    /// First canonical scale constraint that prevents melting the full offered mass.
    #[must_use]
    pub fn limiting_constraint(self) -> Option<MeltingLotMassConstraint> {
        if self.offered_mass > self.equipment_capacity {
            Some(MeltingLotMassConstraint::EquipmentCapacity)
        } else if self.offered_mass > self.transfer_energy_capacity {
            Some(MeltingLotMassConstraint::TransferEnergyRange)
        } else if self.offered_mass > self.finite_energy_capacity {
            Some(MeltingLotMassConstraint::FiniteEnergy)
        } else if self.offered_mass > self.condition_lifetime_capacity {
            Some(MeltingLotMassConstraint::ConditionLifetime)
        } else {
            None
        }
    }
}

fn map_equipment_error(
    process: ProcessId,
    error: ThermalEquipmentEnvelopeError,
) -> MeltingResolutionError {
    match error {
        ThermalEquipmentEnvelopeError::UnknownProcess => {
            MeltingResolutionError::UnknownThermalProcess { process }
        }
        ThermalEquipmentEnvelopeError::Equipment(error) => MeltingResolutionError::Equipment(error),
        ThermalEquipmentEnvelopeError::Capability(error) => {
            MeltingResolutionError::Capability(error)
        }
        ThermalEquipmentEnvelopeError::MissingTransferPower { capability } => {
            MeltingResolutionError::MissingHeatingPower { capability }
        }
        ThermalEquipmentEnvelopeError::MissingMaximumTemperature { capability } => {
            MeltingResolutionError::MissingMaximumTemperature { capability }
        }
        ThermalEquipmentEnvelopeError::MissingMaximumBatchMass { capability } => {
            MeltingResolutionError::MissingMaximumBatchMass { capability }
        }
    }
}

/// Assesses the greatest current mass that can be melted from one homogeneous offered lot.
pub fn assess_melting_lot_mass_envelope(
    registries: &Registries,
    state: &AppState,
    request: MeltingLotMassRequest,
) -> Result<MeltingLotMassEnvelope, MeltingResolutionError> {
    let definition = registries.thermal().get_melting(request.process).ok_or(
        MeltingResolutionError::UnknownThermalProcess {
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
    .map_err(MeltingResolutionError::Input)?;
    let equipment = resolve_thermal_equipment_envelope(
        registries,
        state,
        request.process,
        request.equipment,
        ThermalPlanningCapabilities::new(
            definition.heating_power_capability(),
            definition.max_temperature_capability(),
            definition.max_batch_mass_capability(),
        ),
    )
    .map_err(|error| map_equipment_error(request.process, error))?;
    let material = resolve_phase_change_trace_material(
        registries.materials(),
        definition.material(),
        definition.solid_forms(),
        PurePhaseChangeDirection::Melt,
        &offer.trace,
    )
    .map_err(MeltingResolutionError::Batch)?;
    let unit = resolve_phase_change_trace_physics(
        registries.materials(),
        PurePhaseChangeDirection::Melt,
        &offer.trace,
        Mass::from_milligrams(1),
        material,
    )
    .map_err(MeltingResolutionError::Batch)?;
    if unit.melting_point > equipment.maximum_temperature {
        return Err(
            MeltingResolutionError::MeltingPointExceedsEquipmentMaximum {
                melting_point: unit.melting_point,
                maximum: equipment.maximum_temperature,
            },
        );
    }
    let energy = assess_energy_supply_access(registries, state, request.energy_store)
        .map_err(MeltingResolutionError::Energy)?;
    if energy.carrier() != definition.energy_carrier() {
        return Err(MeltingResolutionError::WrongEnergyCarrier {
            required: definition.energy_carrier(),
            provided: energy.carrier(),
        });
    }
    let transfer_power = equipment.transfer_power.min(energy.max_output_power());
    let transfer_energy_capacity =
        mass_capacity_from_energy(Energy::from_nanojoules(u128::MAX), unit.transfer_energy);
    let finite_energy_capacity =
        mass_capacity_from_energy(energy.available(), unit.transfer_energy);
    let lifetime_ticks = maximum_usable_active_ticks(
        definition.condition_wear_ppm_per_active_tick(),
        equipment.condition,
    );
    let condition_lifetime_capacity = mass_capacity_from_integrated_power(
        transfer_power,
        lifetime_ticks,
        registries,
        unit.transfer_energy,
    );
    Ok(MeltingLotMassEnvelope {
        offered_mass: offer.mass,
        equipment_capacity: equipment.batch_mass_capacity,
        transfer_energy_capacity,
        finite_energy_capacity,
        condition_lifetime_capacity,
    })
}
