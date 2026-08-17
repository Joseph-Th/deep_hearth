//! Pure-material melting resolution; sibling thermal code owns sensible heating and shared registry dispatch.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::{
    CapabilityEvaluationError, CapabilityId, CapabilityValue, evaluate_capabilities,
};
use crate::core::quantity::{Energy, Mass, Power, Temperature};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::energy::{
    EnergyCarrier, EnergyStoreId, EnergySupplyError, PowerDurationError,
    calculate_power_duration_ceiling, validate_energy_supply,
};
use crate::equipment::{
    EquipmentId, EquipmentProviderError, resolve_equipment_capability, resolve_equipment_provider,
};
use crate::inventory::{ConsumedMaterialTrace, MaterialLotSelection, StockpileId};
use crate::maintenance::{Condition, calculate_condition_after_active_ticks};
use crate::material::{
    CommodityKey, FormId, MaterialComposition, MaterialId, MaterialLotSpec, MaterialLotSpecError,
    MaterialPhase, MaterialRegistry,
};
use crate::production::{
    ProcessId, ProcessInputError, ProcessOutputStream, ProcessOutputStreamId, ProcessResolution,
    ProcessResolutionError, ProductionJobId, ProductionJobRecord, validate_selected_process_inputs,
};
use crate::registry::Registries;

use super::{FusionHeatError, SensibleHeatError, calculate_fusion_heat, calculate_sensible_heat};

/// Immutable declaration that one selected-batch process performs pure-material melting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeltingProcessDefinition {
    process: ProcessId,
    heating_power_capability: CapabilityId,
    max_temperature_capability: CapabilityId,
    max_batch_mass_capability: CapabilityId,
    energy_carrier: EnergyCarrier,
    liquid_form: FormId,
    condition_wear_ppm_per_active_tick: u32,
}

impl MeltingProcessDefinition {
    #[must_use]
    pub const fn new(
        process: ProcessId,
        heating_power_capability: CapabilityId,
        max_temperature_capability: CapabilityId,
        max_batch_mass_capability: CapabilityId,
        energy_carrier: EnergyCarrier,
        liquid_form: FormId,
        condition_wear_ppm_per_active_tick: u32,
    ) -> Self {
        Self {
            process,
            heating_power_capability,
            max_temperature_capability,
            max_batch_mass_capability,
            energy_carrier,
            liquid_form,
            condition_wear_ppm_per_active_tick,
        }
    }

    #[must_use]
    pub const fn process(self) -> ProcessId {
        self.process
    }

    #[must_use]
    pub const fn heating_power_capability(self) -> CapabilityId {
        self.heating_power_capability
    }

    #[must_use]
    pub const fn max_temperature_capability(self) -> CapabilityId {
        self.max_temperature_capability
    }

    #[must_use]
    pub const fn max_batch_mass_capability(self) -> CapabilityId {
        self.max_batch_mass_capability
    }

    #[must_use]
    pub const fn energy_carrier(self) -> EnergyCarrier {
        self.energy_carrier
    }

    #[must_use]
    pub const fn liquid_form(self) -> FormId {
        self.liquid_form
    }

    #[must_use]
    pub const fn condition_wear_ppm_per_active_tick(self) -> u32 {
        self.condition_wear_ppm_per_active_tick
    }
}

/// Failure while deriving pure melting physics from exact consumed material traces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MeltingBatchError {
    EmptyInput,
    UnknownInputForm {
        form: FormId,
    },
    InputNotSolid {
        form: FormId,
        phase: MaterialPhase,
    },
    ImpureInput {
        commodity: CommodityKey,
    },
    PureMaterialDoesNotMatchCommodity {
        commodity: CommodityKey,
        pure: MaterialId,
    },
    MixedMaterials {
        expected: MaterialId,
        found: MaterialId,
    },
    InputAboveMeltingPoint {
        material: MaterialId,
        current: Temperature,
        melting_point: Temperature,
    },
    SensibleHeat {
        material: MaterialId,
        error: SensibleHeatError,
    },
    FusionHeat {
        material: MaterialId,
        error: FusionHeatError,
    },
    EnergyOverflow,
    MassOverflow,
    Output(MaterialLotSpecError),
}

impl Display for MeltingBatchError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyInput => formatter.write_str("melting batch contains no material"),
            Self::UnknownInputForm { form } => {
                write!(
                    formatter,
                    "melting batch references unknown form {}",
                    form.value()
                )
            }
            Self::InputNotSolid { form, phase } => write!(
                formatter,
                "melting input form {} is {phase:?} rather than solid",
                form.value()
            ),
            Self::ImpureInput { commodity } => write!(
                formatter,
                "melting input material {} in form {} is compositionally mixed; alloy phase diagrams are not modeled",
                commodity.material().value(),
                commodity.form().value()
            ),
            Self::PureMaterialDoesNotMatchCommodity { commodity, pure } => write!(
                formatter,
                "melting input material {} in form {} claims pure material {} instead",
                commodity.material().value(),
                commodity.form().value(),
                pure.value()
            ),
            Self::MixedMaterials { expected, found } => write!(
                formatter,
                "melting batch mixes material {} with material {}; alloying requires a dedicated resolver",
                expected.value(),
                found.value()
            ),
            Self::InputAboveMeltingPoint {
                material,
                current,
                melting_point,
            } => write!(
                formatter,
                "solid material {} is at {} mK above its {} mK melting point",
                material.value(),
                current.millikelvin(),
                melting_point.millikelvin()
            ),
            Self::SensibleHeat { material, error } => write!(
                formatter,
                "melting material {} cannot be heated to its fusion boundary: {error}",
                material.value()
            ),
            Self::FusionHeat { material, error } => write!(
                formatter,
                "melting material {} cannot resolve latent heat: {error}",
                material.value()
            ),
            Self::EnergyOverflow => {
                formatter.write_str("melting batch energy requirement overflowed")
            }
            Self::MassOverflow => formatter.write_str("melting batch mass overflowed"),
            Self::Output(error) => write!(formatter, "molten output construction failed: {error}"),
        }
    }
}

