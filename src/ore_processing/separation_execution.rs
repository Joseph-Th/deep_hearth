//! Exact constituent-separation resolution and persisted-job audit for authored liberated feed.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::{CapabilityEvaluationError, CapabilityValue, evaluate_capabilities};
use crate::core::quantity::{Energy, Mass, MassFlow, Power, Temperature};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::energy::{
    EnergyCarrier, EnergyStoreId, EnergySupplyError, PowerDurationError,
    calculate_mass_specific_energy, calculate_power_duration_ceiling, validate_energy_supply,
};
use crate::equipment::{EquipmentId, EquipmentProviderError, resolve_equipment_provider};
use crate::inventory::{ConsumedMaterialTrace, MaterialLotSelection, StockpileId};
use crate::maintenance::{ActiveConditionDurationError, Condition};
use crate::material::{
    COMPOSITION_PARTS_PER_MILLION, CommodityKey, FormId, MaterialComposition, MaterialId,
    MaterialLotSpec, MaterialLotSpecError, ParticleSizeDistribution,
};
use crate::production::{
    ProcessId, ProcessInputError, ProcessOutputStream, ProcessResolution, ProcessResolutionError,
    validate_selected_process_inputs,
};
use crate::registry::Registries;

use super::timing::OreProcessActiveTiming;
use super::{
    ConstituentSeparationProcessDefinition, MassFlowDurationError,
    calculate_mass_flow_duration_ceiling,
};

mod validation;

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
            | Self::TargetBelowMassResolution { .. }
            | Self::MassOverflow => None,
        }
    }
}

#[derive(Debug)]
struct SeparationOutputs {
    target: Vec<MaterialLotSpec>,
    residue: Vec<MaterialLotSpec>,
    target_mass: Mass,
    residue_mass: Mass,
}

fn add_grouped_mass(
    grouped: &mut BTreeMap<Temperature, Mass>,
    temperature: Temperature,
    mass: Mass,
) -> Result<(), ConstituentSeparationBatchError> {
    let current = grouped.get(&temperature).copied().unwrap_or(Mass::ZERO);
    grouped.insert(
        temperature,
        current
            .checked_add(mass)
            .ok_or(ConstituentSeparationBatchError::MassOverflow)?,
    );
    Ok(())
}

fn build_pure_outputs(
    grouped: BTreeMap<Temperature, Mass>,
    commodity: CommodityKey,
) -> Result<Vec<MaterialLotSpec>, ConstituentSeparationBatchError> {
    grouped
        .into_iter()
        .map(|(temperature, mass)| {
            MaterialLotSpec::with_composition(
                commodity,
                mass,
                temperature,
                MaterialComposition::pure(commodity.material()),
            )
            .map_err(ConstituentSeparationBatchError::Output)
        })
        .collect()
}

fn build_particulate_outputs(
    grouped: BTreeMap<(Temperature, MaterialComposition, ParticleSizeDistribution), Mass>,
    commodity: CommodityKey,
) -> Result<Vec<MaterialLotSpec>, ConstituentSeparationBatchError> {
    grouped
        .into_iter()
        .map(|((temperature, composition, particle_size), mass)| {
            MaterialLotSpec::with_composition_and_particle_size(
                commodity,
                mass,
                temperature,
                composition,
                particle_size,
            )
            .map_err(ConstituentSeparationBatchError::Output)
        })
        .collect()
}

