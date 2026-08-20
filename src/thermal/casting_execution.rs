//! Pure-material casting/solidification with exact heat release into a finite thermal-energy sink.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::{
    CapabilityEvaluationError, CapabilityId, CapabilityValue, evaluate_capabilities,
};
use crate::core::quantity::{Energy, Mass, Power, Temperature};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::energy::{
    EnergyCarrier, EnergySinkError, EnergyStoreId, PowerDurationError,
    calculate_power_duration_ceiling, validate_energy_sink,
};
use crate::equipment::{
    EquipmentId, EquipmentProviderError, resolve_equipment_capability, resolve_equipment_provider,
};
use crate::inventory::{ConsumedMaterialTrace, MaterialLotSelection, StockpileId};
use crate::maintenance::{
    ActiveConditionDurationError, Condition, assert_valid_condition_wear_ppm_per_tick,
    calculate_usable_condition_after_active_ticks,
};
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

/// Immutable declaration that one selected-batch process solidifies pure liquid matter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CastingProcessDefinition {
    process: ProcessId,
    cooling_power_capability: CapabilityId,
    max_temperature_capability: CapabilityId,
    max_batch_mass_capability: CapabilityId,
    energy_carrier: EnergyCarrier,
    solid_form: FormId,
    condition_wear_ppm_per_active_tick: u32,
}

impl CastingProcessDefinition {
    #[must_use]
    pub const fn new(
        process: ProcessId,
        cooling_power_capability: CapabilityId,
        max_temperature_capability: CapabilityId,
        max_batch_mass_capability: CapabilityId,
        energy_carrier: EnergyCarrier,
        solid_form: FormId,
        condition_wear_ppm_per_active_tick: u32,
    ) -> Self {
        assert_valid_condition_wear_ppm_per_tick(condition_wear_ppm_per_active_tick);
        Self {
            process,
            cooling_power_capability,
            max_temperature_capability,
            max_batch_mass_capability,
            energy_carrier,
            solid_form,
            condition_wear_ppm_per_active_tick,
        }
    }

    #[must_use]
    pub const fn process(self) -> ProcessId {
        self.process
    }

    #[must_use]
    pub const fn cooling_power_capability(self) -> CapabilityId {
        self.cooling_power_capability
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
    pub const fn solid_form(self) -> FormId {
        self.solid_form
    }

    #[must_use]
    pub const fn condition_wear_ppm_per_active_tick(self) -> u32 {
        self.condition_wear_ppm_per_active_tick
    }
}

/// Failure while deriving solidification physics from exact consumed liquid traces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CastingBatchError {
    EmptyInput,
    UnknownInputForm {
        form: FormId,
    },
    InputNotLiquid {
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
    InputBelowMeltingPoint {
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

impl Display for CastingBatchError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyInput => formatter.write_str("casting batch contains no material"),
            Self::UnknownInputForm { form } => {
                write!(
                    formatter,
                    "casting batch references unknown form {}",
                    form.value()
                )
            }
            Self::InputNotLiquid { form, phase } => write!(
                formatter,
                "casting input form {} is {phase:?} rather than liquid",
                form.value()
            ),
            Self::ImpureInput { commodity } => write!(
                formatter,
                "casting input commodity {} is compositionally mixed; alloy solidification diagrams are not modeled",
                commodity.value()
            ),
            Self::PureMaterialDoesNotMatchCommodity { commodity, pure } => write!(
                formatter,
                "casting input commodity {} hosts material {} but its pure composition is material {}",
                commodity.value(),
                commodity.material().value(),
                pure.value()
            ),
            Self::MixedMaterials { expected, found } => write!(
                formatter,
                "casting batch mixes material {} with material {}; alloy solidification requires a dedicated resolver",
                expected.value(),
                found.value()
            ),
            Self::InputBelowMeltingPoint {
                material,
                current,
                melting_point,
            } => write!(
                formatter,
                "liquid material {} is at {} mK below its {} mK melting point",
                material.value(),
                current.millikelvin(),
                melting_point.millikelvin()
            ),
            Self::SensibleHeat { material, error } => write!(
                formatter,
                "casting material {} cannot be cooled to its fusion boundary: {error}",
                material.value()
            ),
            Self::FusionHeat { material, error } => write!(
                formatter,
                "casting material {} cannot resolve latent heat: {error}",
                material.value()
            ),
            Self::EnergyOverflow => {
                formatter.write_str("casting heat-release requirement overflowed")
            }
            Self::MassOverflow => formatter.write_str("casting batch mass overflowed"),
            Self::Output(error) => write!(
                formatter,
                "solid casting output construction failed: {error}"
            ),
        }
    }
}

impl Error for CastingBatchError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::SensibleHeat {
                material: _material,
                error,
            } => Some(error),
            Self::FusionHeat {
                material: _material,
                error,
            } => Some(error),
            Self::Output(error) => Some(error),
            Self::UnknownInputForm { form: _form } => None,
            Self::InputNotLiquid {
                form: _form,
                phase: _phase,
            } => None,
            Self::ImpureInput {
                commodity: _commodity,
            } => None,
            Self::PureMaterialDoesNotMatchCommodity {
                commodity: _commodity,
                pure: _pure,
            } => None,
            Self::MixedMaterials {
                expected: _expected,
                found: _found,
            } => None,
            Self::InputBelowMeltingPoint {
                material: _material,
                current: _current,
                melting_point: _melting_point,
            } => None,
            Self::EmptyInput | Self::EnergyOverflow | Self::MassOverflow => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CastingBatchPhysics {
    material: MaterialId,
    melting_point: Temperature,
    hottest_input: Temperature,
    released_energy: Energy,
    output: MaterialLotSpec,
}

