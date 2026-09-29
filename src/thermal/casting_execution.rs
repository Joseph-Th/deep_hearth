//! Pure-material casting/solidification with exact heat release into a finite thermal-energy sink.

use crate::core::quantity::{Energy, Power, Temperature};
use crate::core::state::AppState;
use crate::energy::{EnergyStoreId, validate_energy_sink_access, validate_energy_sink_release};
use crate::equipment::EquipmentId;
use crate::inventory::MaterialLotSelection;
use crate::inventory::StockpileId;
use crate::material::{CommodityKey, FormId, MaterialComposition, MaterialId, MaterialLotSpec};
use crate::production::{
    ProcessId, ProcessOutputStream, ProcessOutputStreamId, ProcessResolution,
    validate_process_inputs,
};
use crate::registry::Registries;

use super::calculate_phase_sensible_heat;
use super::equipment_physics::{
    ThermalEquipmentRequest, ThermalEquipmentSetupError, ThermalTransferTimingError,
    resolve_runtime_thermal_equipment, resolve_thermal_transfer_timing,
};
use super::phase_change_batch::{
    PurePhaseChangeBatchError, PurePhaseChangeDirection, resolve_pure_phase_change_batch,
};
use super::processes::CastingProcessDefinition;
#[cfg(test)]
use super::{calculate_fusion_heat, calculate_sensible_heat};

/// Failure while deriving solidification physics from exact consumed liquid traces.
pub type CastingBatchError = PurePhaseChangeBatchError;

fn map_thermal_equipment_error(error: ThermalEquipmentSetupError) -> CastingResolutionError {
    match error {
        ThermalEquipmentSetupError::Equipment(error) => CastingResolutionError::Equipment(error),
        ThermalEquipmentSetupError::Capability(error) => CastingResolutionError::Capability(error),
        ThermalEquipmentSetupError::MissingTransferPower { capability } => {
            CastingResolutionError::MissingCoolingPower { capability }
        }
        ThermalEquipmentSetupError::MissingMaximumTemperature { capability } => {
            CastingResolutionError::MissingMaximumTemperature { capability }
        }
        ThermalEquipmentSetupError::MissingMaximumBatchMass { capability } => {
            CastingResolutionError::MissingMaximumBatchMass { capability }
        }
        ThermalEquipmentSetupError::BatchMassExceeded { selected, maximum } => {
            CastingResolutionError::BatchMassExceedsEquipmentCapacity { selected, maximum }
        }
    }
}

fn resolve_casting_batch(
    materials: &crate::material::MaterialRegistry,
    material: MaterialId,
    liquid_form: FormId,
    solid_form: FormId,
    output_temperature: Temperature,
    traces: &[crate::inventory::ConsumedMaterialTrace],
) -> Result<super::phase_change_batch::PurePhaseChangeBatch, CastingBatchError> {
    let mut batch = resolve_pure_phase_change_batch(
        materials,
        material,
        &[liquid_form],
        solid_form,
        PurePhaseChangeDirection::Solidify,
        traces,
    )?;
    let solid_cooling = calculate_phase_sensible_heat(
        materials,
        batch.output.mass(),
        CommodityKey::new(batch.material, solid_form),
        batch.output.composition(),
        batch.melting_point,
        output_temperature,
    )
    .map_err(|error| PurePhaseChangeBatchError::SolidCooling {
        material: batch.material,
        error,
    })?;
    batch.transfer_energy = batch
        .transfer_energy
        .checked_add(solid_cooling.energy())
        .ok_or(PurePhaseChangeBatchError::EnergyOverflow)?;
    batch.output = MaterialLotSpec::with_composition(
        CommodityKey::new(batch.material, solid_form),
        batch.output.mass(),
        output_temperature,
        MaterialComposition::pure(batch.material),
    )
    .map_err(PurePhaseChangeBatchError::Output)?;
    Ok(batch)
}

/// Exact runtime selection, cooling equipment, and finite heat sink for one casting operation.
#[derive(Clone, Copy, Debug)]
pub struct CastingRequest<'selection> {
    process: ProcessId,
    source: StockpileId,
    selections: &'selection [MaterialLotSelection],
    equipment: EquipmentId,
    energy_sink: EnergyStoreId,
}