impl Error for MeltingBatchError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::SensibleHeat { error, .. } => Some(error),
            Self::FusionHeat { error, .. } => Some(error),
            Self::Output(error) => Some(error),
            Self::EmptyInput
            | Self::UnknownInputForm { .. }
            | Self::InputNotSolid { .. }
            | Self::ImpureInput { .. }
            | Self::PureMaterialDoesNotMatchCommodity { .. }
            | Self::MixedMaterials { .. }
            | Self::InputAboveMeltingPoint { .. }
            | Self::EnergyOverflow
            | Self::MassOverflow => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MeltingBatchPhysics {
    material: MaterialId,
    melting_point: Temperature,
    required_energy: Energy,
    output: MaterialLotSpec,
}

fn resolve_melting_batch(
    materials: &MaterialRegistry,
    liquid_form: FormId,
    traces: &[ConsumedMaterialTrace],
) -> Result<MeltingBatchPhysics, MeltingBatchError> {
    let mut batch_material = None;
    let mut melting_point = None;
    let mut total_mass = Mass::ZERO;
    let mut required_energy = Energy::ZERO;

    for trace in traces {
        let profile = trace.profile();
        let form_id = profile.commodity().form();
        let Some(form) = materials.get_form(form_id) else {
            return Err(MeltingBatchError::UnknownInputForm { form: form_id });
        };
        if form.phase() != MaterialPhase::Solid {
            return Err(MeltingBatchError::InputNotSolid {
                form: form_id,
                phase: form.phase(),
            });
        }
        let Some(material) = profile.composition().pure_material() else {
            return Err(MeltingBatchError::ImpureInput {
                commodity: profile.commodity(),
            });
        };
        if profile.commodity().material() != material {
            return Err(MeltingBatchError::PureMaterialDoesNotMatchCommodity {
                commodity: profile.commodity(),
                pure: material,
            });
        }
        if let Some(expected) = batch_material {
            if expected != material {
                return Err(MeltingBatchError::MixedMaterials {
                    expected,
                    found: material,
                });
            }
        } else {
            batch_material = Some(material);
        }

        let fusion = calculate_fusion_heat(materials, trace.mass(), material)
            .map_err(|error| MeltingBatchError::FusionHeat { material, error })?;
        if profile.temperature() > fusion.melting_point() {
            return Err(MeltingBatchError::InputAboveMeltingPoint {
                material,
                current: profile.temperature(),
                melting_point: fusion.melting_point(),
            });
        }
        if let Some(expected) = melting_point {
            debug_assert_eq!(expected, fusion.melting_point());
        } else {
            melting_point = Some(fusion.melting_point());
        }
        let sensible = calculate_sensible_heat(
            materials,
            trace.mass(),
            profile.composition(),
            profile.temperature(),
            fusion.melting_point(),
        )
        .map_err(|error| MeltingBatchError::SensibleHeat { material, error })?;
        required_energy = required_energy
            .checked_add(sensible.energy())
            .and_then(|energy| energy.checked_add(fusion.energy()))
            .ok_or(MeltingBatchError::EnergyOverflow)?;
        total_mass = total_mass
            .checked_add(trace.mass())
            .ok_or(MeltingBatchError::MassOverflow)?;
    }

    let Some(material) = batch_material else {
        return Err(MeltingBatchError::EmptyInput);
    };
    let Some(melting_point) = melting_point else {
        return Err(MeltingBatchError::EmptyInput);
    };
    let output = MaterialLotSpec::with_composition(
        CommodityKey::new(material, liquid_form),
        total_mass,
        melting_point,
        MaterialComposition::pure(material),
    )
    .map_err(MeltingBatchError::Output)?;
    Ok(MeltingBatchPhysics {
        material,
        melting_point,
        required_energy,
        output,
    })
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

/// Failure while resolving selected solid matter into a conserved molten production outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MeltingResolutionError {
    UnknownThermalProcess {
        process: ProcessId,
    },
    Input(ProcessInputError),
    Equipment(EquipmentProviderError),
    Capability(CapabilityEvaluationError),
    MissingHeatingPower {
        capability: CapabilityId,
    },
    MissingMaximumTemperature {
        capability: CapabilityId,
    },
    MissingMaximumBatchMass {
        capability: CapabilityId,
    },
    BatchMassExceedsEquipmentCapacity {
        selected: Mass,
        maximum: Mass,
    },
    Batch(MeltingBatchError),
    MeltingPointExceedsEquipmentMaximum {
        melting_point: Temperature,
        maximum: Temperature,
    },
    Energy(EnergySupplyError),
    WrongEnergyCarrier {
        required: EnergyCarrier,
        provided: EnergyCarrier,
    },
    Duration(PowerDurationError),
    Resolution(ProcessResolutionError),
}

impl Display for MeltingResolutionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownThermalProcess { process } => write!(
                formatter,
                "process {} has no melting resolver definition",
                process.value()
            ),
            Self::Input(error) => write!(formatter, "process input binding failed: {error}"),
            Self::Equipment(error) => write!(formatter, "equipment resolution failed: {error}"),
            Self::Capability(error) => {
                write!(formatter, "equipment capability check failed: {error}")
            }
            Self::MissingHeatingPower { capability } => write!(
                formatter,
                "equipment does not expose configured heating-power capability {}",
                capability.value()
            ),
            Self::MissingMaximumTemperature { capability } => write!(
                formatter,
                "equipment does not expose configured maximum-temperature capability {}",
                capability.value()
            ),
            Self::MissingMaximumBatchMass { capability } => write!(
                formatter,
                "equipment does not expose configured maximum-batch-mass capability {}",
                capability.value()
            ),
            Self::BatchMassExceedsEquipmentCapacity { selected, maximum } => write!(
                formatter,
                "selected batch {} mg exceeds equipment capacity {} mg",
                selected.milligrams(),
                maximum.milligrams()
            ),
            Self::Batch(error) => write!(formatter, "melting batch resolution failed: {error}"),
            Self::MeltingPointExceedsEquipmentMaximum {
                melting_point,
                maximum,
            } => write!(
                formatter,
                "material melting point {} mK exceeds equipment maximum {} mK",
                melting_point.millikelvin(),
                maximum.millikelvin()
            ),
            Self::Energy(error) => write!(formatter, "finite energy supply failed: {error}"),
            Self::WrongEnergyCarrier { required, provided } => write!(
                formatter,
                "melting process requires {required:?} energy but store provides {provided:?}"
            ),
            Self::Duration(error) => {
                write!(formatter, "melting duration calculation failed: {error}")
            }
            Self::Resolution(error) => write!(formatter, "process resolution failed: {error}"),
        }
    }
}