fn resolve_casting_batch(
    materials: &MaterialRegistry,
    solid_form: FormId,
    traces: &[ConsumedMaterialTrace],
) -> Result<CastingBatchPhysics, CastingBatchError> {
    let mut batch_material = None;
    let mut melting_point = None;
    let mut hottest_input = Temperature::ZERO;
    let mut total_mass = Mass::ZERO;
    let mut released_energy = Energy::ZERO;

    for trace in traces {
        let profile = trace.profile();
        let form_id = profile.commodity().form();
        let Some(form) = materials.get_form(form_id) else {
            return Err(CastingBatchError::UnknownInputForm { form: form_id });
        };
        if form.phase() != MaterialPhase::Liquid {
            return Err(CastingBatchError::InputNotLiquid {
                form: form_id,
                phase: form.phase(),
            });
        }
        let Some(material) = profile.composition().pure_material() else {
            return Err(CastingBatchError::ImpureInput {
                commodity: profile.commodity(),
            });
        };
        if profile.commodity().material() != material {
            return Err(CastingBatchError::PureMaterialDoesNotMatchCommodity {
                commodity: profile.commodity(),
                pure: material,
            });
        }
        if let Some(expected) = batch_material {
            if expected != material {
                return Err(CastingBatchError::MixedMaterials {
                    expected,
                    found: material,
                });
            }
        } else {
            batch_material = Some(material);
        }

        let fusion = calculate_fusion_heat(materials, trace.mass(), material)
            .map_err(|error| CastingBatchError::FusionHeat { material, error })?;
        if profile.temperature() < fusion.melting_point() {
            return Err(CastingBatchError::InputBelowMeltingPoint {
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
        hottest_input = hottest_input.max(profile.temperature());
        let sensible = calculate_sensible_heat(
            materials,
            trace.mass(),
            profile.composition(),
            profile.temperature(),
            fusion.melting_point(),
        )
        .map_err(|error| CastingBatchError::SensibleHeat { material, error })?;
        released_energy = released_energy
            .checked_add(sensible.energy())
            .and_then(|energy| energy.checked_add(fusion.energy()))
            .ok_or(CastingBatchError::EnergyOverflow)?;
        total_mass = total_mass
            .checked_add(trace.mass())
            .ok_or(CastingBatchError::MassOverflow)?;
    }

    let Some(material) = batch_material else {
        return Err(CastingBatchError::EmptyInput);
    };
    let Some(melting_point) = melting_point else {
        return Err(CastingBatchError::EmptyInput);
    };
    let output = MaterialLotSpec::with_composition(
        CommodityKey::new(material, solid_form),
        total_mass,
        melting_point,
        MaterialComposition::pure(material),
    )
    .map_err(CastingBatchError::Output)?;
    Ok(CastingBatchPhysics {
        material,
        melting_point,
        hottest_input,
        released_energy,
        output,
    })
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

/// Failure while resolving selected liquid matter into a conserved solid casting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CastingResolutionError {
    UnknownThermalProcess {
        process: ProcessId,
    },
    Input(ProcessInputError),
    Equipment(EquipmentProviderError),
    Capability(CapabilityEvaluationError),
    MissingCoolingPower {
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
    Batch(CastingBatchError),
    InputTemperatureExceedsEquipmentMaximum {
        input: Temperature,
        maximum: Temperature,
    },
    EnergySink(EnergySinkError),
    WrongEnergyCarrier {
        required: EnergyCarrier,
        provided: EnergyCarrier,
    },
    Duration(PowerDurationError),
    ConditionDuration(ActiveConditionDurationError),
    Resolution(ProcessResolutionError),
}

impl Display for CastingResolutionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownThermalProcess { process } => write!(
                formatter,
                "process {} has no casting resolver definition",
                process.value()
            ),
            Self::Input(error) => write!(formatter, "process input binding failed: {error}"),
            Self::Equipment(error) => write!(formatter, "equipment resolution failed: {error}"),
            Self::Capability(error) => {
                write!(formatter, "equipment capability check failed: {error}")
            }
            Self::MissingCoolingPower { capability } => write!(
                formatter,
                "equipment does not expose configured cooling-power capability {}",
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
            Self::Batch(error) => write!(formatter, "casting batch resolution failed: {error}"),
            Self::InputTemperatureExceedsEquipmentMaximum { input, maximum } => write!(
                formatter,
                "casting input temperature {} mK exceeds equipment maximum {} mK",
                input.millikelvin(),
                maximum.millikelvin()
            ),
            Self::EnergySink(error) => write!(formatter, "finite thermal sink failed: {error}"),
            Self::WrongEnergyCarrier { required, provided } => write!(
                formatter,
                "casting process releases {required:?} energy but sink stores {provided:?}"
            ),
            Self::Duration(error) => {
                write!(formatter, "casting duration calculation failed: {error}")
            }
            Self::ConditionDuration(error) => {
                write!(
                    formatter,
                    "casting exceeds equipment condition lifetime: {error}"
                )
            }
            Self::Resolution(error) => write!(formatter, "process resolution failed: {error}"),
        }
    }
}

impl Error for CastingResolutionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Input(error) => Some(error),
            Self::Equipment(error) => Some(error),
            Self::Capability(error) => Some(error),
            Self::Batch(error) => Some(error),
            Self::EnergySink(error) => Some(error),
            Self::Duration(error) => Some(error),
            Self::ConditionDuration(error) => Some(error),
            Self::Resolution(error) => Some(error),
            Self::UnknownThermalProcess { process: _process } => None,
            Self::MissingCoolingPower {
                capability: _capability,
            }
            | Self::MissingMaximumTemperature {
                capability: _capability,
            }
            | Self::MissingMaximumBatchMass {
                capability: _capability,
            } => None,
            Self::BatchMassExceedsEquipmentCapacity {
                selected: _selected,
                maximum: _maximum,
            } => None,
            Self::InputTemperatureExceedsEquipmentMaximum {
                input: _input,
                maximum: _maximum,
            } => None,
            Self::WrongEnergyCarrier {
                required: _required,
                provided: _provided,
            } => None,
        }
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

    let cooling_power = match provider.get_capability(definition.cooling_power_capability()) {
        Some(CapabilityValue::Power(power)) => power,
        Some(_) | None => {
            return Err(CastingResolutionError::MissingCoolingPower {
                capability: definition.cooling_power_capability(),
            });
        }
    };
    let maximum_temperature = match provider.get_capability(definition.max_temperature_capability())
    {
        Some(CapabilityValue::Temperature(temperature)) => temperature,
        Some(_) | None => {
            return Err(CastingResolutionError::MissingMaximumTemperature {
                capability: definition.max_temperature_capability(),
            });
        }
    };
    let maximum_batch_mass = match provider.get_capability(definition.max_batch_mass_capability()) {
        Some(CapabilityValue::Mass(mass)) => mass,
        Some(_) | None => {
            return Err(CastingResolutionError::MissingMaximumBatchMass {
                capability: definition.max_batch_mass_capability(),
            });
        }
    };
    if inputs.input_mass() > maximum_batch_mass {
        return Err(CastingResolutionError::BatchMassExceedsEquipmentCapacity {
            selected: inputs.input_mass(),
            maximum: maximum_batch_mass,
        });
    }

    let batch = resolve_casting_batch(
        registries.materials(),
        definition.solid_form(),
        inputs.consumed_inputs(),
    )
    .map_err(CastingResolutionError::Batch)?;
    if batch.hottest_input > maximum_temperature {
        return Err(
            CastingResolutionError::InputTemperatureExceedsEquipmentMaximum {
                input: batch.hottest_input,
                maximum: maximum_temperature,
            },
        );
    }
    let energy_sink = validate_energy_sink(registries, state, energy_sink, batch.released_energy)
        .map_err(CastingResolutionError::EnergySink)?;
    let provided_carrier = energy_sink.trace().carrier();
    if provided_carrier != definition.energy_carrier() {
        return Err(CastingResolutionError::WrongEnergyCarrier {
            required: definition.energy_carrier(),
            provided: provided_carrier,
        });
    }
    let transfer_power = cooling_power.min(energy_sink.max_input_power());
    let duration = calculate_power_duration_ceiling(
        transfer_power,
        batch.released_energy,
        registries.core().physical_tick_duration(),
    )
    .map_err(CastingResolutionError::Duration)?;
    let equipment_condition_after = calculate_usable_condition_after_active_ticks(
        definition.condition_wear_ppm_per_active_tick(),
        provider.condition(),
        duration,
    )
    .map_err(CastingResolutionError::ConditionDuration)?;
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
        released_energy: batch.released_energy,
        transfer_power,
    })
}