impl<'selection> CastingRequest<'selection> {
    #[must_use]
    pub const fn new(
        process: ProcessId,
        source: StockpileId,
        selections: &'selection [MaterialLotSelection],
        equipment: EquipmentId,
        energy_sink: EnergyStoreId,
    ) -> Self {
        Self {
            process,
            source,
            selections,
            equipment,
            energy_sink,
        }
    }
}

/// Observable physically resolved casting operation before production start.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct ResolvedCasting {
    resolution: ProcessResolution,
    equipment: EquipmentId,
    material: MaterialId,
    melting_point: Temperature,
    released_energy: Energy,
    transfer_power: Power,
}

impl ResolvedCasting {
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
    pub const fn released_energy(&self) -> Energy {
        self.released_energy
    }

    #[must_use]
    pub const fn transfer_power(&self) -> Power {
        self.transfer_power
    }
}

/// Resolves exact sensible plus latent heat release, cooling limits, sink capacity, and solid output.
pub fn resolve_casting_process(
    registries: &Registries,
    state: &AppState,
    request: CastingRequest<'_>,
) -> Result<ResolvedCasting, CastingResolutionError> {
    let CastingRequest {
        process,
        source,
        selections,
        equipment,
        energy_sink,
    } = request;
    let definition = registries
        .thermal()
        .get_casting(process)
        .ok_or(CastingResolutionError::UnknownThermalProcess { process })?;
    let inputs = validate_process_inputs(state, process, source, selections)
        .map_err(CastingResolutionError::Input)?;
    let thermal_equipment = resolve_runtime_thermal_equipment(
        registries,
        state,
        ThermalEquipmentRequest::new(
            process,
            equipment,
            definition.cooling_power_capability(),
            definition.max_temperature_capability(),
            definition.max_batch_mass_capability(),
            inputs.input_mass(),
        ),
    )
    .map_err(map_thermal_equipment_error)?;
    let provider = thermal_equipment.provider();
    let equipment_use = thermal_equipment.equipment_use();
    let limits = thermal_equipment.limits();

    let batch = resolve_casting_batch(
        registries.materials(),
        definition.material(),
        definition.liquid_form(),
        definition.solid_form(),
        definition.output_temperature(),
        inputs.consumed_inputs(),
    )
    .map_err(CastingResolutionError::Batch)?;
    if batch.hottest_input > limits.maximum_temperature() {
        return Err(
            CastingResolutionError::InputTemperatureExceedsEquipmentMaximum {
                input: batch.hottest_input,
                maximum: limits.maximum_temperature(),
            },
        );
    }
    let energy_sink_access = validate_energy_sink_access(registries, state, energy_sink)
        .map_err(CastingResolutionError::EnergySink)?;
    let provided_carrier = energy_sink_access.carrier();
    if provided_carrier != definition.energy_carrier() {
        return Err(CastingResolutionError::WrongEnergyCarrier {
            required: definition.energy_carrier(),
            provided: provided_carrier,
        });
    }
    let timing = resolve_thermal_transfer_timing(
        registries,
        limits.transfer_power(),
        energy_sink_access.max_input_power(),
        batch.transfer_energy,
        definition.condition_wear_ppm_per_active_tick(),
        provider.condition(),
    )
    .map_err(|error| match error {
        ThermalTransferTimingError::Duration(error) => CastingResolutionError::Duration(error),
        ThermalTransferTimingError::ConditionDuration(error) => {
            CastingResolutionError::ConditionDuration(error)
        }
    })?;
    let transfer_power = timing.transfer_power();
    let duration = timing.duration();
    let equipment_condition_after = timing.condition_after();
    let energy_sink = validate_energy_sink_release(
        registries,
        energy_sink_access,
        batch.transfer_energy,
        duration,
    )
    .map_err(CastingResolutionError::EnergySink)?;
    let resolution = inputs
        .resolve_with_equipment_and_energy_release(
            duration,
            vec![ProcessOutputStream::new(
                ProcessOutputStreamId::PRIMARY,
                vec![batch.output],
            )],
            energy_sink,
            equipment_use,
            equipment_condition_after,
        )
        .map_err(CastingResolutionError::Resolution)?;
    Ok(ResolvedCasting {
        resolution,
        equipment,
        material: batch.material,
        melting_point: batch.melting_point,
        released_energy: batch.transfer_energy,
        transfer_power,
    })
}

mod errors;
mod validation;

pub use errors::{CastingJobValidationError, CastingResolutionError};
pub(super) use validation::validate_loaded_casting_job;

#[cfg(test)]
#[path = "casting_execution_tests.rs"]
mod tests;
