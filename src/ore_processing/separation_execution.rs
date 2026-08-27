//! Exact constituent-separation resolution and persisted-job audit for authored liberated feed.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::{CapabilityEvaluationError, evaluate_capabilities};
use crate::core::quantity::{Energy, Mass, MassFlow, Power};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::energy::{
    EnergyCarrier, EnergyStoreId, EnergySupplyError, PowerDurationError,
    calculate_mass_specific_energy, validate_energy_supply,
};
use crate::equipment::{EquipmentId, EquipmentProviderError, resolve_equipment_provider};
use crate::inventory::{MaterialLotSelection, StockpileId};
use crate::maintenance::{ActiveConditionDurationError, Condition};
use crate::material::{FormId, MaterialId, MaterialLotSpecError};
use crate::production::{
    ProcessId, ProcessInputError, ProcessOutputStream, ProcessResolution, ProcessResolutionError,
    validate_selected_process_inputs,
};
use crate::registry::Registries;

use super::powered_physics::{
    PoweredOreEquipmentError, PoweredOreTimingError, resolve_powered_ore_equipment,
    resolve_powered_ore_timing,
};
use super::{ConstituentSeparationProcessDefinition, MassFlowDurationError};

mod outputs;
mod validation;

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

/// Failure while deriving physically conservative constituent streams from selected feed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConstituentSeparationBatchError {
    EmptyInput,
    InputFormMismatch {
        expected: FormId,
        found: FormId,
    },
    InputHostMaterialMismatch {
        expected: MaterialId,
        found: MaterialId,
    },
    UnsupportedConstituent {
        material: MaterialId,
    },
    MissingTargetConstituent {
        material: MaterialId,
    },
    MissingResidueConstituent {
        material: MaterialId,
    },
    MissingNonTargetConstituent,
    TargetBelowMassResolution {
        material: MaterialId,
        selected: Mass,
    },
    MassOverflow,
    Output(MaterialLotSpecError),
}

impl Display for ConstituentSeparationBatchError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyInput => {
                formatter.write_str("constituent-separation batch contains no material")
            }
            Self::InputFormMismatch { expected, found } => write!(
                formatter,
                "constituent separation requires input form {} but selected form {}",
                expected.value(),
                found.value()
            ),
            Self::InputHostMaterialMismatch { expected, found } => write!(
                formatter,
                "constituent separation requires host material {} but selected commodity uses {}",
                expected.value(),
                found.value()
            ),
            Self::UnsupportedConstituent { material } => write!(
                formatter,
                "constituent separation cannot classify un-authored material {}",
                material.value()
            ),
            Self::MissingTargetConstituent { material } => write!(
                formatter,
                "constituent separation feed contains no authored target material {}",
                material.value()
            ),
            Self::MissingResidueConstituent { material } => write!(
                formatter,
                "constituent separation feed contains no authored residue material {}",
                material.value()
            ),
            Self::MissingNonTargetConstituent => formatter.write_str(
                "constituent concentration requires at least one non-target constituent",
            ),
            Self::TargetBelowMassResolution { material, selected } => write!(
                formatter,
                "selected {} mg contains less than one authoritative milligram of recoverable target material {}",
                selected.milligrams(),
                material.value()
            ),
            Self::MassOverflow => {
                formatter.write_str("constituent-separation output mass overflowed")
            }
            Self::Output(error) => write!(
                formatter,
                "constituent-separation output specification is invalid: {error}"
            ),
        }
    }
}

impl Error for ConstituentSeparationBatchError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Output(error) => Some(error),
            Self::EmptyInput
            | Self::InputFormMismatch { .. }
            | Self::InputHostMaterialMismatch { .. }
            | Self::UnsupportedConstituent { .. }
            | Self::MissingTargetConstituent { .. }
            | Self::MissingResidueConstituent { .. }
            | Self::MissingNonTargetConstituent
            | Self::TargetBelowMassResolution { .. }
            | Self::MassOverflow => None,
        }
    }
}

/// Failure while resolving one exact constituent-separation operation before mutation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConstituentSeparationResolutionError {
    UnknownProcess {
        process: ProcessId,
    },
    Input(ProcessInputError),
    Equipment(EquipmentProviderError),
    Capability(CapabilityEvaluationError),
    MissingMassFlowCapability,
    MissingMaximumBatchMassCapability,
    BatchMassExceeded {
        selected: Mass,
        maximum: Mass,
    },
    Batch(ConstituentSeparationBatchError),
    Energy(EnergySupplyError),
    WrongEnergyCarrier {
        required: EnergyCarrier,
        provided: EnergyCarrier,
    },
    ThroughputDuration(MassFlowDurationError),
    EnergyDuration(PowerDurationError),
    ConditionDuration(ActiveConditionDurationError),
    Resolution(ProcessResolutionError),
}

impl Display for ConstituentSeparationResolutionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownProcess { process } => write!(
                formatter,
                "process {} has no authored constituent-separation semantics",
                process.value()
            ),
            Self::Input(error) => write!(formatter, "constituent-separation input failed: {error}"),
            Self::Equipment(error) => write!(
                formatter,
                "constituent-separation equipment failed: {error}"
            ),
            Self::Capability(error) => write!(
                formatter,
                "constituent-separation capability failed: {error}"
            ),
            Self::MissingMassFlowCapability => formatter
                .write_str("constituent-separation equipment has no usable mass-flow capability"),
            Self::MissingMaximumBatchMassCapability => formatter.write_str(
                "constituent-separation equipment has no usable maximum-batch capability",
            ),
            Self::BatchMassExceeded { selected, maximum } => write!(
                formatter,
                "selected constituent-separation batch {} mg exceeds equipment maximum {} mg",
                selected.milligrams(),
                maximum.milligrams()
            ),
            Self::Batch(error) => write!(formatter, "constituent-separation batch failed: {error}"),
            Self::Energy(error) => write!(
                formatter,
                "constituent-separation energy supply failed: {error}"
            ),
            Self::WrongEnergyCarrier { required, provided } => write!(
                formatter,
                "constituent separation requires {required:?} energy but source provides {provided:?}"
            ),
            Self::ThroughputDuration(error) => write!(
                formatter,
                "constituent-separation throughput duration failed: {error}"
            ),
            Self::EnergyDuration(error) => write!(
                formatter,
                "constituent-separation energy duration failed: {error}"
            ),
            Self::ConditionDuration(error) => write!(
                formatter,
                "constituent separation exceeds equipment condition lifetime: {error}"
            ),
            Self::Resolution(error) => write!(
                formatter,
                "constituent-separation process resolution failed: {error}"
            ),
        }
    }
}

impl Error for ConstituentSeparationResolutionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Input(error) => Some(error),
            Self::Equipment(error) => Some(error),
            Self::Capability(error) => Some(error),
            Self::Batch(error) => Some(error),
            Self::Energy(error) => Some(error),
            Self::ThroughputDuration(error) => Some(error),
            Self::EnergyDuration(error) => Some(error),
            Self::ConditionDuration(error) => Some(error),
            Self::Resolution(error) => Some(error),
            Self::UnknownProcess { .. }
            | Self::MissingMassFlowCapability
            | Self::MissingMaximumBatchMassCapability
            | Self::BatchMassExceeded { .. }
            | Self::WrongEnergyCarrier { .. } => None,
        }
    }
}

/// Physical rate constraint currently setting resolved constituent-separation duration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstituentSeparationBottleneck {
    Throughput,
    EnergyDelivery,
    Balanced,
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
    pub fn bottleneck(&self) -> ConstituentSeparationBottleneck {
        match self.throughput_duration.cmp(&self.energy_duration) {
            std::cmp::Ordering::Greater => ConstituentSeparationBottleneck::Throughput,
            std::cmp::Ordering::Less => ConstituentSeparationBottleneck::EnergyDelivery,
            std::cmp::Ordering::Equal => ConstituentSeparationBottleneck::Balanced,
        }
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
    let inputs = validate_selected_process_inputs(registries, state, process, source, selections)
        .map_err(ConstituentSeparationResolutionError::Input)?;
    let provider = resolve_equipment_provider(registries, state, equipment)
        .map_err(ConstituentSeparationResolutionError::Equipment)?;
    let process_definition = registries
        .production()
        .get_process(process)
        .ok_or(ConstituentSeparationResolutionError::UnknownProcess { process })?;
    evaluate_capabilities(
        registries.capabilities(),
        &provider,
        process_definition.capability_requirements(),
    )
    .map_err(ConstituentSeparationResolutionError::Capability)?;
    let selected_mass = inputs.input_mass();
    let powered_equipment = resolve_powered_ore_equipment(
        provider.definition(),
        provider.condition(),
        definition.mass_flow_capability(),
        definition.max_batch_mass_capability(),
        selected_mass,
    )
    .map_err(|error| match error {
        PoweredOreEquipmentError::MissingMassFlowCapability => {
            ConstituentSeparationResolutionError::MissingMassFlowCapability
        }
        PoweredOreEquipmentError::MissingMaximumBatchMassCapability => {
            ConstituentSeparationResolutionError::MissingMaximumBatchMassCapability
        }
        PoweredOreEquipmentError::BatchMassExceeded { selected, maximum } => {
            ConstituentSeparationResolutionError::BatchMassExceeded { selected, maximum }
        }
    })?;
    let processing_rate = powered_equipment.processing_rate();
    let target_particle_size_policy = registries
        .materials()
        .get_form(definition.target_output_form())
        .unwrap_or_else(|| {
            unreachable!("registered separation target output form must remain available")
        })
        .particle_size_policy();
    let outputs = resolve_separation_outputs(
        definition,
        target_particle_size_policy,
        inputs.consumed_inputs(),
    )
    .map_err(ConstituentSeparationResolutionError::Batch)?;
    let required_energy =
        calculate_mass_specific_energy(selected_mass, definition.specific_energy());
    let energy_supply = validate_energy_supply(registries, state, energy_store, required_energy)
        .map_err(ConstituentSeparationResolutionError::Energy)?;
    if energy_supply.trace().carrier() != definition.energy_carrier() {
        return Err(ConstituentSeparationResolutionError::WrongEnergyCarrier {
            required: definition.energy_carrier(),
            provided: energy_supply.trace().carrier(),
        });
    }
    let available_power = energy_supply.max_output_power();
    let timing = resolve_powered_ore_timing(
        registries,
        processing_rate,
        selected_mass,
        required_energy,
        available_power,
        definition.condition_wear_ppm_per_active_tick(),
        provider.condition(),
    )
    .map_err(|error| match error {
        PoweredOreTimingError::Throughput(error) => {
            ConstituentSeparationResolutionError::ThroughputDuration(error)
        }
        PoweredOreTimingError::Energy(error) => {
            ConstituentSeparationResolutionError::EnergyDuration(error)
        }
        PoweredOreTimingError::Condition(error) => {
            ConstituentSeparationResolutionError::ConditionDuration(error)
        }
    })?;
    let throughput_duration = timing.throughput_duration();
    let energy_duration = timing.energy_duration();
    let duration = timing.duration();
    let condition_after = timing.condition_after();
    let equipment_use = provider.validated_use();
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
            energy_supply,
            equipment_use,
            condition_after,
        )
        .map_err(ConstituentSeparationResolutionError::Resolution)?;
    Ok(ResolvedConstituentSeparation {
        resolution,
        equipment,
        condition_before: provider.condition(),
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
