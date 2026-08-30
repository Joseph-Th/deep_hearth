//! Resolves and replays pure-material melting operations.

use crate::capability::{CapabilityId, evaluate_capabilities};
use crate::core::quantity::{Energy, Power, Temperature};
use crate::core::state::AppState;
use crate::energy::{EnergyCarrier, EnergyStoreId, validate_energy_supply};
use crate::equipment::{EquipmentId, resolve_equipment_provider};
use crate::inventory::MaterialLotSelection;
use crate::inventory::StockpileId;
use crate::material::{FormId, MaterialId};
use crate::production::{
    ProcessId, ProcessOutputStream, ProcessOutputStreamId, ProcessResolution,
    validate_selected_process_inputs,
};
use crate::registry::Registries;

use super::PhaseChangeProcessProfile;
use super::equipment_physics::{
    ThermalBatchLimitError, ThermalPowerTemperatureError, ThermalTransferTimingError,
    resolve_thermal_power_temperature_limits, resolve_thermal_transfer_timing,
    validate_thermal_batch_mass,
};
use super::phase_change_batch::{
    PurePhaseChangeBatchError, PurePhaseChangeDirection, resolve_pure_phase_change_batch,
};
#[cfg(test)]
use super::{calculate_fusion_heat, calculate_sensible_heat};
#[cfg(test)]
use crate::material::{CommodityKey, MaterialPhase};

/// Immutable declaration that one selected-batch process performs pure-material melting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MeltingProcessDefinition {
    process: ProcessId,
    profile: PhaseChangeProcessProfile,
    material: MaterialId,
    solid_forms: Vec<FormId>,
    liquid_form: FormId,
}

impl MeltingProcessDefinition {
    #[must_use]
    pub fn new(
        process: ProcessId,
        profile: PhaseChangeProcessProfile,
        material: MaterialId,
        solid_forms: Vec<FormId>,
        liquid_form: FormId,
    ) -> Self {
        assert!(
            !solid_forms.is_empty(),
            "melting process must accept at least one solid input form"
        );
        assert!(
            solid_forms.windows(2).all(|pair| pair[0] < pair[1]),
            "melting input forms must be strictly ordered and unique"
        );
        Self {
            process,
            profile,
            material,
            solid_forms,
            liquid_form,
        }
    }

    #[must_use]
    pub const fn process(&self) -> ProcessId {
        self.process
    }

    #[must_use]
    pub const fn heating_power_capability(&self) -> CapabilityId {
        self.profile.transfer_power_capability()
    }

    #[must_use]
    pub const fn max_temperature_capability(&self) -> CapabilityId {
        self.profile.max_temperature_capability()
    }

    #[must_use]
    pub const fn max_batch_mass_capability(&self) -> CapabilityId {
        self.profile.max_batch_mass_capability()
    }

    #[must_use]
    pub const fn energy_carrier(&self) -> EnergyCarrier {
        self.profile.energy_carrier()
    }

    #[must_use]
    pub const fn material(&self) -> MaterialId {
        self.material
    }

    #[must_use]
    pub fn solid_forms(&self) -> &[FormId] {
        &self.solid_forms
    }

    #[must_use]
    pub const fn liquid_form(&self) -> FormId {
        self.liquid_form
    }

    #[must_use]
    pub const fn condition_wear_ppm_per_active_tick(&self) -> u32 {
        self.profile.condition_wear_ppm_per_active_tick()
    }
}

/// Failure while deriving pure melting physics from exact consumed material traces.
pub type MeltingBatchError = PurePhaseChangeBatchError;

fn resolve_melting_batch(
    materials: &crate::material::MaterialRegistry,
    definition: &MeltingProcessDefinition,
    traces: &[crate::inventory::ConsumedMaterialTrace],
) -> Result<super::phase_change_batch::PurePhaseChangeBatch, MeltingBatchError> {
    resolve_pure_phase_change_batch(
        materials,
        definition.material(),
        definition.solid_forms(),
        definition.liquid_form(),
        PurePhaseChangeDirection::Melt,
        traces,
    )
}

/// Exact runtime selection and providers requested for one melting operation.
#[derive(Clone, Copy, Debug)]
pub struct MeltingRequest<'selection> {
    process: ProcessId,
    source: StockpileId,
    selections: &'selection [MaterialLotSelection],
    equipment: EquipmentId,
    energy_store: EnergyStoreId,
}

impl<'selection> MeltingRequest<'selection> {
    #[must_use]
    pub const fn new(
        process: ProcessId,
        source: StockpileId,
        selections: &'selection [MaterialLotSelection],
        equipment: EquipmentId,
        energy_store: EnergyStoreId,
    ) -> Self {
        Self {
            process,
            source,
            selections,
            equipment,
            energy_store,
        }
    }
}

/// Observable physically resolved melting operation before production start.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedMelting {
    resolution: ProcessResolution,
    equipment: EquipmentId,
    material: MaterialId,
    melting_point: Temperature,
    required_energy: Energy,
    transfer_power: Power,
}

impl ResolvedMelting {
    pub const fn process_resolution(&self) -> &ProcessResolution {
        &self.resolution
    }