impl Error for MeltingResolutionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Input(error) => Some(error),
            Self::Equipment(error) => Some(error),
            Self::Capability(error) => Some(error),
            Self::Batch(error) => Some(error),
            Self::Energy(error) => Some(error),
            Self::Duration(error) => Some(error),
            Self::Resolution(error) => Some(error),
            Self::UnknownThermalProcess { .. }
            | Self::MissingHeatingPower { .. }
            | Self::MissingMaximumTemperature { .. }
            | Self::MissingMaximumBatchMass { .. }
            | Self::BatchMassExceedsEquipmentCapacity { .. }
            | Self::MeltingPointExceedsEquipmentMaximum { .. }
            | Self::WrongEnergyCarrier { .. } => None,
        }
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

    let heating_power = match provider.get_capability(definition.heating_power_capability()) {
        Some(CapabilityValue::Power(power)) => power,
        Some(_) | None => {
            return Err(MeltingResolutionError::MissingHeatingPower {
                capability: definition.heating_power_capability(),
            });
        }
    };
    let maximum_temperature = match provider.get_capability(definition.max_temperature_capability())
    {
        Some(CapabilityValue::Temperature(temperature)) => temperature,
        Some(_) | None => {
            return Err(MeltingResolutionError::MissingMaximumTemperature {
                capability: definition.max_temperature_capability(),
            });
        }
    };
    let maximum_batch_mass = match provider.get_capability(definition.max_batch_mass_capability()) {
        Some(CapabilityValue::Mass(mass)) => mass,
        Some(_) | None => {
            return Err(MeltingResolutionError::MissingMaximumBatchMass {
                capability: definition.max_batch_mass_capability(),
            });
        }
    };
    if inputs.input_mass() > maximum_batch_mass {
        return Err(MeltingResolutionError::BatchMassExceedsEquipmentCapacity {
            selected: inputs.input_mass(),
            maximum: maximum_batch_mass,
        });
    }

    let batch = resolve_melting_batch(
        registries.materials(),
        definition.liquid_form(),
        inputs.consumed_inputs(),
    )
    .map_err(MeltingResolutionError::Batch)?;
    if batch.melting_point > maximum_temperature {
        return Err(
            MeltingResolutionError::MeltingPointExceedsEquipmentMaximum {
                melting_point: batch.melting_point,
                maximum: maximum_temperature,
            },
        );
    }
    let energy_supply =
        validate_energy_supply(registries, state, energy_store, batch.required_energy)
            .map_err(MeltingResolutionError::Energy)?;
    let provided_carrier = energy_supply.trace().carrier();
    if provided_carrier != definition.energy_carrier() {
        return Err(MeltingResolutionError::WrongEnergyCarrier {
            required: definition.energy_carrier(),
            provided: provided_carrier,
        });
    }
    let transfer_power = heating_power.min(energy_supply.max_output_power());
    let duration = calculate_power_duration_ceiling(
        transfer_power,
        batch.required_energy,
        registries.core().ticks_per_second(),
    )
    .map_err(MeltingResolutionError::Duration)?;
    let equipment_condition_after = calculate_condition_after_active_ticks(
        definition.condition_wear_ppm_per_active_tick(),
        provider.condition(),
        duration,
    );
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
        required_energy: batch.required_energy,
        transfer_power,
    })
}

/// Invalid persisted melting semantics discovered during exhaustive load validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MeltingJobValidationError {
    MissingEnergy {
        job: ProductionJobId,
    },
    MissingEquipmentProvider {
        job: ProductionJobId,
    },
    UnknownEquipmentDefinition {
        job: ProductionJobId,
    },
    UnknownEnergyDefinition {
        job: ProductionJobId,
    },
    MissingHeatingPowerCapability {
        job: ProductionJobId,
    },
    MissingMaximumTemperatureCapability {
        job: ProductionJobId,
    },
    MissingMaximumBatchMassCapability {
        job: ProductionJobId,
    },
    BatchMassExceedsEquipmentCapacity {
        job: ProductionJobId,
        selected: Mass,
        maximum: Mass,
    },
    Batch {
        job: ProductionJobId,
        error: MeltingBatchError,
    },
    MeltingPointExceedsEquipmentMaximum {
        job: ProductionJobId,
        melting_point: Temperature,
        maximum: Temperature,
    },
    WrongEnergyCarrier {
        job: ProductionJobId,
        required: EnergyCarrier,
        provided: EnergyCarrier,
    },
    EnergyMismatch {
        job: ProductionJobId,
        traced: Energy,
        required: Energy,
    },
    Duration {
        job: ProductionJobId,
        error: PowerDurationError,
    },
    DurationMismatch {
        job: ProductionJobId,
        stored: TickSpan,
        required: TickSpan,
    },
    MissingEquipmentConditionOutcome {
        job: ProductionJobId,
    },
    EquipmentConditionOutcomeMismatch {
        job: ProductionJobId,
        stored: Condition,
        required: Condition,
    },
    OutputMismatch {
        job: ProductionJobId,
    },
}