/// Invalid persisted casting semantics discovered during exhaustive load validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CastingJobValidationError {
    UnexpectedConsumedEnergy {
        job: ProductionJobId,
    },
    MissingReleasedEnergy {
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
    MissingCoolingPowerCapability {
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
        error: CastingBatchError,
    },
    InputTemperatureExceedsEquipmentMaximum {
        job: ProductionJobId,
        input: Temperature,
        maximum: Temperature,
    },
    WrongEnergyCarrier {
        job: ProductionJobId,
        required: EnergyCarrier,
        provided: EnergyCarrier,
    },
    ReleasedEnergyMismatch {
        job: ProductionJobId,
        traced: Energy,
        required: Energy,
    },
    Duration {
        job: ProductionJobId,
        error: PowerDurationError,
    },
    ConditionDuration {
        job: ProductionJobId,
        error: ActiveConditionDurationError,
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

impl Display for CastingJobValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnexpectedConsumedEnergy { job } => write!(
                formatter,
                "casting job {} unexpectedly consumes finite energy",
                job.value()
            ),
            Self::MissingReleasedEnergy { job } => write!(
                formatter,
                "casting job {} has no released-energy trace",
                job.value()
            ),
            Self::MissingEquipmentProvider { job } => write!(
                formatter,
                "casting job {} has no equipment provider",
                job.value()
            ),
            Self::UnknownEquipmentDefinition { job } => write!(
                formatter,
                "casting job {} references unavailable equipment",
                job.value()
            ),
            Self::UnknownEnergyDefinition { job } => write!(
                formatter,
                "casting job {} references unavailable thermal sink definition",
                job.value()
            ),
            Self::MissingCoolingPowerCapability { job } => write!(
                formatter,
                "casting job {} provider lacks cooling power",
                job.value()
            ),
            Self::MissingMaximumTemperatureCapability { job } => write!(
                formatter,
                "casting job {} provider lacks maximum temperature",
                job.value()
            ),
            Self::MissingMaximumBatchMassCapability { job } => write!(
                formatter,
                "casting job {} provider lacks maximum batch mass",
                job.value()
            ),
            Self::BatchMassExceedsEquipmentCapacity {
                job,
                selected,
                maximum,
            } => write!(
                formatter,
                "casting job {} batch {} mg exceeds provider capacity {} mg",
                job.value(),
                selected.milligrams(),
                maximum.milligrams()
            ),
            Self::Batch { job, error } => write!(
                formatter,
                "casting job {} batch cannot be reproduced: {error}",
                job.value()
            ),
            Self::InputTemperatureExceedsEquipmentMaximum {
                job,
                input,
                maximum,
            } => write!(
                formatter,
                "casting job {} input {} mK exceeds provider maximum {} mK",
                job.value(),
                input.millikelvin(),
                maximum.millikelvin()
            ),
            Self::WrongEnergyCarrier {
                job,
                required,
                provided,
            } => write!(
                formatter,
                "casting job {} releases {required:?} energy but traces {provided:?}",
                job.value()
            ),
            Self::ReleasedEnergyMismatch {
                job,
                traced,
                required,
            } => write!(
                formatter,
                "casting job {} traces {} nJ released but physics requires {} nJ",
                job.value(),
                traced.nanojoules(),
                required.nanojoules()
            ),
            Self::Duration { job, error } => write!(
                formatter,
                "casting job {} duration cannot be recomputed: {error}",
                job.value()
            ),
            Self::ConditionDuration { job, error } => write!(
                formatter,
                "casting job {} exceeds equipment condition lifetime: {error}",
                job.value()
            ),
            Self::DurationMismatch {
                job,
                stored,
                required,
            } => write!(
                formatter,
                "casting job {} stores {} ticks but physics requires {} ticks",
                job.value(),
                stored.value(),
                required.value()
            ),
            Self::MissingEquipmentConditionOutcome { job } => write!(
                formatter,
                "casting job {} has no post-operation equipment condition",
                job.value()
            ),
            Self::EquipmentConditionOutcomeMismatch {
                job,
                stored,
                required,
            } => write!(
                formatter,
                "casting job {} stores condition {} ppm but active-time wear requires {} ppm",
                job.value(),
                stored.parts_per_million(),
                required.parts_per_million()
            ),
            Self::OutputMismatch { job } => write!(
                formatter,
                "casting job {} solid output does not match its consumed liquid material",
                job.value()
            ),
        }
    }
}

