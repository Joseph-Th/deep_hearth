//! Pure-material casting/solidification with exact heat release into a finite thermal-energy sink.

use crate::capability::{CapabilityId, evaluate_capabilities};
use crate::core::quantity::{Energy, Power, Temperature};
use crate::core::state::AppState;
use crate::energy::{
    EnergyCarrier, EnergyStoreId, validate_energy_sink_access, validate_energy_sink_release,
};
use crate::equipment::{EquipmentId, resolve_equipment_provider};
use crate::inventory::MaterialLotSelection;
use crate::inventory::StockpileId;
use crate::material::{CommodityKey, FormId, MaterialComposition, MaterialId, MaterialLotSpec};
use crate::production::{
    ProcessId, ProcessOutputStream, ProcessOutputStreamId, ProcessResolution,
    validate_selected_process_inputs,
};
use crate::registry::Registries;

use super::equipment_physics::{
    ThermalBatchLimitError, ThermalPowerTemperatureError, ThermalTransferTimingError,
    resolve_thermal_power_temperature_limits, resolve_thermal_transfer_timing,
    validate_thermal_batch_mass,
};
use super::phase_change_batch::{
    PurePhaseChangeBatchError, PurePhaseChangeDirection, resolve_pure_phase_change_batch,
};
use super::{PhaseChangeForms, PhaseChangeProcessProfile, calculate_phase_sensible_heat};
#[cfg(test)]
use super::{calculate_fusion_heat, calculate_sensible_heat};

/// Immutable declaration that one selected-batch process solidifies pure liquid matter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CastingPhaseChange {
    forms: PhaseChangeForms,
    output_temperature: Temperature,
}

impl CastingPhaseChange {
    #[must_use]
    pub const fn new(forms: PhaseChangeForms, output_temperature: Temperature) -> Self {
        assert!(
            output_temperature.millikelvin() > 0,
            "casting output temperature must be above absolute zero"
        );
        Self {
            forms,
            output_temperature,
        }
    }

    #[must_use]
    pub const fn liquid_form(self) -> FormId {
        self.forms.input()
    }

    #[must_use]
    pub const fn solid_form(self) -> FormId {
        self.forms.output()
    }

    #[must_use]
    pub const fn output_temperature(self) -> Temperature {
        self.output_temperature
    }
}

/// Immutable declaration that one selected-batch process solidifies pure liquid matter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CastingProcessDefinition {
    process: ProcessId,
    profile: PhaseChangeProcessProfile,
    material: MaterialId,
    phase_change: CastingPhaseChange,
}

impl CastingProcessDefinition {
    #[must_use]
    pub const fn new(
        process: ProcessId,
        profile: PhaseChangeProcessProfile,
        material: MaterialId,
        phase_change: CastingPhaseChange,
    ) -> Self {
        Self {
            process,
            profile,
            material,
            phase_change,
        }
    }

    #[must_use]
    pub const fn process(self) -> ProcessId {
        self.process
    }

    #[must_use]
    pub const fn cooling_power_capability(self) -> CapabilityId {
        self.profile.transfer_power_capability()
    }

    #[must_use]
    pub const fn max_temperature_capability(self) -> CapabilityId {
        self.profile.max_temperature_capability()
    }

    #[must_use]
    pub const fn max_batch_mass_capability(self) -> CapabilityId {
        self.profile.max_batch_mass_capability()
    }

    #[must_use]
    pub const fn energy_carrier(self) -> EnergyCarrier {
        self.profile.energy_carrier()
    }

    #[must_use]
    pub const fn material(self) -> MaterialId {
        self.material
    }

    #[must_use]
    pub const fn liquid_form(self) -> FormId {
        self.phase_change.liquid_form()
    }

    #[must_use]
    pub const fn solid_form(self) -> FormId {
        self.phase_change.solid_form()
    }

    /// Temperature of the solid lot after the casting cycle removes latent and sensible heat.
    #[must_use]
    pub const fn output_temperature(self) -> Temperature {
        self.phase_change.output_temperature()
    }

    #[must_use]
    pub const fn condition_wear_ppm_per_active_tick(self) -> u32 {
        self.profile.condition_wear_ppm_per_active_tick()
    }
}

/// Failure while deriving solidification physics from exact consumed liquid traces.
pub type CastingBatchError = PurePhaseChangeBatchError;

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
#[derive(Clone, Debug, PartialEq, Eq)]
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
    let inputs = validate_selected_process_inputs(registries, state, process, source, selections)
        .map_err(CastingResolutionError::Input)?;
    let provider = resolve_equipment_provider(registries, state, equipment)
        .map_err(CastingResolutionError::Equipment)?;
    let equipment_use = provider.validated_use();
    let process_definition = match registries.production().get_process(process) {
        Some(process_definition) => process_definition,
        None => return Err(CastingResolutionError::UnknownThermalProcess { process }),
    };
    evaluate_capabilities(
        registries.capabilities(),
        &provider,
        process_definition.capability_requirements(),
    )
    .map_err(CastingResolutionError::Capability)?;

    let limits = resolve_thermal_power_temperature_limits(
        provider.definition(),
        provider.condition(),
        definition.cooling_power_capability(),
        definition.max_temperature_capability(),
    )
    .map_err(|error| match error {
        ThermalPowerTemperatureError::MissingTransferPower => {
            CastingResolutionError::MissingCoolingPower {
                capability: definition.cooling_power_capability(),
            }
        }
        ThermalPowerTemperatureError::MissingMaximumTemperature => {
            CastingResolutionError::MissingMaximumTemperature {
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
            CastingResolutionError::MissingMaximumBatchMass {
                capability: definition.max_batch_mass_capability(),
            }
        }
        ThermalBatchLimitError::BatchMassExceeded { selected, maximum } => {
            CastingResolutionError::BatchMassExceedsEquipmentCapacity { selected, maximum }
        }
    })?;

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