    #[must_use]
    pub const fn equipment(&self) -> EquipmentId {
        self.equipment
    }

    #[must_use]
    pub const fn material(&self) -> MaterialId {
        self.material
    }

    #[must_use]
    pub const fn melting_point(&self) -> Temperature {
        self.melting_point
    }

    #[must_use]
    pub const fn required_energy(&self) -> Energy {
        self.required_energy
    }

    #[must_use]
    pub const fn transfer_power(&self) -> Power {
        self.transfer_power
    }
}

/// Resolves exact sensible plus latent heat, equipment limits, energy supply, and molten output.
pub fn resolve_melting_process(
    registries: &Registries,
    state: &AppState,
    request: MeltingRequest<'_>,
) -> Result<ResolvedMelting, MeltingResolutionError> {
    let MeltingRequest {
        process,
        source,
        selections,
        equipment,
        energy_store,
    } = request;
    let definition = registries
        .thermal()
        .get_melting(process)
        .ok_or(MeltingResolutionError::UnknownThermalProcess { process })?;
    let inputs = validate_selected_process_inputs(registries, state, process, source, selections)
        .map_err(MeltingResolutionError::Input)?;
    let provider = resolve_equipment_provider(registries, state, equipment)
        .map_err(MeltingResolutionError::Equipment)?;
    let equipment_use = provider.validated_use();
    let process_definition = match registries.production().get_process(process) {
        Some(process_definition) => process_definition,
        None => return Err(MeltingResolutionError::UnknownThermalProcess { process }),
    };
    evaluate_capabilities(
        registries.capabilities(),
        &provider,
        process_definition.capability_requirements(),
    )
    .map_err(MeltingResolutionError::Capability)?;

    let limits = resolve_thermal_power_temperature_limits(
        provider.definition(),
        provider.condition(),
        definition.heating_power_capability(),
        definition.max_temperature_capability(),
    )
    .map_err(|error| match error {
        ThermalPowerTemperatureError::MissingTransferPower => {
            MeltingResolutionError::MissingHeatingPower {
                capability: definition.heating_power_capability(),
            }
        }
        ThermalPowerTemperatureError::MissingMaximumTemperature => {
            MeltingResolutionError::MissingMaximumTemperature {
                capability: definition.max_temperature_capability(),
            }
        }
    })?;
    validate_thermal_batch_mass(
        provider.definition(),
        provider.condition(),
        definition.max_batch_mass_capability(),
        inputs.input_mass(),
    )
    .map_err(|error| match error {
        ThermalBatchLimitError::MissingMaximumBatchMass => {
            MeltingResolutionError::MissingMaximumBatchMass {
                capability: definition.max_batch_mass_capability(),
            }
        }
        ThermalBatchLimitError::BatchMassExceeded { selected, maximum } => {
            MeltingResolutionError::BatchMassExceedsEquipmentCapacity { selected, maximum }
        }
    })?;

    let batch = resolve_melting_batch(registries.materials(), definition, inputs.consumed_inputs())
        .map_err(MeltingResolutionError::Batch)?;
    if batch.melting_point > limits.maximum_temperature() {
        return Err(
            MeltingResolutionError::MeltingPointExceedsEquipmentMaximum {
                melting_point: batch.melting_point,
                maximum: limits.maximum_temperature(),
            },
        );
    }
    let energy_supply =
        validate_energy_supply(registries, state, energy_store, batch.transfer_energy)
            .map_err(MeltingResolutionError::Energy)?;
    let provided_carrier = energy_supply.trace().carrier();
    if provided_carrier != definition.energy_carrier() {
        return Err(MeltingResolutionError::WrongEnergyCarrier {
            required: definition.energy_carrier(),
            provided: provided_carrier,
        });
    }
    let timing = resolve_thermal_transfer_timing(
        registries,
        limits.transfer_power(),
        energy_supply.max_output_power(),
        batch.transfer_energy,
        definition.condition_wear_ppm_per_active_tick(),
        provider.condition(),
    )
    .map_err(|error| match error {
        ThermalTransferTimingError::Duration(error) => MeltingResolutionError::Duration(error),
        ThermalTransferTimingError::ConditionDuration(error) => {
            MeltingResolutionError::ConditionDuration(error)
        }
    })?;
    let transfer_power = timing.transfer_power();
    let duration = timing.duration();
    let equipment_condition_after = timing.condition_after();
    let resolution = inputs
        .resolve_with_energy_and_equipment(
            duration,
            vec![ProcessOutputStream::new(
                ProcessOutputStreamId::PRIMARY,
                vec![batch.output],
            )],
            energy_supply,
            equipment_use,
            equipment_condition_after,
        )
        .map_err(MeltingResolutionError::Resolution)?;
    Ok(ResolvedMelting {
        resolution,
        equipment,
        material: batch.material,
        melting_point: batch.melting_point,
        required_energy: batch.transfer_energy,
        transfer_power,
    })
}

mod errors;
mod validation;

pub use errors::{MeltingJobValidationError, MeltingResolutionError};
pub(super) use validation::validate_loaded_melting_job;

#[cfg(test)]
#[path = "melting_execution_tests.rs"]
mod tests;
