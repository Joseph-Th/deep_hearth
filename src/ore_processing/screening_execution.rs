//! Exact particle-size screening resolution and actor-safe representability planning.

use crate::core::quantity::{Energy, Mass, MassFlow, Power};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::energy::EnergyStoreId;
use crate::equipment::EquipmentId;
use crate::inventory::{MaterialLotSelection, StockpileId};
use crate::maintenance::Condition;
use crate::production::{ProcessId, ProcessResolution, validate_selected_process_inputs};
use crate::registry::Registries;

use super::definitions::ScreeningProcessDefinition;
use super::powered_physics::{
    PoweredOreBottleneck, classify_powered_ore_bottleneck, resolve_powered_ore_provider,
    resolve_powered_ore_supply,
};

mod errors;
mod outputs;
mod validation;

pub use errors::{ScreeningBatchError, ScreeningResolutionError};
use outputs::{representable_screening_mass_floor, resolve_screening_outputs};
pub use validation::ScreeningJobValidationError;
pub(crate) use validation::validate_loaded_screening_job;

#[cfg(test)]
use crate::core::quantity::Temperature;
#[cfg(test)]
use crate::material::{
    CommodityKey, MaterialComposition, MaterialLotSpec, ParticleSizeDistribution, ParticleSizeRange,
};

/// Runtime request to classify one explicitly selected particulate batch by an authored aperture.
#[derive(Clone, Copy, Debug)]
pub struct ScreeningRequest<'selection> {
    process: ProcessId,
    source: StockpileId,
    selections: &'selection [MaterialLotSelection],
    equipment: EquipmentId,
    energy_store: EnergyStoreId,
}

impl<'selection> ScreeningRequest<'selection> {
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

/// Fully resolved screening operation ready for the canonical production start transaction.
#[must_use]
#[derive(Debug)]
pub struct ResolvedScreening {
    resolution: ProcessResolution,
    equipment: ScreeningEquipmentProfile,
    constraints: ScreeningConstraintProfile,
    partition: ScreeningPartition,
}

#[derive(Debug)]
struct ScreeningEquipmentProfile {
    id: EquipmentId,
    condition_before: Condition,
    condition_after: Condition,
}

#[derive(Debug)]
struct ScreeningConstraintProfile {
    processing_rate: MassFlow,
    required_energy: Energy,
    available_power: Power,
    throughput_duration: TickSpan,
    energy_duration: TickSpan,
}

#[derive(Debug)]
struct ScreeningPartition {
    undersize_mass: Mass,
    oversize_mass: Mass,
}

impl ResolvedScreening {
    pub const fn process_resolution(&self) -> &ProcessResolution {
        &self.resolution
    }

    #[must_use]
    pub const fn equipment(&self) -> EquipmentId {
        self.equipment.id
    }

    #[must_use]
    pub const fn condition_before(&self) -> Condition {
        self.equipment.condition_before
    }

    #[must_use]
    pub const fn condition_after(&self) -> Condition {
        self.equipment.condition_after
    }

    #[must_use]
    pub const fn processing_rate(&self) -> MassFlow {
        self.constraints.processing_rate
    }

    #[must_use]
    pub const fn required_energy(&self) -> Energy {
        self.constraints.required_energy
    }

    #[must_use]
    pub const fn available_power(&self) -> Power {
        self.constraints.available_power
    }

    #[must_use]
    pub const fn throughput_duration(&self) -> TickSpan {
        self.constraints.throughput_duration
    }

    #[must_use]
    pub const fn energy_duration(&self) -> TickSpan {
        self.constraints.energy_duration
    }

    #[must_use]
    pub const fn undersize_mass(&self) -> Mass {
        self.partition.undersize_mass
    }

    #[must_use]
    pub const fn oversize_mass(&self) -> Mass {
        self.partition.oversize_mass
    }

    #[must_use]
    pub fn bottleneck(&self) -> PoweredOreBottleneck {
        classify_powered_ore_bottleneck(
            self.constraints.throughput_duration,
            self.constraints.energy_duration,
        )
    }
}

/// Resolves exact dry screening from selected particulate matter and runtime equipment.
///
/// Relative size-class weights are converted to whole-milligram stream masses only after identical
/// physical input profiles have been aggregated. This makes the result independent of lot
/// fragmentation. If the weighted partition is not exactly representable at whole-milligram mass
/// resolution, resolution is refused rather than silently reclassifying a fractional amount into the
/// wrong particle-size stream.
/// Returns the greatest whole-milligram batch at or below `requested` whose authored particle
/// partition can be represented exactly.
///
/// This is an actor-safe planning projection over observable particle-size state. It prevents
/// callers from duplicating screen-weight divisibility rules or probing neighboring masses until
/// one happens to resolve. Equipment, energy, and condition limits are still validated by
/// [`resolve_screening_process`].
#[must_use = "screening planning results must be used"]
pub fn resolve_representable_screening_mass(
    definition: ScreeningProcessDefinition,
    distribution: &crate::material::ParticleSizeDistribution,
    requested: Mass,
) -> Result<Mass, ScreeningBatchError> {
    representable_screening_mass_floor(definition, distribution, requested)
}

pub fn resolve_screening_process(
    registries: &Registries,
    state: &AppState,
    request: ScreeningRequest<'_>,
) -> Result<ResolvedScreening, ScreeningResolutionError> {
    let ScreeningRequest {
        process,
        source,
        selections,
        equipment,
        energy_store,
    } = request;
    let definition = registries
        .ore_processing()
        .get_screening(process)
        .ok_or(ScreeningResolutionError::UnknownScreeningProcess { process })?;
    let inputs = validate_selected_process_inputs(registries, state, process, source, selections)
        .map_err(ScreeningResolutionError::Input)?;
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
    .map_err(ScreeningResolutionError::from)?;
    let processing_rate = provider.processing_rate();

    let outputs = resolve_screening_outputs(definition, inputs.consumed_inputs())
        .map_err(ScreeningResolutionError::Batch)?;
    let supply = resolve_powered_ore_supply(
        registries,
        state,
        energy_store,
        profile,
        selected_mass,
        processing_rate,
        provider.condition_before(),
    )
    .map_err(ScreeningResolutionError::from)?;
    let required_energy = supply.required_energy();
    let available_power = supply.available_power();
    let throughput_duration = supply.throughput_duration();
    let energy_duration = supply.energy_duration();
    let duration = supply.duration();
    let condition_after = supply.condition_after();
    let resolution = inputs
        .resolve_with_energy_and_equipment(
            duration,
            outputs.streams,
            supply.energy_supply(),
            provider.validated_use(),
            condition_after,
        )
        .map_err(ScreeningResolutionError::Resolution)?;

    Ok(ResolvedScreening {
        resolution,
        equipment: ScreeningEquipmentProfile {
            id: provider.id(),
            condition_before: provider.condition_before(),
            condition_after,
        },
        constraints: ScreeningConstraintProfile {
            processing_rate,
            required_energy,
            available_power,
            throughput_duration,
            energy_duration,
        },
        partition: ScreeningPartition {
            undersize_mass: outputs.undersize_mass,
            oversize_mass: outputs.oversize_mass,
        },
    })
}

#[cfg(test)]
#[path = "screening_execution_tests.rs"]
mod tests;