impl Error for CastingJobValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Batch { job: _job, error } => Some(error),
            Self::Duration { job: _job, error } => Some(error),
            Self::ConditionDuration { job: _job, error } => Some(error),
            Self::UnexpectedConsumedEnergy { job: _job }
            | Self::MissingReleasedEnergy { job: _job }
            | Self::MissingEquipmentProvider { job: _job }
            | Self::UnknownEquipmentDefinition { job: _job }
            | Self::UnknownEnergyDefinition { job: _job }
            | Self::MissingCoolingPowerCapability { job: _job }
            | Self::MissingMaximumTemperatureCapability { job: _job }
            | Self::MissingMaximumBatchMassCapability { job: _job }
            | Self::MissingEquipmentConditionOutcome { job: _job }
            | Self::OutputMismatch { job: _job } => None,
            Self::BatchMassExceedsEquipmentCapacity {
                job: _job,
                selected: _selected,
                maximum: _maximum,
            } => None,
            Self::InputTemperatureExceedsEquipmentMaximum {
                job: _job,
                input: _input,
                maximum: _maximum,
            } => None,
            Self::WrongEnergyCarrier {
                job: _job,
                required: _required,
                provided: _provided,
            } => None,
            Self::ReleasedEnergyMismatch {
                job: _job,
                traced: _traced,
                required: _required,
            } => None,
            Self::DurationMismatch {
                job: _job,
                stored: _stored,
                required: _required,
            } => None,
            Self::EquipmentConditionOutcomeMismatch {
                job: _job,
                stored: _stored,
                required: _required,
            } => None,
        }
    }
}

pub(super) fn validate_loaded_casting_job(
    registries: &Registries,
    job: &ProductionJobRecord,
    definition: CastingProcessDefinition,
) -> Result<(), CastingJobValidationError> {
    if job.consumed_energy().is_some() {
        return Err(CastingJobValidationError::UnexpectedConsumedEnergy { job: job.id() });
    }
    let Some(released_energy) = job.released_energy() else {
        return Err(CastingJobValidationError::MissingReleasedEnergy { job: job.id() });
    };
    let Some(provider) = job.equipment_provider() else {
        return Err(CastingJobValidationError::MissingEquipmentProvider { job: job.id() });
    };
    let Some(equipment_definition) = registries.equipment().get_equipment(provider.definition())
    else {
        return Err(CastingJobValidationError::UnknownEquipmentDefinition { job: job.id() });
    };
    let Some(energy_definition) = registries.energy().get_store(released_energy.definition())
    else {
        return Err(CastingJobValidationError::UnknownEnergyDefinition { job: job.id() });
    };
    let cooling_power = match resolve_equipment_capability(
        equipment_definition,
        provider.condition(),
        definition.cooling_power_capability(),
    ) {
        Some(CapabilityValue::Power(power)) => power,
        Some(_) | None => {
            return Err(CastingJobValidationError::MissingCoolingPowerCapability { job: job.id() });
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
                CastingJobValidationError::MissingMaximumTemperatureCapability { job: job.id() },
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
                CastingJobValidationError::MissingMaximumBatchMassCapability { job: job.id() },
            );
        }
    };
    if job.consumed_mass() > maximum_batch_mass {
        return Err(
            CastingJobValidationError::BatchMassExceedsEquipmentCapacity {
                job: job.id(),
                selected: job.consumed_mass(),
                maximum: maximum_batch_mass,
            },
        );
    }
    let batch = resolve_casting_batch(
        registries.materials(),
        definition.solid_form(),
        job.consumed_inputs(),
    )
    .map_err(|error| CastingJobValidationError::Batch {
        job: job.id(),
        error,
    })?;
    if batch.hottest_input > maximum_temperature {
        return Err(
            CastingJobValidationError::InputTemperatureExceedsEquipmentMaximum {
                job: job.id(),
                input: batch.hottest_input,
                maximum: maximum_temperature,
            },
        );
    }
    if released_energy.carrier() != definition.energy_carrier() {
        return Err(CastingJobValidationError::WrongEnergyCarrier {
            job: job.id(),
            required: definition.energy_carrier(),
            provided: released_energy.carrier(),
        });
    }
    if released_energy.energy() != batch.released_energy {
        return Err(CastingJobValidationError::ReleasedEnergyMismatch {
            job: job.id(),
            traced: released_energy.energy(),
            required: batch.released_energy,
        });
    }
    let transfer_power = cooling_power.min(energy_definition.max_input_power());
    let required_duration = calculate_power_duration_ceiling(
        transfer_power,
        batch.released_energy,
        registries.core().physical_tick_duration(),
    )
    .map_err(|error| CastingJobValidationError::Duration {
        job: job.id(),
        error,
    })?;
    let stored_duration = job.active_duration();
    if stored_duration != required_duration {
        return Err(CastingJobValidationError::DurationMismatch {
            job: job.id(),
            stored: stored_duration,
            required: required_duration,
        });
    }
    let required_condition_after = calculate_usable_condition_after_active_ticks(
        definition.condition_wear_ppm_per_active_tick(),
        provider.condition(),
        required_duration,
    )
    .map_err(|error| CastingJobValidationError::ConditionDuration {
        job: job.id(),
        error,
    })?;
    let Some(stored_condition_after) = job.equipment_condition_after() else {
        return Err(CastingJobValidationError::MissingEquipmentConditionOutcome { job: job.id() });
    };
    if stored_condition_after != required_condition_after {
        return Err(
            CastingJobValidationError::EquipmentConditionOutcomeMismatch {
                job: job.id(),
                stored: stored_condition_after,
                required: required_condition_after,
            },
        );
    }
    let Some(output_stream) = job.single_output_stream() else {
        return Err(CastingJobValidationError::OutputMismatch { job: job.id() });
    };
    if output_stream.outputs() != [batch.output] {
        return Err(CastingJobValidationError::OutputMismatch { job: job.id() });
    }
    Ok(())
}