impl Display for MeltingJobValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingEnergy { job } => write!(
                formatter,
                "melting job {} has no consumed energy",
                job.value()
            ),
            Self::MissingEquipmentProvider { job } => write!(
                formatter,
                "melting job {} has no equipment provider",
                job.value()
            ),
            Self::UnknownEquipmentDefinition { job } => write!(
                formatter,
                "melting job {} references unavailable equipment",
                job.value()
            ),
            Self::UnknownEnergyDefinition { job } => write!(
                formatter,
                "melting job {} references unavailable energy storage",
                job.value()
            ),
            Self::MissingHeatingPowerCapability { job } => write!(
                formatter,
                "melting job {} provider lacks heating power",
                job.value()
            ),
            Self::MissingMaximumTemperatureCapability { job } => write!(
                formatter,
                "melting job {} provider lacks maximum temperature",
                job.value()
            ),
            Self::MissingMaximumBatchMassCapability { job } => write!(
                formatter,
                "melting job {} provider lacks maximum batch mass",
                job.value()
            ),
            Self::BatchMassExceedsEquipmentCapacity {
                job,
                selected,
                maximum,
            } => write!(
                formatter,
                "melting job {} batch {} mg exceeds provider capacity {} mg",
                job.value(),
                selected.milligrams(),
                maximum.milligrams()
            ),
            Self::Batch { job, error } => write!(
                formatter,
                "melting job {} batch cannot be reproduced: {error}",
                job.value()
            ),
            Self::MeltingPointExceedsEquipmentMaximum {
                job,
                melting_point,
                maximum,
            } => write!(
                formatter,
                "melting job {} requires {} mK but provider maximum is {} mK",
                job.value(),
                melting_point.millikelvin(),
                maximum.millikelvin()
            ),
            Self::WrongEnergyCarrier {
                job,
                required,
                provided,
            } => write!(
                formatter,
                "melting job {} requires {required:?} energy but traces {provided:?}",
                job.value()
            ),
            Self::EnergyMismatch {
                job,
                traced,
                required,
            } => write!(
                formatter,
                "melting job {} traces {} nJ but physics requires {} nJ",
                job.value(),
                traced.nanojoules(),
                required.nanojoules()
            ),
            Self::Duration { job, error } => write!(
                formatter,
                "melting job {} duration cannot be recomputed: {error}",
                job.value()
            ),
            Self::DurationMismatch {
                job,
                stored,
                required,
            } => write!(
                formatter,
                "melting job {} stores {} ticks but physics requires {} ticks",
                job.value(),
                stored.value(),
                required.value()
            ),
            Self::MissingEquipmentConditionOutcome { job } => write!(
                formatter,
                "melting job {} has no post-operation equipment condition",
                job.value()
            ),
            Self::EquipmentConditionOutcomeMismatch {
                job,
                stored,
                required,
            } => write!(
                formatter,
                "melting job {} stores condition {} ppm but active-time wear requires {} ppm",
                job.value(),
                stored.parts_per_million(),
                required.parts_per_million()
            ),
            Self::OutputMismatch { job } => write!(
                formatter,
                "melting job {} molten output does not match its consumed material",
                job.value()
            ),
        }
    }
}

impl Error for MeltingJobValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Batch { error, .. } => Some(error),
            Self::Duration { error, .. } => Some(error),
            Self::MissingEnergy { .. }
            | Self::MissingEquipmentProvider { .. }
            | Self::UnknownEquipmentDefinition { .. }
            | Self::UnknownEnergyDefinition { .. }
            | Self::MissingHeatingPowerCapability { .. }
            | Self::MissingMaximumTemperatureCapability { .. }
            | Self::MissingMaximumBatchMassCapability { .. }
            | Self::BatchMassExceedsEquipmentCapacity { .. }
            | Self::MeltingPointExceedsEquipmentMaximum { .. }
            | Self::WrongEnergyCarrier { .. }
            | Self::EnergyMismatch { .. }
            | Self::DurationMismatch { .. }
            | Self::MissingEquipmentConditionOutcome { .. }
            | Self::EquipmentConditionOutcomeMismatch { .. }
            | Self::OutputMismatch { .. } => None,
        }
    }
}

