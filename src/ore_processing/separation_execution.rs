//! Exact manual and powered constituent-separation resolution for authored liberated feed.

use crate::core::quantity::{Energy, Mass, MassFlow, Power};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::energy::EnergyStoreId;
use crate::equipment::EquipmentId;
use crate::inventory::{MaterialLotSelection, StockpileId};
use crate::maintenance::Condition;
use crate::production::{
    ProcessId, ProcessOutputStream, ProcessResolution, validate_process_inputs,
};
use crate::registry::Registries;

use super::ConstituentSeparationProcessDefinition;
use super::powered_physics::{
    PoweredOreBottleneck, classify_powered_ore_bottleneck, resolve_powered_ore_provider,
    resolve_powered_ore_supply,
};

mod errors;
mod manual;
mod outputs;
mod validation;

pub use errors::{ConstituentSeparationBatchError, ConstituentSeparationResolutionError};
pub use manual::{
    ManualConstituentSeparationCommitError, ManualConstituentSeparationRequest,
    ManualConstituentSeparationResolutionError, ResolvedManualConstituentSeparation,
    StartManualConstituentSeparationError, ValidatedManualConstituentSeparationStart,
    resolve_manual_constituent_separation_process, validate_start_manual_constituent_separation,
};
use outputs::resolve_separation_outputs;
pub use validation::ConstituentSeparationJobValidationError;
pub(crate) use validation::validate_loaded_constituent_separation_job;

/// Runtime request to separate one explicitly selected liberated particulate batch.
#[derive(Clone, Copy, Debug)]
pub struct ConstituentSeparationRequest<'selection> {
    process: ProcessId,
    source: StockpileId,
    selections: &'selection [MaterialLotSelection],
    equipment: EquipmentId,
    energy_store: EnergyStoreId,
}

impl<'selection> ConstituentSeparationRequest<'selection> {
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

/// Fully resolved constituent separation ready for routed production start.
#[must_use]
#[derive(Debug)]
pub struct ResolvedConstituentSeparation {
    resolution: ProcessResolution,
    equipment: EquipmentId,
    condition_before: Condition,
    condition_after: Condition,
    processing_rate: MassFlow,
    required_energy: Energy,
    available_power: Power,
    throughput_duration: TickSpan,
    energy_duration: TickSpan,
    target_mass: Mass,
    residue_mass: Mass,
}

impl ResolvedConstituentSeparation {
    pub const fn process_resolution(&self) -> &ProcessResolution {
        &self.resolution
    }

    #[must_use]
    pub const fn equipment(&self) -> EquipmentId {
        self.equipment
    }

    #[must_use]
    pub const fn condition_before(&self) -> Condition {
        self.condition_before
    }

    #[must_use]
    pub const fn condition_after(&self) -> Condition {
        self.condition_after
    }

    #[must_use]
    pub const fn processing_rate(&self) -> MassFlow {
        self.processing_rate
    }

    #[must_use]
    pub const fn required_energy(&self) -> Energy {
        self.required_energy
    }

    #[must_use]
    pub const fn available_power(&self) -> Power {
        self.available_power
    }

    #[must_use]
    pub const fn throughput_duration(&self) -> TickSpan {
        self.throughput_duration
    }

    #[must_use]
    pub const fn energy_duration(&self) -> TickSpan {
        self.energy_duration
    }

    #[must_use]
    pub const fn target_mass(&self) -> Mass {
        self.target_mass
    }

    #[must_use]
    pub const fn residue_mass(&self) -> Mass {
        self.residue_mass
    }

    #[must_use]
    pub fn bottleneck(&self) -> PoweredOreBottleneck {
        classify_powered_ore_bottleneck(self.throughput_duration, self.energy_duration)
    }
}

/// Resolves an authored liberated feed into a recovered target stream and physical particulate residue.
pub fn resolve_constituent_separation_process(
    registries: &Registries,
    state: &AppState,
    request: ConstituentSeparationRequest<'_>,
) -> Result<ResolvedConstituentSeparation, ConstituentSeparationResolutionError> {
    let ConstituentSeparationRequest {
        process,
        source,
        selections,
        equipment,
        energy_store,
    } = request;
    let definition = registries
        .ore_processing()
        .get_constituent_separation(process)
        .ok_or(ConstituentSeparationResolutionError::UnknownProcess { process })?;
    let inputs = validate_process_inputs(state, process, source, selections)
        .map_err(ConstituentSeparationResolutionError::Input)?;
    let selected_mass = inputs.input_mass();
    let profile = definition.operating_profile();
    let provider = resolve_powered_ore_provider(
        registries,
        state,
        process,
        equipment,
        profile,
        selected_mass,
    )
    .map_err(ConstituentSeparationResolutionError::from)?;
    let processing_rate = provider.processing_rate();
    let target_particle_size_policy = registries
        .materials()
        .get_form(definition.target_output_form())
        .unwrap_or_else(|| {
            unreachable!("registered separation target output form must remain available")
        })
        .particle_size_policy();
    let outputs = resolve_separation_outputs(
        registries.materials(),
        definition.physics(),
        target_particle_size_policy,
        inputs.consumed_inputs(),
    )
    .map_err(ConstituentSeparationResolutionError::Batch)?;
    let supply = resolve_powered_ore_supply(
        registries,
        state,
        energy_store,
        profile,
        selected_mass,
        processing_rate,
        provider.condition_before(),
    )
    .map_err(ConstituentSeparationResolutionError::from)?;
    let required_energy = supply.required_energy();
    let available_power = supply.available_power();
    let throughput_duration = supply.throughput_duration();
    let energy_duration = supply.energy_duration();
    let duration = supply.duration();
    let condition_after = supply.condition_after();
    let resolution = inputs
        .resolve_with_energy_and_equipment(
            duration,
            vec![
                ProcessOutputStream::new(
                    ConstituentSeparationProcessDefinition::TARGET_STREAM,
                    outputs.target,
                ),
                ProcessOutputStream::new(
                    ConstituentSeparationProcessDefinition::RESIDUE_STREAM,
                    outputs.residue,
                ),
            ],
            supply.energy_supply(),
            provider.validated_use(),
            condition_after,
        )
        .map_err(ConstituentSeparationResolutionError::Resolution)?;
    Ok(ResolvedConstituentSeparation {
        resolution,
        equipment: provider.id(),
        condition_before: provider.condition_before(),
        condition_after,
        processing_rate,
        required_energy,
        available_power,
        throughput_duration,
        energy_duration,
        target_mass: outputs.target_mass,
        residue_mass: outputs.residue_mass,
    })
}

#[cfg(test)]
#[path = "separation_execution_tests.rs"]
mod tests;
