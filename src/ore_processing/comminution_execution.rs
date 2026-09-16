//! Resolves exact manual and powered comminution operations.

use crate::core::quantity::{Energy, MassFlow, Power};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::energy::EnergyStoreId;
use crate::equipment::EquipmentId;
use crate::inventory::{MaterialLotSelection, StockpileId};
use crate::maintenance::Condition;
use crate::production::{
    ProcessId, ProcessOutputStream, ProcessOutputStreamId, ProcessResolution,
    validate_process_inputs,
};
use crate::registry::Registries;

use super::powered_physics::{
    PoweredOreBottleneck, classify_powered_ore_bottleneck, resolve_powered_ore_provider,
    resolve_powered_ore_supply,
};

mod errors;
mod manual;
mod outputs;
mod validation;

pub use errors::ComminutionResolutionError;
pub use manual::{
    ManualComminutionCommitError, ManualComminutionRequest, ManualComminutionResolutionError,
    ResolvedManualComminution, StartManualComminutionError, ValidatedManualComminutionStart,
    resolve_manual_comminution_process, validate_start_manual_comminution,
};
pub use outputs::ComminutionBatchError;
use outputs::resolve_comminution_outputs;
pub use validation::ComminutionJobValidationError;
pub(crate) use validation::validate_loaded_comminution_job;

#[cfg(test)]
use crate::core::quantity::Temperature;
#[cfg(test)]
use crate::material::{CommodityKey, MaterialComposition, MaterialLotSpec, ParticleSizeRange};

/// Runtime request to reduce one explicitly selected solid batch to an authored finer form.
#[derive(Clone, Copy, Debug)]
pub struct ComminutionRequest<'selection> {
    process: ProcessId,
    source: StockpileId,
    selections: &'selection [MaterialLotSelection],
    equipment: EquipmentId,
    energy_store: EnergyStoreId,
}

impl<'selection> ComminutionRequest<'selection> {
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

/// Fully resolved comminution operation ready for the canonical production start transaction.
#[must_use]
#[derive(Debug)]
pub struct ResolvedComminution {
    resolution: ProcessResolution,
    equipment: EquipmentId,
    condition_before: Condition,
    condition_after: Condition,
    processing_rate: MassFlow,
    required_energy: Energy,
    available_power: Power,
    throughput_duration: TickSpan,
    energy_duration: TickSpan,
}

impl ResolvedComminution {
    pub const fn process_resolution(&self) -> &ProcessResolution {
        &self.resolution
    }

    #[must_use]
    pub const fn equipment(&self) -> EquipmentId {
        self.equipment
    }

    /// Equipment condition observed when this operation was resolved.
    #[must_use]
    pub const fn condition_before(&self) -> Condition {
        self.condition_before
    }

    /// Predicted equipment condition after the resolved active duration completes.
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

    /// Duration imposed by condition-adjusted equipment material throughput alone.
    #[must_use]
    pub const fn throughput_duration(&self) -> TickSpan {
        self.throughput_duration
    }

    /// Duration imposed by the selected finite energy source's delivery power alone.
    #[must_use]
    pub const fn energy_duration(&self) -> TickSpan {
        self.energy_duration
    }

    /// Reports which physical rate constraint currently determines authoritative duration.
    #[must_use]
    pub fn bottleneck(&self) -> PoweredOreBottleneck {
        classify_powered_ore_bottleneck(self.throughput_duration, self.energy_duration)
    }
}

/// Resolves exact crushing/grinding behavior from selected solid matter and runtime equipment.
///
/// Comminution assigns an authored weighted particle-size distribution while preserving each
/// distinct composition and temperature. Particulate inputs must be strictly reduced at the
/// distribution envelope without coarsening represented fines, and constrained operations require
/// every selected feed envelope to lie inside their authored operating range. Untracked coarse inputs
/// establish their first explicit size state. It does not purify ore or invent yield bonuses. Exact
/// mass-specific work is reserved from a finite energy source, while operation duration is the
/// slower of equipment throughput and source power.
pub fn resolve_comminution_process(
    registries: &Registries,
    state: &AppState,
    request: ComminutionRequest<'_>,
) -> Result<ResolvedComminution, ComminutionResolutionError> {
    let ComminutionRequest {
        process,
        source,
        selections,
        equipment,
        energy_store,
    } = request;
    let definition = registries
        .ore_processing()
        .get_comminution(process)
        .ok_or(ComminutionResolutionError::UnknownComminutionProcess { process })?;
    let inputs = validate_process_inputs(registries, state, process, source, selections)
        .map_err(ComminutionResolutionError::Input)?;
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
    .map_err(ComminutionResolutionError::from)?;
    let processing_rate = provider.processing_rate();
    let outputs = resolve_comminution_outputs(definition, inputs.consumed_inputs())
        .map_err(ComminutionResolutionError::Batch)?;
    let supply = resolve_powered_ore_supply(
        registries,
        state,
        energy_store,
        profile,
        selected_mass,
        processing_rate,
        provider.condition_before(),
    )
    .map_err(ComminutionResolutionError::from)?;
    let required_energy = supply.required_energy();
    let available_power = supply.available_power();
    let throughput_duration = supply.throughput_duration();
    let energy_duration = supply.energy_duration();
    let duration = supply.duration();
    let condition_after = supply.condition_after();
    let resolution = inputs
        .resolve_with_energy_and_equipment(
            duration,
            vec![ProcessOutputStream::new(
                ProcessOutputStreamId::PRIMARY,
                outputs,
            )],
            supply.energy_supply(),
            provider.validated_use(),
            condition_after,
        )
        .map_err(ComminutionResolutionError::Resolution)?;

    Ok(ResolvedComminution {
        resolution,
        equipment: provider.id(),
        condition_before: provider.condition_before(),
        condition_after,
        processing_rate,
        required_energy,
        available_power,
        throughput_duration,
        energy_duration,
    })
}

#[cfg(test)]
#[path = "comminution_execution_tests.rs"]
mod tests;