fn resolve_separation_outputs(
    definition: ConstituentSeparationProcessDefinition,
    traces: &[ConsumedMaterialTrace],
) -> Result<SeparationOutputs, ConstituentSeparationBatchError> {
    if traces.is_empty() {
        return Err(ConstituentSeparationBatchError::EmptyInput);
    }

    let mut grouped =
        BTreeMap::<(Temperature, MaterialComposition, ParticleSizeDistribution), Mass>::new();
    for trace in traces {
        let profile = trace.profile();
        if profile.commodity().form() != definition.input_form() {
            return Err(ConstituentSeparationBatchError::InputFormMismatch {
                expected: definition.input_form(),
                found: profile.commodity().form(),
            });
        }
        if profile.commodity().material() != definition.target_material() {
            return Err(ConstituentSeparationBatchError::InputHostMaterialMismatch {
                expected: definition.target_material(),
                found: profile.commodity().material(),
            });
        }
        for component in profile.composition().components() {
            if component.material() != definition.target_material()
                && component.material() != definition.residue_material()
            {
                return Err(ConstituentSeparationBatchError::UnsupportedConstituent {
                    material: component.material(),
                });
            }
        }
        if profile
            .composition()
            .parts_per_million(definition.target_material())
            == 0
        {
            return Err(ConstituentSeparationBatchError::MissingTargetConstituent {
                material: definition.target_material(),
            });
        }
        if profile
            .composition()
            .parts_per_million(definition.residue_material())
            == 0
        {
            return Err(ConstituentSeparationBatchError::MissingResidueConstituent {
                material: definition.residue_material(),
            });
        }
        let particle_size = profile
            .particle_size_distribution()
            .cloned()
            .unwrap_or_else(|| {
                unreachable!(
                    "authored constituent-separation input form requires particulate state"
                )
            });
        let key = (
            profile.temperature(),
            profile.composition().clone(),
            particle_size,
        );
        let current = grouped.get(&key).copied().unwrap_or(Mass::ZERO);
        grouped.insert(
            key,
            current
                .checked_add(trace.mass())
                .ok_or(ConstituentSeparationBatchError::MassOverflow)?,
        );
    }

    let mut target_by_temperature = BTreeMap::new();
    let mut residue_by_profile = BTreeMap::new();
    let mut target_mass = Mass::ZERO;
    let mut residue_mass = Mass::ZERO;
    for ((temperature, composition, particle_size), mass) in grouped {
        let target_ppm = composition.parts_per_million(definition.target_material());
        let numerator = u128::from(mass.milligrams()) * u128::from(target_ppm);
        let denominator = u128::from(COMPOSITION_PARTS_PER_MILLION);
        let target_milligrams = numerator / denominator;
        let target_remainder_ppm_mg = numerator % denominator;
        let target_milligrams = u64::try_from(target_milligrams)
            .map_err(|_| ConstituentSeparationBatchError::MassOverflow)?;
        let group_target = Mass::from_milligrams(target_milligrams);
        if group_target.is_zero() {
            return Err(ConstituentSeparationBatchError::TargetBelowMassResolution {
                material: definition.target_material(),
                selected: mass,
            });
        }
        let group_residue = mass
            .checked_sub(group_target)
            .unwrap_or_else(|| unreachable!("constituent projection cannot exceed selected mass"));
        debug_assert!(!group_residue.is_zero());

        add_grouped_mass(&mut target_by_temperature, temperature, group_target)?;
        let boundary_mass = if target_remainder_ppm_mg == 0 {
            Mass::ZERO
        } else {
            Mass::from_milligrams(1)
        };
        let pure_residue_mass = group_residue
            .checked_sub(boundary_mass)
            .unwrap_or_else(|| unreachable!("fractional target remainder requires residue mass"));
        if !pure_residue_mass.is_zero() {
            let pure_key = (
                temperature,
                MaterialComposition::pure(definition.residue_material()),
                particle_size.clone(),
            );
            let current = residue_by_profile
                .get(&pure_key)
                .copied()
                .unwrap_or(Mass::ZERO);
            residue_by_profile.insert(
                pure_key,
                current
                    .checked_add(pure_residue_mass)
                    .ok_or(ConstituentSeparationBatchError::MassOverflow)?,
            );
        }
        if !boundary_mass.is_zero() {
            let boundary_target_ppm = u32::try_from(target_remainder_ppm_mg)
                .unwrap_or_else(|_| unreachable!("ppm remainder is below normalization"));
            let boundary_composition = MaterialComposition::new(vec![
                crate::material::CompositionComponent::new(
                    definition.target_material(),
                    boundary_target_ppm,
                ),
                crate::material::CompositionComponent::new(
                    definition.residue_material(),
                    COMPOSITION_PARTS_PER_MILLION - boundary_target_ppm,
                ),
            ])
            .unwrap_or_else(|error| {
                unreachable!("bounded separation boundary composition must be valid: {error}")
            });
            let boundary_key = (temperature, boundary_composition, particle_size);
            let current = residue_by_profile
                .get(&boundary_key)
                .copied()
                .unwrap_or(Mass::ZERO);
            residue_by_profile.insert(
                boundary_key,
                current
                    .checked_add(boundary_mass)
                    .ok_or(ConstituentSeparationBatchError::MassOverflow)?,
            );
        }
        target_mass = target_mass
            .checked_add(group_target)
            .ok_or(ConstituentSeparationBatchError::MassOverflow)?;
        residue_mass = residue_mass
            .checked_add(group_residue)
            .ok_or(ConstituentSeparationBatchError::MassOverflow)?;
    }

    Ok(SeparationOutputs {
        target: build_pure_outputs(
            target_by_temperature,
            CommodityKey::new(
                definition.target_material(),
                definition.target_output_form(),
            ),
        )?,
        residue: build_particulate_outputs(
            residue_by_profile,
            CommodityKey::new(
                definition.residue_material(),
                definition.residue_output_form(),
            ),
        )?,
        target_mass,
        residue_mass,
    })
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

/// Resolves an authored two-constituent liberated feed into pure target and residue streams.
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
    let processing_rate = match provider.get_capability(definition.mass_flow_capability()) {
        Some(CapabilityValue::MassFlow(rate)) => rate,
        Some(_) | None => {
            return Err(ConstituentSeparationResolutionError::MissingMassFlowCapability);
        }
    };
    let maximum_batch_mass = match provider.get_capability(definition.max_batch_mass_capability()) {
        Some(CapabilityValue::Mass(mass)) => mass,
        Some(_) | None => {
            return Err(ConstituentSeparationResolutionError::MissingMaximumBatchMassCapability);
        }
    };
    let selected_mass = inputs.input_mass();
    if selected_mass > maximum_batch_mass {
        return Err(ConstituentSeparationResolutionError::BatchMassExceeded {
            selected: selected_mass,
            maximum: maximum_batch_mass,
        });
    }
    let outputs = resolve_separation_outputs(definition, inputs.consumed_inputs())
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
    let throughput_duration = calculate_mass_flow_duration_ceiling(
        processing_rate,
        selected_mass,
        registries.core().physical_tick_duration(),
    )
    .map_err(ConstituentSeparationResolutionError::ThroughputDuration)?;
    let available_power = energy_supply.max_output_power();
    let energy_duration = calculate_power_duration_ceiling(
        available_power,
        required_energy,
        registries.core().physical_tick_duration(),
    )
    .map_err(ConstituentSeparationResolutionError::EnergyDuration)?;
    let timing = OreProcessActiveTiming::new(throughput_duration, energy_duration);
    let duration = timing.duration();
    let condition_after = timing
        .condition_after(
            definition.condition_wear_ppm_per_active_tick(),
            provider.condition(),
        )
        .map_err(ConstituentSeparationResolutionError::ConditionDuration)?;
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
