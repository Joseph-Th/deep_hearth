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

use super::{
    FusionHeatError, PhaseChangeForms, SensibleHeatError, calculate_fusion_heat,
    calculate_sensible_heat,
};

/// Immutable declaration that one selected-batch process solidifies pure liquid matter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CastingProcessDefinition {
    process: ProcessId,
    cooling_power_capability: CapabilityId,
    max_temperature_capability: CapabilityId,
    max_batch_mass_capability: CapabilityId,
    energy_carrier: EnergyCarrier,
    forms: PhaseChangeForms,
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
        forms: PhaseChangeForms,
        condition_wear_ppm_per_active_tick: u32,
    ) -> Self {
        assert_valid_condition_wear_ppm_per_tick(condition_wear_ppm_per_active_tick);
        Self {
            process,
            cooling_power_capability,
            max_temperature_capability,
            max_batch_mass_capability,
            energy_carrier,
            forms,
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
    pub const fn liquid_form(self) -> FormId {
        self.forms.input()
    }

    #[must_use]
    pub const fn solid_form(self) -> FormId {
        self.forms.output()
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
    InputFormMismatch {
        expected: FormId,
        found: FormId,
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
            Self::InputFormMismatch { expected, found } => write!(
                formatter,
                "casting process requires liquid input form {} but selected form {} was provided",
                expected.value(),
                found.value()
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
            Self::InputFormMismatch {
                expected: _expected,
                found: _found,
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
    liquid_form: FormId,
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
        if form_id != liquid_form {
            return Err(CastingBatchError::InputFormMismatch {
                expected: liquid_form,
                found: form_id,
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
        definition.liquid_form(),
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
        definition.liquid_form(),
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

#[cfg(test)]
#[path = "casting_execution_tests.rs"]
mod tests;