#[cfg(all(
    test,
    any(not(feature = "test-unit-sharded"), feature = "test-unit-industry")
))]
mod tests {
    use super::*;
    use crate::capability::{
        CapabilityComparison, CapabilityDefinition, CapabilityProfile, CapabilityRequirement,
        CapabilityValueKind,
    };
    use crate::content::{
        FORM_INGOT, FORM_MOLTEN, MATERIAL_COPPER, make_test_registries_with_casting,
    };
    use crate::core::state::{StateValidationError, validate_loaded_state};
    use crate::core::time::WorldSeed;
    use crate::energy::{
        EnergyStoreDefinition, EnergyStoreDefinitionId, EnergyStoreRecord,
        ExplicitEnergyAccountingError, add_energy_store, calculate_explicit_energy_accounting,
        validate_energy_sink,
    };
    use crate::equipment::{EquipmentDefinition, EquipmentDefinitionId, add_equipment};
    use crate::inventory::{
        MaterialLotId, StockpileStorageProfile, add_solid_stockpile_for_test, add_stockpile,
        deposit_lot_for_test,
    };
    use crate::maintenance::MaintenanceThresholds;
    use crate::matter::calculate_matter_accounting;
    use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
    use crate::production::{
        CompletionCommitError, ProcessDefinition, apply_completion_plan, decide_due_completions,
        validate_start_process,
    };
    use crate::simulation::advance_tick;
    use crate::thermal::ThermalJobValidationError;

    const COOLING_POWER: CapabilityId = CapabilityId::new(960_001);
    const MAX_TEMPERATURE: CapabilityId = CapabilityId::new(960_002);
    const MAX_BATCH_MASS: CapabilityId = CapabilityId::new(960_003);
    const MOLD: EquipmentDefinitionId = EquipmentDefinitionId::new(960_001);
    const HEAT_SINK: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(960_001);
    const PROCESS: ProcessId = ProcessId::new(960_001);
    const MELTING_POINT: Temperature = Temperature::from_millikelvin(1_357_770);

    #[derive(Clone, Copy)]
    struct FixtureIds {
        source: StockpileId,
        destination: StockpileId,
        source_lot: MaterialLotId,
        equipment: EquipmentId,
        heat_sink: EnergyStoreId,
    }

    struct CastingFixture {
        registries: Registries,
        state: AppState,
        ids: FixtureIds,
    }

    fn condition(parts_per_million: u32) -> Condition {
        match Condition::new(parts_per_million) {
            Ok(condition) => condition,
            Err(error) => panic!("casting condition fixture failed: {error}"),
        }
    }

    fn make_registries(
        sink_carrier: EnergyCarrier,
        sink_capacity: Energy,
        sink_input_power: Power,
    ) -> Registries {
        let profile = match CapabilityProfile::new([
            (
                COOLING_POWER,
                CapabilityValue::Power(Power::from_microwatts(10_000_000)),
            ),
            (
                MAX_TEMPERATURE,
                CapabilityValue::Temperature(Temperature::from_millikelvin(1_600_000)),
            ),
            (
                MAX_BATCH_MASS,
                CapabilityValue::Mass(Mass::from_milligrams(20)),
            ),
        ]) {
            Ok(profile) => profile,
            Err(error) => panic!("casting capability profile failed: {error}"),
        };
        let thresholds = match MaintenanceThresholds::new(condition(600_000), condition(250_000)) {
            Ok(thresholds) => thresholds,
            Err(error) => panic!("casting maintenance fixture failed: {error}"),
        };
        let equipment = EquipmentDefinition::new(
            MOLD,
            "test cooled casting mold",
            Mass::from_milligrams(500_000),
            profile,
            thresholds,
        );
        let sink = EnergyStoreDefinition::new_with_transfer_limits(
            HEAT_SINK,
            "test finite thermal sink",
            sink_carrier,
            sink_capacity,
            sink_input_power,
            Power::ZERO,
        );
        let process = ProcessDefinition::new_selected_batch(
            PROCESS,
            "pure material casting",
            vec![
                CapabilityRequirement::new(
                    COOLING_POWER,
                    CapabilityComparison::AtLeast,
                    CapabilityValue::Power(Power::from_microwatts(1_000_000)),
                ),
                CapabilityRequirement::new(
                    MAX_TEMPERATURE,
                    CapabilityComparison::AtLeast,
                    CapabilityValue::Temperature(Temperature::from_millikelvin(1_400_000)),
                ),
                CapabilityRequirement::new(
                    MAX_BATCH_MASS,
                    CapabilityComparison::AtLeast,
                    CapabilityValue::Mass(Mass::from_milligrams(1)),
                ),
            ],
        );
        make_test_registries_with_casting(
            vec![
                CapabilityDefinition::new(
                    COOLING_POWER,
                    "casting cooling power",
                    CapabilityValueKind::Power,
                ),
                CapabilityDefinition::new(
                    MAX_TEMPERATURE,
                    "casting maximum input temperature",
                    CapabilityValueKind::Temperature,
                ),
                CapabilityDefinition::new(
                    MAX_BATCH_MASS,
                    "casting maximum batch mass",
                    CapabilityValueKind::Mass,
                ),
            ],
            equipment,
            sink,
            process,
            CastingProcessDefinition::new(
                PROCESS,
                COOLING_POWER,
                MAX_TEMPERATURE,
                MAX_BATCH_MASS,
                EnergyCarrier::Thermal,
                FORM_INGOT,
                10,
            ),
        )
    }