pub(super) fn validate_loaded_melting_job(
    registries: &Registries,
    job: &ProductionJobRecord,
    definition: MeltingProcessDefinition,
) -> Result<(), MeltingJobValidationError> {
    let Some(consumed_energy) = job.consumed_energy() else {
        return Err(MeltingJobValidationError::MissingEnergy { job: job.id() });
    };
    let Some(provider) = job.equipment_provider() else {
        return Err(MeltingJobValidationError::MissingEquipmentProvider { job: job.id() });
    };
    let Some(equipment_definition) = registries.equipment().get_equipment(provider.definition())
    else {
        return Err(MeltingJobValidationError::UnknownEquipmentDefinition { job: job.id() });
    };
    let Some(energy_definition) = registries.energy().get_store(consumed_energy.definition())
    else {
        return Err(MeltingJobValidationError::UnknownEnergyDefinition { job: job.id() });
    };
    let heating_power = match resolve_equipment_capability(
        equipment_definition,
        provider.condition(),
        definition.heating_power_capability(),
    ) {
        Some(CapabilityValue::Power(power)) => power,
        Some(_) | None => {
            return Err(MeltingJobValidationError::MissingHeatingPowerCapability { job: job.id() });
        }
    };
    let maximum_temperature = match resolve_equipment_capability(
        equipment_definition,
        provider.condition(),
        definition.max_temperature_capability(),
    ) {
        Some(CapabilityValue::Temperature(temperature)) => temperature,
        Some(_) | None => {
            return Err(
                MeltingJobValidationError::MissingMaximumTemperatureCapability { job: job.id() },
            );
        }
    };
    let maximum_batch_mass = match resolve_equipment_capability(
        equipment_definition,
        provider.condition(),
        definition.max_batch_mass_capability(),
    ) {
        Some(CapabilityValue::Mass(mass)) => mass,
        Some(_) | None => {
            return Err(
                MeltingJobValidationError::MissingMaximumBatchMassCapability { job: job.id() },
            );
        }
    };
    if job.consumed_mass() > maximum_batch_mass {
        return Err(
            MeltingJobValidationError::BatchMassExceedsEquipmentCapacity {
                job: job.id(),
                selected: job.consumed_mass(),
                maximum: maximum_batch_mass,
            },
        );
    }
    let batch = resolve_melting_batch(
        registries.materials(),
        definition.liquid_form(),
        job.consumed_inputs(),
    )
    .map_err(|error| MeltingJobValidationError::Batch {
        job: job.id(),
        error,
    })?;
    if batch.melting_point > maximum_temperature {
        return Err(
            MeltingJobValidationError::MeltingPointExceedsEquipmentMaximum {
                job: job.id(),
                melting_point: batch.melting_point,
                maximum: maximum_temperature,
            },
        );
    }
    if consumed_energy.carrier() != definition.energy_carrier() {
        return Err(MeltingJobValidationError::WrongEnergyCarrier {
            job: job.id(),
            required: definition.energy_carrier(),
            provided: consumed_energy.carrier(),
        });
    }
    if consumed_energy.energy() != batch.required_energy {
        return Err(MeltingJobValidationError::EnergyMismatch {
            job: job.id(),
            traced: consumed_energy.energy(),
            required: batch.required_energy,
        });
    }
    let transfer_power = heating_power.min(energy_definition.max_output_power());
    let required_duration = calculate_power_duration_ceiling(
        transfer_power,
        batch.required_energy,
        registries.core().ticks_per_second(),
    )
    .map_err(|error| MeltingJobValidationError::Duration {
        job: job.id(),
        error,
    })?;
    let stored_duration = job.active_duration();
    if stored_duration != required_duration {
        return Err(MeltingJobValidationError::DurationMismatch {
            job: job.id(),
            stored: stored_duration,
            required: required_duration,
        });
    }
    let required_condition_after = calculate_condition_after_active_ticks(
        definition.condition_wear_ppm_per_active_tick(),
        provider.condition(),
        required_duration,
    );
    let Some(stored_condition_after) = job.equipment_condition_after() else {
        return Err(MeltingJobValidationError::MissingEquipmentConditionOutcome { job: job.id() });
    };
    if stored_condition_after != required_condition_after {
        return Err(
            MeltingJobValidationError::EquipmentConditionOutcomeMismatch {
                job: job.id(),
                stored: stored_condition_after,
                required: required_condition_after,
            },
        );
    }
    let Some(output_stream) = job.single_output_stream() else {
        return Err(MeltingJobValidationError::OutputMismatch { job: job.id() });
    };
    if output_stream.outputs() != [batch.output] {
        return Err(MeltingJobValidationError::OutputMismatch { job: job.id() });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{
        CapabilityComparison, CapabilityDefinition, CapabilityProfile, CapabilityRequirement,
        CapabilityValueKind,
    };
    use crate::content::{
        FORM_INGOT, FORM_MOLTEN, MATERIAL_COPPER, MATERIAL_SLAG, make_test_registries_with_melting,
    };
    use crate::core::state::{StateValidationError, validate_loaded_state};
    use crate::core::time::WorldSeed;
    use crate::energy::{
        EnergyStoreDefinition, EnergyStoreDefinitionId, add_energy_store_with_initial_for_test,
        calculate_explicit_energy_accounting,
    };
    use crate::equipment::{EquipmentDefinition, EquipmentDefinitionId, add_equipment};
    use crate::inventory::{
        StockpileStorageError, StockpileStorageProfile, add_solid_stockpile_for_test,
        add_stockpile, deposit_composed_lot_for_test, deposit_lot_for_test,
    };
    use crate::maintenance::MaintenanceThresholds;
    use crate::material::{CompositionComponent, MaterialComposition};
    use crate::matter::calculate_matter_accounting;
    use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
    use crate::production::{ProcessDefinition, StartProcessError, validate_start_process};
    use crate::simulation::advance_tick;
    use crate::thermal::ThermalJobValidationError;

    const HEATING_POWER: CapabilityId = CapabilityId::new(950_001);
    const MAX_TEMPERATURE: CapabilityId = CapabilityId::new(950_002);
    const MAX_BATCH_MASS: CapabilityId = CapabilityId::new(950_003);
    const FURNACE: EquipmentDefinitionId = EquipmentDefinitionId::new(950_001);
    const ENERGY_STORE: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(950_001);
    const PROCESS: ProcessId = ProcessId::new(950_001);
    const COPPER_MELTING_POINT: Temperature = Temperature::from_millikelvin(1_357_770);
    const INPUT_TEMPERATURE: Temperature = Temperature::from_millikelvin(300_000);

    #[derive(Clone, Copy)]
    struct FixtureIds {
        source: StockpileId,
        destination: StockpileId,
        equipment: EquipmentId,
        energy_store: EnergyStoreId,
        source_lot: crate::inventory::MaterialLotId,
    }

    struct MeltingFixture {
        registries: Registries,
        state: AppState,
        ids: FixtureIds,
    }

    fn condition(parts_per_million: u32) -> Condition {
        match Condition::new(parts_per_million) {
            Ok(condition) => condition,
            Err(error) => panic!("melting condition fixture failed: {error}"),
        }
    }

    fn make_registries(maximum_temperature: Temperature, carrier: EnergyCarrier) -> Registries {
        let profile = match CapabilityProfile::new([
            (
                HEATING_POWER,
                CapabilityValue::Power(Power::from_microwatts(20_000_000)),
            ),
            (
                MAX_TEMPERATURE,
                CapabilityValue::Temperature(maximum_temperature),
            ),
            (
                MAX_BATCH_MASS,
                CapabilityValue::Mass(Mass::from_milligrams(20)),
            ),
        ]) {
            Ok(profile) => profile,
            Err(error) => panic!("melting capability profile failed: {error}"),
        };
        let thresholds = match MaintenanceThresholds::new(condition(600_000), condition(250_000)) {
            Ok(thresholds) => thresholds,
            Err(error) => panic!("melting maintenance fixture failed: {error}"),
        };
        let equipment = EquipmentDefinition::new(
            FURNACE,
            "test induction furnace",
            Mass::from_milligrams(2_000_000),
            profile,
            thresholds,
        );
        let energy = EnergyStoreDefinition::new(
            ENERGY_STORE,
            "test melting electrical buffer",
            carrier,
            Energy::from_nanojoules(2_000_000_000_000),
            Power::from_microwatts(10_000_000),
        );
        let process = ProcessDefinition::new_selected_batch(
            PROCESS,
            "pure material melting",
            vec![
                CapabilityRequirement::new(
                    HEATING_POWER,
                    CapabilityComparison::AtLeast,
                    CapabilityValue::Power(Power::from_microwatts(1_000_000)),
                ),
                CapabilityRequirement::new(
                    MAX_TEMPERATURE,
                    CapabilityComparison::AtLeast,
                    CapabilityValue::Temperature(Temperature::from_millikelvin(1_200_000)),
                ),
                CapabilityRequirement::new(
                    MAX_BATCH_MASS,
                    CapabilityComparison::AtLeast,
                    CapabilityValue::Mass(Mass::from_milligrams(1)),
                ),
            ],
        );
        make_test_registries_with_melting(
            vec![
                CapabilityDefinition::new(
                    HEATING_POWER,
                    "melting heating power",
                    CapabilityValueKind::Power,
                ),
                CapabilityDefinition::new(
                    MAX_TEMPERATURE,
                    "melting maximum temperature",
                    CapabilityValueKind::Temperature,
                ),
                CapabilityDefinition::new(
                    MAX_BATCH_MASS,
                    "melting maximum batch mass",
                    CapabilityValueKind::Mass,
                ),
            ],
            equipment,
            energy,
            process,
            MeltingProcessDefinition::new(
                PROCESS,
                HEATING_POWER,
                MAX_TEMPERATURE,
                MAX_BATCH_MASS,
                EnergyCarrier::Electrical,
                FORM_MOLTEN,
                10,
            ),
        )
    }

    fn make_fixture(
        maximum_temperature: Temperature,
        carrier: EnergyCarrier,
        input_mass: Mass,
    ) -> MeltingFixture {
        let registries = make_registries(maximum_temperature, carrier);
        let mut state = AppState::new(WorldSeed::new(0x9500_0001));
        let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000)) {
            Ok(source) => source,
            Err(error) => panic!("melting source fixture failed: {error}"),
        };
        let vessel_profile = match StockpileStorageProfile::new(
            false,
            true,
            Temperature::from_millikelvin(1_500_000),
        ) {
            Ok(profile) => profile,
            Err(error) => panic!("melting vessel profile failed: {error}"),
        };
        let destination =
            match add_stockpile(&mut state, Mass::from_milligrams(1_000), vessel_profile) {
                Ok(destination) => destination,
                Err(error) => panic!("melting vessel fixture failed: {error}"),
            };
        let source_lot = match deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_COPPER, FORM_INGOT),
            input_mass,
            INPUT_TEMPERATURE,
        ) {
            Ok(lot) => lot,
            Err(error) => panic!("melting copper fixture failed: {error}"),
        };
        let equipment = match add_equipment(&registries, &mut state, FURNACE, Condition::PRISTINE) {
            Ok(equipment) => equipment,
            Err(error) => panic!("melting equipment fixture failed: {error}"),
        };
        let energy_store = match add_energy_store_with_initial_for_test(
            &registries,
            &mut state,
            ENERGY_STORE,
            Energy::from_nanojoules(1_000_000_000_000),
        ) {
            Ok(store) => store,
            Err(error) => panic!("melting energy fixture failed: {error}"),
        };
        MeltingFixture {
            registries,
            state,
            ids: FixtureIds {
                source,
                destination,
                equipment,
                energy_store,
                source_lot,
            },
        }
    }

    fn resolve_selected(
        registries: &Registries,
        state: &AppState,
        ids: FixtureIds,
        mass: Mass,
    ) -> Result<ResolvedMelting, MeltingResolutionError> {
        resolve_melting_process(
            registries,
            state,
            MeltingRequest::new(
                PROCESS,
                ids.source,
                &[MaterialLotSelection::new(ids.source_lot, mass)],
                ids.equipment,
                ids.energy_store,
            ),
        )
    }

    fn explicit_energy_total(registries: &Registries, state: &AppState) -> Energy {
        match calculate_explicit_energy_accounting(registries, state).and_then(|accounting| {
            accounting
                .total()
                .ok_or(crate::energy::ExplicitEnergyAccountingError::Overflow)
        }) {
            Ok(total) => total,
            Err(error) => panic!("explicit energy accounting failed: {error}"),
        }
    }

    fn matter_total(state: &AppState) -> crate::core::quantity::AggregateMass {
        match calculate_matter_accounting(state) {
            Ok(accounting) => accounting.total(),
            Err(error) => panic!("matter accounting failed: {error}"),
        }
    }

    #[cfg(feature = "test-soak")]
    fn commit_one_melt(
        registries: &Registries,
        state: &mut AppState,
        ids: FixtureIds,
        mass: Mass,
    ) -> TickSpan {
        let resolved = match resolve_selected(registries, state, ids, mass) {
            Ok(resolved) => resolved,
            Err(error) => panic!("melting resolution failed: {error}"),
        };
        let duration = resolved.process_resolution().duration();
        let token = match validate_start_process(
            registries,
            state,
            resolved.process_resolution(),
            ids.source,
            ids.destination,
        ) {
            Ok(token) => token,
            Err(error) => panic!("melting start validation failed: {error}"),
        };
        if let Err(error) = token.commit(state) {
            panic!("melting start commit failed: {error}");
        }
        for _ in 0..duration.value() {
            if let Err(error) = advance_tick(registries, state) {
                panic!("melting completion tick failed: {error}");
            }
        }
        duration
    }

    #[test]
    fn melting_resolves_exact_sensible_plus_latent_energy_and_molten_output() {
        let fixture = make_fixture(
            Temperature::from_millikelvin(1_500_000),
            EnergyCarrier::Electrical,
            Mass::from_milligrams(10),
        );
        let sensible = match calculate_sensible_heat(
            fixture.registries.materials(),
            Mass::from_milligrams(10),
            &MaterialComposition::pure(MATERIAL_COPPER),
            INPUT_TEMPERATURE,
            COPPER_MELTING_POINT,
        ) {
            Ok(heat) => heat.energy(),
            Err(error) => panic!("melting sensible fixture failed: {error}"),
        };
        let latent = match calculate_fusion_heat(
            fixture.registries.materials(),
            Mass::from_milligrams(10),
            MATERIAL_COPPER,
        ) {
            Ok(heat) => heat.energy(),
            Err(error) => panic!("melting latent fixture failed: {error}"),
        };
        let expected_energy = match sensible.checked_add(latent) {
            Some(energy) => energy,
            None => panic!("melting expected energy overflowed"),
        };

        let resolved = match resolve_selected(
            &fixture.registries,
            &fixture.state,
            fixture.ids,
            Mass::from_milligrams(10),
        ) {
            Ok(resolved) => resolved,
            Err(error) => panic!("melting resolution failed: {error}"),
        };

        assert_eq!(resolved.material(), MATERIAL_COPPER);
        assert_eq!(resolved.melting_point(), COPPER_MELTING_POINT);
        assert_eq!(resolved.required_energy(), expected_energy);
        assert_eq!(
            resolved.transfer_power(),
            Power::from_microwatts(10_000_000)
        );
        assert_eq!(resolved.process_resolution().outputs().len(), 1);
        let output = &resolved.process_resolution().outputs()[0];
        assert_eq!(
            output.commodity(),
            CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN)
        );
        assert_eq!(output.mass(), Mass::from_milligrams(10));
        assert_eq!(output.temperature(), COPPER_MELTING_POINT);
        assert_eq!(
            output.composition(),
            &MaterialComposition::pure(MATERIAL_COPPER)
        );
    }

    #[test]
    fn melting_requires_liquid_capable_destination_storage() {
        let fixture = make_fixture(
            Temperature::from_millikelvin(1_500_000),
            EnergyCarrier::Electrical,
            Mass::from_milligrams(10),
        );
        let mut state = fixture.state;
        let bad_destination =
            match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
                Ok(stockpile) => stockpile,
                Err(error) => panic!("solid destination fixture failed: {error}"),
            };
        let resolved = match resolve_selected(
            &fixture.registries,
            &state,
            fixture.ids,
            Mass::from_milligrams(10),
        ) {
            Ok(resolved) => resolved,
            Err(error) => panic!("melting resolution failed: {error}"),
        };
        let before = state.clone();

        assert_eq!(
            validate_start_process(
                &fixture.registries,
                &state,
                resolved.process_resolution(),
                fixture.ids.source,
                bad_destination,
            ),
            Err(StartProcessError::DestinationStorage(
                StockpileStorageError::PhaseNotAccepted {
                    stockpile: bad_destination,
                    phase: MaterialPhase::Liquid,
                }
            ))
        );
        assert_eq!(state, before);
    }

    #[test]
    fn melting_preserves_matter_and_modeled_energy_through_save_resume_and_completion() {
        let mut fixture = make_fixture(
            Temperature::from_millikelvin(1_500_000),
            EnergyCarrier::Electrical,
            Mass::from_milligrams(10),
        );
        let initial_matter = matter_total(&fixture.state);
        let initial_energy = explicit_energy_total(&fixture.registries, &fixture.state);
        let resolved = match resolve_selected(
            &fixture.registries,
            &fixture.state,
            fixture.ids,
            Mass::from_milligrams(10),
        ) {
            Ok(resolved) => resolved,
            Err(error) => panic!("melting resolution failed: {error}"),
        };
        let duration = resolved.process_resolution().duration();
        let token = match validate_start_process(
            &fixture.registries,
            &fixture.state,
            resolved.process_resolution(),
            fixture.ids.source,
            fixture.ids.destination,
        ) {
            Ok(token) => token,
            Err(error) => panic!("melting start validation failed: {error}"),
        };
        let job = match token.commit(&mut fixture.state) {
            Ok(job) => job,
            Err(error) => panic!("melting start commit failed: {error}"),
        };
        assert_eq!(matter_total(&fixture.state), initial_matter);
        assert_eq!(
            explicit_energy_total(&fixture.registries, &fixture.state),
            initial_energy
        );
        assert_eq!(
            validate_loaded_state(&fixture.registries, &fixture.state),
            Ok(())
        );

        let encoded =
            match serde_json::to_vec(&SaveEnvelope::new(&fixture.registries, &fixture.state)) {
                Ok(encoded) => encoded,
                Err(error) => panic!("melting save serialization failed: {error}"),
            };
        let decoded: LoadedSaveEnvelope = match serde_json::from_slice(&encoded) {
            Ok(decoded) => decoded,
            Err(error) => panic!("melting save deserialization failed: {error}"),
        };
        let mut resumed = match decoded.into_state(&fixture.registries) {
            Ok(state) => state,
            Err(error) => panic!("melting save validation failed: {error}"),
        };
        let mut uninterrupted = fixture.state;
        assert_eq!(resumed, uninterrupted);

        for _ in 0..duration.value() {
            let first = match advance_tick(&fixture.registries, &mut uninterrupted) {
                Ok(outcome) => outcome,
                Err(error) => panic!("uninterrupted melting continuation failed: {error}"),
            };
            let second = match advance_tick(&fixture.registries, &mut resumed) {
                Ok(outcome) => outcome,
                Err(error) => panic!("resumed melting continuation failed: {error}"),
            };
            assert_eq!(first, second);
        }
        assert_eq!(resumed, uninterrupted);
        assert!(resumed.production().get_job(job).is_none());
        assert_eq!(matter_total(&resumed), initial_matter);
        assert_eq!(
            explicit_energy_total(&fixture.registries, &resumed),
            initial_energy
        );
        let output = match resumed
            .inventory()
            .lots()
            .find(|lot| lot.stockpile() == fixture.ids.destination)
        {
            Some(output) => output,
            None => panic!("molten output lot missing after completion"),
        };
        assert_eq!(
            output.commodity(),
            CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN)
        );
        assert_eq!(output.mass(), Mass::from_milligrams(10));
        assert_eq!(output.temperature(), COPPER_MELTING_POINT);
    }

    #[test]
    fn melting_rejects_impure_input_and_insufficient_furnace_temperature() {
        let mut fixture = make_fixture(
            Temperature::from_millikelvin(1_500_000),
            EnergyCarrier::Electrical,
            Mass::from_milligrams(10),
        );
        let mixed = match MaterialComposition::new(vec![
            CompositionComponent::new(MATERIAL_COPPER, 500_000),
            CompositionComponent::new(MATERIAL_SLAG, 500_000),
        ]) {
            Ok(composition) => composition,
            Err(error) => panic!("mixed melting fixture failed: {error}"),
        };
        let mixed_lot = match deposit_composed_lot_for_test(
            &fixture.registries,
            &mut fixture.state,
            fixture.ids.source,
            CommodityKey::new(MATERIAL_COPPER, FORM_INGOT),
            Mass::from_milligrams(5),
            INPUT_TEMPERATURE,
            mixed,
        ) {
            Ok(lot) => lot,
            Err(error) => panic!("mixed melting lot fixture failed: {error}"),
        };
        assert!(matches!(
            resolve_melting_process(
                &fixture.registries,
                &fixture.state,
                MeltingRequest::new(
                    PROCESS,
                    fixture.ids.source,
                    &[MaterialLotSelection::new(
                        mixed_lot,
                        Mass::from_milligrams(5)
                    )],
                    fixture.ids.equipment,
                    fixture.ids.energy_store,
                ),
            ),
            Err(MeltingResolutionError::Batch(
                MeltingBatchError::ImpureInput { .. }
            ))
        ));

        let cool_fixture = make_fixture(
            Temperature::from_millikelvin(1_300_000),
            EnergyCarrier::Electrical,
            Mass::from_milligrams(10),
        );
        assert_eq!(
            resolve_selected(
                &cool_fixture.registries,
                &cool_fixture.state,
                cool_fixture.ids,
                Mass::from_milligrams(10),
            ),
            Err(
                MeltingResolutionError::MeltingPointExceedsEquipmentMaximum {
                    melting_point: COPPER_MELTING_POINT,
                    maximum: Temperature::from_millikelvin(1_300_000),
                }
            )
        );
    }

    #[test]
    fn melting_job_tampering_is_rejected_by_physics_and_destination_audits() {
        let mut fixture = make_fixture(
            Temperature::from_millikelvin(1_500_000),
            EnergyCarrier::Electrical,
            Mass::from_milligrams(10),
        );
        let resolved = match resolve_selected(
            &fixture.registries,
            &fixture.state,
            fixture.ids,
            Mass::from_milligrams(10),
        ) {
            Ok(resolved) => resolved,
            Err(error) => panic!("melting resolution failed: {error}"),
        };
        let required_energy = resolved.required_energy();
        let token = match validate_start_process(
            &fixture.registries,
            &fixture.state,
            resolved.process_resolution(),
            fixture.ids.source,
            fixture.ids.destination,
        ) {
            Ok(token) => token,
            Err(error) => panic!("melting start validation failed: {error}"),
        };
        let job = match token.commit(&mut fixture.state) {
            Ok(job) => job,
            Err(error) => panic!("melting start commit failed: {error}"),
        };

        let mut tampered_energy =
            match serde_json::to_value(SaveEnvelope::new(&fixture.registries, &fixture.state)) {
                Ok(encoded) => encoded,
                Err(error) => panic!("melting energy tamper serialization failed: {error}"),
            };
        tampered_energy["state"]["systems"]["production"]["jobs"][job.value().to_string()]["resources"]
            ["consumed_energy"]["energy"] = serde_json::json!(1_u64);
        let tampered_energy: LoadedSaveEnvelope = match serde_json::from_value(tampered_energy) {
            Ok(decoded) => decoded,
            Err(error) => panic!("melting energy tamper failed decode: {error}"),
        };
        assert_eq!(
            tampered_energy.into_state(&fixture.registries),
            Err(LoadError::InvalidState(StateValidationError::ThermalJob(
                ThermalJobValidationError::Melting(MeltingJobValidationError::EnergyMismatch {
                    job,
                    traced: Energy::from_nanojoules(1),
                    required: required_energy,
                })
            )))
        );

        let mut invalid_destination =
            match serde_json::to_value(SaveEnvelope::new(&fixture.registries, &fixture.state)) {
                Ok(encoded) => encoded,
                Err(error) => panic!("melting destination tamper serialization failed: {error}"),
            };
        let destination = fixture.ids.destination.value().to_string();
        invalid_destination["state"]["systems"]["inventory"]["stockpiles"][destination.clone()]["storage_profile"]
            ["can_store_liquid"] = serde_json::json!(false);
        invalid_destination["state"]["systems"]["inventory"]["stockpiles"][destination]["storage_profile"]
            ["can_store_solid"] = serde_json::json!(true);
        let invalid_destination: LoadedSaveEnvelope =
            match serde_json::from_value(invalid_destination) {
                Ok(decoded) => decoded,
                Err(error) => panic!("melting destination tamper failed decode: {error}"),
            };
        assert_eq!(
            invalid_destination.into_state(&fixture.registries),
            Err(LoadError::InvalidState(
                StateValidationError::JobOutputStorage {
                    job,
                    error: StockpileStorageError::PhaseNotAccepted {
                        stockpile: fixture.ids.destination,
                        phase: MaterialPhase::Liquid,
                    },
                }
            ))
        );
    }

    #[cfg(feature = "test-soak")]
    #[test]
    fn small_melt_soak_preserves_conservation_and_deterministic_replay() {
        let fixture = make_fixture(
            Temperature::from_millikelvin(1_500_000),
            EnergyCarrier::Electrical,
            Mass::from_milligrams(500),
        );
        let initial_matter = matter_total(&fixture.state);
        let initial_energy = explicit_energy_total(&fixture.registries, &fixture.state);
        let mut first = fixture.state.clone();
        let mut second = fixture.state;

        for step in 0..500_u64 {
            let first_duration = commit_one_melt(
                &fixture.registries,
                &mut first,
                fixture.ids,
                Mass::from_milligrams(1),
            );
            let second_duration = commit_one_melt(
                &fixture.registries,
                &mut second,
                fixture.ids,
                Mass::from_milligrams(1),
            );
            assert_eq!(first_duration, second_duration);
            if step % 73 == 0 {
                assert_eq!(validate_loaded_state(&fixture.registries, &first), Ok(()));
                assert_eq!(matter_total(&first), initial_matter);
                assert_eq!(
                    explicit_energy_total(&fixture.registries, &first),
                    initial_energy
                );
            }
        }

        assert_eq!(first, second);
        assert_eq!(matter_total(&first), initial_matter);
        assert_eq!(
            explicit_energy_total(&fixture.registries, &first),
            initial_energy
        );
        let molten_mass = first
            .inventory()
            .get_stockpile(fixture.ids.destination)
            .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN)));
        assert_eq!(molten_mass, Some(Mass::from_milligrams(500)));
    }
}