    fn make_fixture(input_mass: Mass, input_temperature: Temperature) -> CastingFixture {
        let registries = make_registries(
            EnergyCarrier::Thermal,
            Energy::from_nanojoules(100_000_000_000),
            Power::from_microwatts(10_000_000),
        );
        let mut state = AppState::new(WorldSeed::new(0x9600_0001));
        let source_profile = match StockpileStorageProfile::new(
            false,
            true,
            Temperature::from_millikelvin(1_600_000),
        ) {
            Ok(profile) => profile,
            Err(error) => panic!("casting source profile failed: {error}"),
        };
        let source = match add_stockpile(&mut state, Mass::from_milligrams(1_000), source_profile) {
            Ok(source) => source,
            Err(error) => panic!("casting source fixture failed: {error}"),
        };
        let destination =
            match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000)) {
                Ok(destination) => destination,
                Err(error) => panic!("casting destination fixture failed: {error}"),
            };
        let source_lot = match deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN),
            input_mass,
            input_temperature,
        ) {
            Ok(lot) => lot,
            Err(error) => panic!("casting molten input fixture failed: {error}"),
        };
        let equipment = match add_equipment(&registries, &mut state, MOLD, Condition::PRISTINE) {
            Ok(equipment) => equipment,
            Err(error) => panic!("casting equipment fixture failed: {error}"),
        };
        let heat_sink = match add_energy_store(&registries, &mut state, HEAT_SINK) {
            Ok(store) => store,
            Err(error) => panic!("casting heat sink fixture failed: {error}"),
        };
        CastingFixture {
            registries,
            state,
            ids: FixtureIds {
                source,
                destination,
                source_lot,
                equipment,
                heat_sink,
            },
        }
    }

    fn resolve_selected(
        registries: &Registries,
        state: &AppState,
        ids: FixtureIds,
        mass: Mass,
    ) -> Result<ResolvedCasting, CastingResolutionError> {
        resolve_casting_process(
            registries,
            state,
            CastingRequest::new(
                PROCESS,
                ids.source,
                &[MaterialLotSelection::new(ids.source_lot, mass)],
                ids.equipment,
                ids.heat_sink,
            ),
        )
    }

    fn matter_total(state: &AppState) -> crate::core::quantity::AggregateMass {
        match calculate_matter_accounting(state) {
            Ok(accounting) => accounting.total(),
            Err(error) => panic!("casting matter accounting failed: {error}"),
        }
    }

    fn energy_total(registries: &Registries, state: &AppState) -> Energy {
        match calculate_explicit_energy_accounting(registries, state).and_then(|accounting| {
            accounting
                .total()
                .ok_or(ExplicitEnergyAccountingError::Overflow)
        }) {
            Ok(total) => total,
            Err(error) => panic!("casting energy accounting failed: {error}"),
        }
    }

    fn finish_job(registries: &Registries, state: &mut AppState, duration: TickSpan) {
        for _ in 0..duration.value() {
            if let Err(error) = advance_tick(registries, state) {
                panic!("casting completion tick failed: {error}");
            }
        }
    }

    #[test]
    fn casting_at_fusion_boundary_releases_exact_latent_heat() {
        let fixture = make_fixture(Mass::from_milligrams(10), MELTING_POINT);
        let expected = match calculate_fusion_heat(
            fixture.registries.materials(),
            Mass::from_milligrams(10),
            MATERIAL_COPPER,
        ) {
            Ok(heat) => heat.energy(),
            Err(error) => panic!("casting latent heat fixture failed: {error}"),
        };

        let resolved = match resolve_selected(
            &fixture.registries,
            &fixture.state,
            fixture.ids,
            Mass::from_milligrams(10),
        ) {
            Ok(resolved) => resolved,
            Err(error) => panic!("casting resolution failed: {error}"),
        };

        assert_eq!(resolved.material(), MATERIAL_COPPER);
        assert_eq!(resolved.melting_point(), MELTING_POINT);
        assert_eq!(resolved.released_energy(), expected);
        assert_eq!(resolved.process_resolution().outputs().len(), 1);
        let output = &resolved.process_resolution().outputs()[0];
        assert_eq!(
            output.commodity(),
            CommodityKey::new(MATERIAL_COPPER, FORM_INGOT)
        );
        assert_eq!(output.mass(), Mass::from_milligrams(10));
        assert_eq!(output.temperature(), MELTING_POINT);
    }

    #[test]
    fn superheated_casting_releases_sensible_cooling_plus_latent_heat() {
        let input_temperature = Temperature::from_millikelvin(1_400_000);
        let fixture = make_fixture(Mass::from_milligrams(10), input_temperature);
        let sensible = match calculate_sensible_heat(
            fixture.registries.materials(),
            Mass::from_milligrams(10),
            &MaterialComposition::pure(MATERIAL_COPPER),
            input_temperature,
            MELTING_POINT,
        ) {
            Ok(heat) => heat.energy(),
            Err(error) => panic!("casting sensible-cooling fixture failed: {error}"),
        };
        let latent = match calculate_fusion_heat(
            fixture.registries.materials(),
            Mass::from_milligrams(10),
            MATERIAL_COPPER,
        ) {
            Ok(heat) => heat.energy(),
            Err(error) => panic!("casting latent fixture failed: {error}"),
        };
        let expected = match sensible.checked_add(latent) {
            Some(energy) => energy,
            None => panic!("casting expected released energy overflowed"),
        };

        let resolved = match resolve_selected(
            &fixture.registries,
            &fixture.state,
            fixture.ids,
            Mass::from_milligrams(10),
        ) {
            Ok(resolved) => resolved,
            Err(error) => panic!("superheated casting resolution failed: {error}"),
        };

        assert_eq!(resolved.released_energy(), expected);
        assert!(resolved.released_energy() > latent);
    }

    #[test]
    fn casting_rejects_wrong_energy_sink_carrier_without_mutation() {
        let registries = make_registries(
            EnergyCarrier::Electrical,
            Energy::from_nanojoules(10_000_000_000),
            Power::from_microwatts(10_000_000),
        );
        let mut state = AppState::new(WorldSeed::new(0x9600_0002));
        let source_profile = match StockpileStorageProfile::new(
            false,
            true,
            Temperature::from_millikelvin(1_500_000),
        ) {
            Ok(profile) => profile,
            Err(error) => panic!("wrong-carrier source profile failed: {error}"),
        };
        let source = match add_stockpile(&mut state, Mass::from_milligrams(100), source_profile) {
            Ok(source) => source,
            Err(error) => panic!("wrong-carrier source failed: {error}"),
        };
        let lot = match deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN),
            Mass::from_milligrams(10),
            MELTING_POINT,
        ) {
            Ok(lot) => lot,
            Err(error) => panic!("wrong-carrier molten input failed: {error}"),
        };
        let equipment = match add_equipment(&registries, &mut state, MOLD, Condition::PRISTINE) {
            Ok(equipment) => equipment,
            Err(error) => panic!("wrong-carrier equipment failed: {error}"),
        };
        let sink = match add_energy_store(&registries, &mut state, HEAT_SINK) {
            Ok(sink) => sink,
            Err(error) => panic!("wrong-carrier sink failed: {error}"),
        };
        let before = state.clone();

        assert!(matches!(
            resolve_casting_process(
                &registries,
                &state,
                CastingRequest::new(
                    PROCESS,
                    source,
                    &[MaterialLotSelection::new(lot, Mass::from_milligrams(10))],
                    equipment,
                    sink,
                ),
            ),
            Err(CastingResolutionError::WrongEnergyCarrier {
                required: EnergyCarrier::Thermal,
                provided: EnergyCarrier::Electrical,
            })
        ));
        assert_eq!(state, before);
    }

    #[test]
    fn casting_moves_released_heat_only_when_completion_becomes_authoritative() {
        let mut fixture = make_fixture(Mass::from_milligrams(10), MELTING_POINT);
        let initial_matter = matter_total(&fixture.state);
        let initial_energy = energy_total(&fixture.registries, &fixture.state);
        let resolved = match resolve_selected(
            &fixture.registries,
            &fixture.state,
            fixture.ids,
            Mass::from_milligrams(10),
        ) {
            Ok(resolved) => resolved,
            Err(error) => panic!("casting resolution failed: {error}"),
        };
        let released = resolved.released_energy();
        let duration = resolved.process_resolution().duration();
        let token = match validate_start_process(
            &fixture.registries,
            &fixture.state,
            resolved.process_resolution(),
            fixture.ids.source,
            fixture.ids.destination,
        ) {
            Ok(token) => token,
            Err(error) => panic!("casting start validation failed: {error}"),
        };
        if let Err(error) = token.commit(&mut fixture.state) {
            panic!("casting start commit failed: {error}");
        }

        assert_eq!(
            fixture
                .state
                .energy()
                .get_store(fixture.ids.heat_sink)
                .map(EnergyStoreRecord::stored),
            Some(Energy::ZERO)
        );
        assert_eq!(matter_total(&fixture.state), initial_matter);
        assert_eq!(
            energy_total(&fixture.registries, &fixture.state),
            initial_energy
        );
        assert_eq!(
            validate_loaded_state(&fixture.registries, &fixture.state),
            Ok(())
        );

        finish_job(&fixture.registries, &mut fixture.state, duration);
        assert_eq!(
            fixture
                .state
                .energy()
                .get_store(fixture.ids.heat_sink)
                .map(EnergyStoreRecord::stored),
            Some(released)
        );
        assert_eq!(matter_total(&fixture.state), initial_matter);
        assert_eq!(
            energy_total(&fixture.registries, &fixture.state),
            initial_energy
        );
        assert_eq!(
            fixture
                .state
                .inventory()
                .get_stockpile(fixture.ids.destination)
                .map(|stockpile| {
                    stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_INGOT))
                }),
            Some(Mass::from_milligrams(10))
        );
    }

    #[test]
    fn active_casting_job_reserves_thermal_sink_exclusively() {
        let mut fixture = make_fixture(Mass::from_milligrams(10), MELTING_POINT);
        let resolved = match resolve_selected(
            &fixture.registries,
            &fixture.state,
            fixture.ids,
            Mass::from_milligrams(10),
        ) {
            Ok(resolved) => resolved,
            Err(error) => panic!("casting resolution failed: {error}"),
        };
        let released = resolved.released_energy();
        let token = match validate_start_process(
            &fixture.registries,
            &fixture.state,
            resolved.process_resolution(),
            fixture.ids.source,
            fixture.ids.destination,
        ) {
            Ok(token) => token,
            Err(error) => panic!("casting start validation failed: {error}"),
        };
        let job = match token.commit(&mut fixture.state) {
            Ok(job) => job,
            Err(error) => panic!("casting start commit failed: {error}"),
        };

        assert_eq!(
            validate_energy_sink(
                &fixture.registries,
                &fixture.state,
                fixture.ids.heat_sink,
                released,
            ),
            Err(EnergySinkError::StoreBusy {
                store: fixture.ids.heat_sink,
                job,
                release: fixture
                    .state
                    .production()
                    .get_job(job)
                    .map(ProductionJobRecord::occupancy_release)
                    .unwrap_or_else(|| panic!("casting job disappeared")),
            })
        );
    }

    #[test]
    fn due_casting_completion_rejects_stale_energy_revision_atomically() {
        let mut fixture = make_fixture(Mass::from_milligrams(10), MELTING_POINT);
        let resolved = match resolve_selected(
            &fixture.registries,
            &fixture.state,
            fixture.ids,
            Mass::from_milligrams(10),
        ) {
            Ok(resolved) => resolved,
            Err(error) => panic!("casting completion-race resolution failed: {error}"),
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
            Err(error) => panic!("casting completion-race start failed: {error}"),
        };
        let job = match token.commit(&mut fixture.state) {
            Ok(job) => job,
            Err(error) => panic!("casting completion-race commit failed: {error}"),
        };
        for _ in 1..duration.value() {
            if let Err(error) = advance_tick(&fixture.registries, &mut fixture.state) {
                panic!("casting completion-race pre-due tick failed: {error}");
            }
        }
        let due = match fixture.state.production().get_job(job) {
            Some(record) => record.completes_at(),
            None => panic!("casting completion-race job disappeared before planning"),
        };
        let plan = match decide_due_completions(&fixture.registries, &fixture.state, due) {
            Ok(plan) => plan,
            Err(error) => panic!("casting completion-race planning failed: {error:?}"),
        };
        let expected = fixture.state.energy().revision();
        if let Err(error) = add_energy_store(&fixture.registries, &mut fixture.state, HEAT_SINK) {
            panic!("casting independent energy mutation failed: {error}");
        }
        let before = fixture.state.clone();

        assert_eq!(
            apply_completion_plan(&mut fixture.state, plan),
            Err(CompletionCommitError::EnergyRevisionConflict {
                expected,
                actual: expected + 1,
            })
        );
        assert_eq!(fixture.state, before);
        assert!(fixture.state.production().get_job(job).is_some());
        assert_eq!(
            fixture
                .state
                .energy()
                .get_store(fixture.ids.heat_sink)
                .map(EnergyStoreRecord::stored),
            Some(Energy::ZERO)
        );
        assert_eq!(
            fixture
                .state
                .inventory()
                .get_stockpile(fixture.ids.destination)
                .map(|stockpile| {
                    stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_INGOT))
                }),
            Some(Mass::ZERO)
        );
    }

    #[test]
    fn casting_save_resume_preserves_exact_completion_and_rejects_tampered_heat() {
        let mut fixture = make_fixture(Mass::from_milligrams(10), MELTING_POINT);
        let resolved = match resolve_selected(
            &fixture.registries,
            &fixture.state,
            fixture.ids,
            Mass::from_milligrams(10),
        ) {
            Ok(resolved) => resolved,
            Err(error) => panic!("casting resolution failed: {error}"),
        };
        let required = resolved.released_energy();
        let duration = resolved.process_resolution().duration();
        let token = match validate_start_process(
            &fixture.registries,
            &fixture.state,
            resolved.process_resolution(),
            fixture.ids.source,
            fixture.ids.destination,
        ) {
            Ok(token) => token,
            Err(error) => panic!("casting start validation failed: {error}"),
        };
        let job = match token.commit(&mut fixture.state) {
            Ok(job) => job,
            Err(error) => panic!("casting start commit failed: {error}"),
        };
        let encoded =
            match serde_json::to_vec(&SaveEnvelope::new(&fixture.registries, &fixture.state)) {
                Ok(encoded) => encoded,
                Err(error) => panic!("casting save serialization failed: {error}"),
            };
        let decoded: LoadedSaveEnvelope = match serde_json::from_slice(&encoded) {
            Ok(decoded) => decoded,
            Err(error) => panic!("casting save deserialization failed: {error}"),
        };
        let mut resumed = match decoded.into_state(&fixture.registries) {
            Ok(state) => state,
            Err(error) => panic!("casting save validation failed: {error}"),
        };
        let mut uninterrupted = fixture.state.clone();
        finish_job(&fixture.registries, &mut resumed, duration);
        finish_job(&fixture.registries, &mut uninterrupted, duration);
        assert_eq!(resumed, uninterrupted);

        let mut tampered =
            match serde_json::to_value(SaveEnvelope::new(&fixture.registries, &fixture.state)) {
                Ok(encoded) => encoded,
                Err(error) => panic!("casting tamper serialization failed: {error}"),
            };
        tampered["state"]["systems"]["production"]["jobs"][job.value().to_string()]["resources"]
            ["released_energy"]["energy"] = serde_json::json!(1_u64);
        let tampered: LoadedSaveEnvelope = match serde_json::from_value(tampered) {
            Ok(decoded) => decoded,
            Err(error) => panic!("casting tampered save failed decode: {error}"),
        };
        assert_eq!(
            tampered.into_state(&fixture.registries),
            Err(LoadError::InvalidState(StateValidationError::ThermalJob(
                ThermalJobValidationError::Casting(
                    CastingJobValidationError::ReleasedEnergyMismatch {
                        job,
                        traced: Energy::from_nanojoules(1),
                        required,
                    }
                )
            )))
        );
    }

    #[cfg(feature = "test-soak")]
    #[test]
    #[ignore = "long-horizon soak"]
    fn casting_soak_preserves_conservation_and_replay() {
        let fixture = make_fixture(Mass::from_milligrams(300), MELTING_POINT);
        let initial_matter = matter_total(&fixture.state);
        let initial_energy = energy_total(&fixture.registries, &fixture.state);
        let mut first = fixture.state.clone();
        let mut second = fixture.state;

        for step in 0..300_u64 {
            for state in [&mut first, &mut second] {
                let resolved = match resolve_selected(
                    &fixture.registries,
                    state,
                    fixture.ids,
                    Mass::from_milligrams(1),
                ) {
                    Ok(resolved) => resolved,
                    Err(error) => panic!("casting soak resolution failed: {error}"),
                };
                let duration = resolved.process_resolution().duration();
                let token = match validate_start_process(
                    &fixture.registries,
                    state,
                    resolved.process_resolution(),
                    fixture.ids.source,
                    fixture.ids.destination,
                ) {
                    Ok(token) => token,
                    Err(error) => panic!("casting soak start failed: {error}"),
                };
                if let Err(error) = token.commit(state) {
                    panic!("casting soak commit failed: {error}");
                }
                finish_job(&fixture.registries, state, duration);
            }
            if step % 47 == 0 {
                assert_eq!(validate_loaded_state(&fixture.registries, &first), Ok(()));
                assert_eq!(matter_total(&first), initial_matter);
                assert_eq!(energy_total(&fixture.registries, &first), initial_energy);
            }
        }

        assert_eq!(first, second);
        assert_eq!(matter_total(&first), initial_matter);
        assert_eq!(energy_total(&fixture.registries, &first), initial_energy);
        assert_eq!(
            first
                .inventory()
                .get_stockpile(fixture.ids.destination)
                .map(|stockpile| {
                    stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_INGOT))
                }),
            Some(Mass::from_milligrams(300))
        );
    }
}
