//! Authored sensible-heating semantics and physical resolution against exact matter, equipment, and finite energy.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::{
    CapabilityEvaluationError, CapabilityId, CapabilityRegistry, CapabilityValue,
    CapabilityValueKind, evaluate_capabilities,
};
use crate::core::quantity::{Energy, Mass, Power, Temperature};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::energy::{
    EnergyCarrier, EnergyStoreId, EnergySupplyError, PowerDurationError,
    calculate_power_duration_ceiling, validate_energy_supply,
};
use crate::equipment::{EquipmentId, EquipmentProviderError, resolve_equipment_provider};
use crate::inventory::{MaterialLotSelection, StockpileId};
use crate::material::{MaterialLotSpec, MaterialLotSpecError};
use crate::production::{
    ProcessId, ProcessInputError, ProcessResolution, ProcessResolutionError, ProductionJobId,
    ProductionJobRecord, ProductionRegistry, validate_selected_process_inputs,
};
use crate::registry::Registries;

use super::{HeatDirection, SensibleHeatError, calculate_sensible_heat};

/// Immutable declaration that one process is resolved as ideal sensible heating.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SensibleHeatingProcessDefinition {
    process: ProcessId,
    heating_power_capability: CapabilityId,
    max_temperature_capability: CapabilityId,
    max_batch_mass_capability: CapabilityId,
    energy_carrier: EnergyCarrier,
}

impl SensibleHeatingProcessDefinition {
    #[must_use]
    pub const fn new(
        process: ProcessId,
        heating_power_capability: CapabilityId,
        max_temperature_capability: CapabilityId,
        max_batch_mass_capability: CapabilityId,
        energy_carrier: EnergyCarrier,
    ) -> Self {
        Self {
            process,
            heating_power_capability,
            max_temperature_capability,
            max_batch_mass_capability,
            energy_carrier,
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
}

/// Immutable lookup table for process-specific thermal resolution semantics.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThermalRegistry {
    sensible_heating: BTreeMap<ProcessId, SensibleHeatingProcessDefinition>,
}

impl ThermalRegistry {
    pub(crate) fn new(
        definitions: impl IntoIterator<Item = SensibleHeatingProcessDefinition>,
    ) -> Self {
        let mut sensible_heating = BTreeMap::new();
        for definition in definitions {
            let process = definition.process();
            assert!(
                sensible_heating.insert(process, definition).is_none(),
                "duplicate sensible-heating definition for process {}",
                process.value()
            );
        }
        Self { sensible_heating }
    }

    #[must_use]
    pub fn get_sensible_heating(
        &self,
        process: ProcessId,
    ) -> Option<SensibleHeatingProcessDefinition> {
        self.sensible_heating.get(&process).copied()
    }

    pub(crate) fn validate_references(
        &self,
        production: &ProductionRegistry,
        capabilities: &CapabilityRegistry,
    ) {
        for definition in self.sensible_heating.values().copied() {
            assert!(
                production.get_process(definition.process()).is_some(),
                "thermal definition references missing process {}",
                definition.process().value()
            );
            let power = match capabilities.get_capability(definition.heating_power_capability()) {
                Some(capability) => capability,
                None => panic!(
                    "thermal process {} references missing heating-power capability {}",
                    definition.process().value(),
                    definition.heating_power_capability().value()
                ),
            };
            assert_eq!(
                power.kind(),
                CapabilityValueKind::Power,
                "thermal process {} heating-power capability must be Power",
                definition.process().value()
            );
            let maximum = match capabilities.get_capability(definition.max_temperature_capability())
            {
                Some(capability) => capability,
                None => panic!(
                    "thermal process {} references missing maximum-temperature capability {}",
                    definition.process().value(),
                    definition.max_temperature_capability().value()
                ),
            };
            assert_eq!(
                maximum.kind(),
                CapabilityValueKind::Temperature,
                "thermal process {} maximum-temperature capability must be Temperature",
                definition.process().value()
            );
            let maximum_batch =
                match capabilities.get_capability(definition.max_batch_mass_capability()) {
                    Some(capability) => capability,
                    None => panic!(
                        "thermal process {} references missing maximum-batch-mass capability {}",
                        definition.process().value(),
                        definition.max_batch_mass_capability().value()
                    ),
                };
            assert_eq!(
                maximum_batch.kind(),
                CapabilityValueKind::Mass,
                "thermal process {} maximum-batch-mass capability must be Mass",
                definition.process().value()
            );
        }
    }
}

/// Observable physically resolved sensible-heating operation before production start.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedSensibleHeating {
    resolution: ProcessResolution,
    equipment: EquipmentId,
    target: Temperature,
    required_energy: Energy,
    transfer_power: Power,
}

impl ResolvedSensibleHeating {
    pub const fn process_resolution(&self) -> &ProcessResolution {
        &self.resolution
    }

    #[must_use]
    pub const fn equipment(&self) -> EquipmentId {
        self.equipment
    }

    #[must_use]
    pub const fn target(&self) -> Temperature {
        self.target
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

/// Exact runtime selection and providers requested for one sensible-heating operation.
#[derive(Clone, Copy, Debug)]
pub struct SensibleHeatingRequest<'selection> {
    process: ProcessId,
    source: StockpileId,
    selections: &'selection [MaterialLotSelection],
    equipment: EquipmentId,
    energy_store: EnergyStoreId,
    target: Temperature,
}

impl<'selection> SensibleHeatingRequest<'selection> {
    #[must_use]
    pub const fn new(
        process: ProcessId,
        source: StockpileId,
        selections: &'selection [MaterialLotSelection],
        equipment: EquipmentId,
        energy_store: EnergyStoreId,
        target: Temperature,
    ) -> Self {
        Self {
            process,
            source,
            selections,
            equipment,
            energy_store,
            target,
        }
    }
}

/// Failure while resolving exact material heating into a startable production outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SensibleHeatingResolutionError {
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
    TargetExceedsEquipmentMaximum {
        target: Temperature,
        maximum: Temperature,
    },
    BatchMassExceedsEquipmentCapacity {
        selected: Mass,
        maximum: Mass,
    },
    TargetBelowInputTemperature {
        current: Temperature,
        target: Temperature,
    },
    Heat(SensibleHeatError),
    RequiredEnergyOverflow,
    NoHeatingRequired,
    Energy(EnergySupplyError),
    WrongEnergyCarrier {
        required: EnergyCarrier,
        provided: EnergyCarrier,
    },
    Duration(PowerDurationError),
    Output(MaterialLotSpecError),
    Resolution(ProcessResolutionError),
}

impl Display for SensibleHeatingResolutionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownThermalProcess { process } => write!(
                formatter,
                "process {} has no sensible-heating resolver definition",
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
            Self::TargetExceedsEquipmentMaximum { target, maximum } => write!(
                formatter,
                "target {} mK exceeds equipment maximum {} mK",
                target.millikelvin(),
                maximum.millikelvin()
            ),
            Self::BatchMassExceedsEquipmentCapacity { selected, maximum } => write!(
                formatter,
                "selected batch {} mg exceeds equipment capacity {} mg",
                selected.milligrams(),
                maximum.milligrams()
            ),
            Self::TargetBelowInputTemperature { current, target } => write!(
                formatter,
                "sensible-heating target {} mK is below selected input temperature {} mK",
                target.millikelvin(),
                current.millikelvin()
            ),
            Self::Heat(error) => write!(formatter, "sensible-heat calculation failed: {error}"),
            Self::RequiredEnergyOverflow => {
                formatter.write_str("required sensible heat overflowed")
            }
            Self::NoHeatingRequired => {
                formatter.write_str("selected matter is already at target temperature")
            }
            Self::Energy(error) => write!(formatter, "finite energy supply failed: {error}"),
            Self::WrongEnergyCarrier { required, provided } => write!(
                formatter,
                "sensible-heating process requires {required:?} energy but store provides {provided:?}"
            ),
            Self::Duration(error) => {
                write!(formatter, "heating duration calculation failed: {error}")
            }
            Self::Output(error) => write!(formatter, "heated output construction failed: {error}"),
            Self::Resolution(error) => write!(formatter, "process resolution failed: {error}"),
        }
    }
}

impl Error for SensibleHeatingResolutionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Input(error) => Some(error),
            Self::Equipment(error) => Some(error),
            Self::Capability(error) => Some(error),
            Self::Heat(error) => Some(error),
            Self::Energy(error) => Some(error),
            Self::Duration(error) => Some(error),
            Self::Output(error) => Some(error),
            Self::Resolution(error) => Some(error),
            Self::UnknownThermalProcess { .. }
            | Self::MissingHeatingPower { .. }
            | Self::MissingMaximumTemperature { .. }
            | Self::MissingMaximumBatchMass { .. }
            | Self::TargetExceedsEquipmentMaximum { .. }
            | Self::BatchMassExceedsEquipmentCapacity { .. }
            | Self::TargetBelowInputTemperature { .. }
            | Self::RequiredEnergyOverflow
            | Self::NoHeatingRequired
            | Self::WrongEnergyCarrier { .. } => None,
        }
    }
}

/// Resolves exact sensible heating from selected material state, equipment throughput, and a
/// finite energy store. The ideal transfer is 100% into sensible material heat; losses are not
/// invented until a thermal-environment owner exists to receive them.
pub fn resolve_sensible_heating_process(
    registries: &Registries,
    state: &AppState,
    request: SensibleHeatingRequest<'_>,
) -> Result<ResolvedSensibleHeating, SensibleHeatingResolutionError> {
    let SensibleHeatingRequest {
        process,
        source,
        selections,
        equipment,
        energy_store,
        target,
    } = request;
    let definition = registries
        .thermal()
        .get_sensible_heating(process)
        .ok_or(SensibleHeatingResolutionError::UnknownThermalProcess { process })?;
    let inputs = validate_selected_process_inputs(registries, state, process, source, selections)
        .map_err(SensibleHeatingResolutionError::Input)?;
    let provider = resolve_equipment_provider(registries, state, equipment)
        .map_err(SensibleHeatingResolutionError::Equipment)?;
    let equipment_use = provider.validated_use();
    let process_definition = match registries.production().get_process(process) {
        Some(process_definition) => process_definition,
        None => return Err(SensibleHeatingResolutionError::UnknownThermalProcess { process }),
    };
    evaluate_capabilities(
        registries.capabilities(),
        provider.capabilities(),
        process_definition.capability_requirements(),
    )
    .map_err(SensibleHeatingResolutionError::Capability)?;

    let heating_power = match provider
        .capabilities()
        .get_capability(definition.heating_power_capability())
    {
        Some(CapabilityValue::Power(power)) => power,
        Some(_) | None => {
            return Err(SensibleHeatingResolutionError::MissingHeatingPower {
                capability: definition.heating_power_capability(),
            });
        }
    };
    let maximum_temperature = match provider
        .capabilities()
        .get_capability(definition.max_temperature_capability())
    {
        Some(CapabilityValue::Temperature(temperature)) => temperature,
        Some(_) | None => {
            return Err(SensibleHeatingResolutionError::MissingMaximumTemperature {
                capability: definition.max_temperature_capability(),
            });
        }
    };
    if target > maximum_temperature {
        return Err(
            SensibleHeatingResolutionError::TargetExceedsEquipmentMaximum {
                target,
                maximum: maximum_temperature,
            },
        );
    }
    let maximum_batch_mass = match provider
        .capabilities()
        .get_capability(definition.max_batch_mass_capability())
    {
        Some(CapabilityValue::Mass(mass)) => mass,
        Some(_) | None => {
            return Err(SensibleHeatingResolutionError::MissingMaximumBatchMass {
                capability: definition.max_batch_mass_capability(),
            });
        }
    };
    if inputs.input_mass() > maximum_batch_mass {
        return Err(
            SensibleHeatingResolutionError::BatchMassExceedsEquipmentCapacity {
                selected: inputs.input_mass(),
                maximum: maximum_batch_mass,
            },
        );
    }

    let mut required_energy = Energy::ZERO;
    let mut output_masses = BTreeMap::new();
    for trace in inputs.consumed_inputs() {
        let profile = trace.profile();
        if target < profile.temperature() {
            return Err(
                SensibleHeatingResolutionError::TargetBelowInputTemperature {
                    current: profile.temperature(),
                    target,
                },
            );
        }
        let heat = calculate_sensible_heat(
            registries.materials(),
            trace.mass(),
            profile.composition(),
            profile.temperature(),
            target,
        )
        .map_err(SensibleHeatingResolutionError::Heat)?;
        debug_assert!(matches!(
            heat.direction(),
            HeatDirection::None | HeatDirection::IntoMaterial
        ));
        required_energy = required_energy
            .checked_add(heat.energy())
            .ok_or(SensibleHeatingResolutionError::RequiredEnergyOverflow)?;
        let key = (profile.commodity(), profile.composition().clone());
        let current = output_masses.get(&key).copied().unwrap_or(Mass::ZERO);
        let combined = current
            .checked_add(trace.mass())
            .ok_or(SensibleHeatingResolutionError::RequiredEnergyOverflow)?;
        output_masses.insert(key, combined);
    }
    if required_energy.is_zero() {
        return Err(SensibleHeatingResolutionError::NoHeatingRequired);
    }

    let energy_supply = validate_energy_supply(registries, state, energy_store, required_energy)
        .map_err(SensibleHeatingResolutionError::Energy)?;
    let provided_carrier = energy_supply.trace().carrier();
    if provided_carrier != definition.energy_carrier() {
        return Err(SensibleHeatingResolutionError::WrongEnergyCarrier {
            required: definition.energy_carrier(),
            provided: provided_carrier,
        });
    }
    let transfer_power = heating_power.min(energy_supply.max_output_power());
    let duration = calculate_power_duration_ceiling(
        transfer_power,
        required_energy,
        registries.core().ticks_per_second(),
    )
    .map_err(SensibleHeatingResolutionError::Duration)?;

    let mut outputs = Vec::with_capacity(output_masses.len());
    for ((commodity, composition), mass) in output_masses {
        let output = MaterialLotSpec::with_composition(commodity, mass, target, composition)
            .map_err(SensibleHeatingResolutionError::Output)?;
        outputs.push(output);
    }
    let resolution = inputs
        .resolve_with_energy_and_equipment(duration, outputs, energy_supply, equipment_use)
        .map_err(SensibleHeatingResolutionError::Resolution)?;
    Ok(ResolvedSensibleHeating {
        resolution,
        equipment,
        target,
        required_energy,
        transfer_power,
    })
}

/// Invalid persisted operation-specific thermal semantics discovered during exhaustive load audit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThermalJobValidationError {
    MissingEquipmentProvider {
        job: ProductionJobId,
    },
    UnknownEquipmentDefinition {
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
    TargetExceedsEquipmentMaximum {
        job: ProductionJobId,
        target: Temperature,
        maximum: Temperature,
    },
    BatchMassExceedsEquipmentCapacity {
        job: ProductionJobId,
        selected: Mass,
        maximum: Mass,
    },
    MissingEnergy {
        job: ProductionJobId,
    },
    MixedOutputTemperatures {
        job: ProductionJobId,
    },
    TargetBelowInputTemperature {
        job: ProductionJobId,
        current: Temperature,
        target: Temperature,
    },
    Heat {
        job: ProductionJobId,
        error: SensibleHeatError,
    },
    RequiredEnergyOverflow {
        job: ProductionJobId,
    },
    EnergyMismatch {
        job: ProductionJobId,
        traced: Energy,
        required: Energy,
    },
    OutputConstruction {
        job: ProductionJobId,
        error: MaterialLotSpecError,
    },
    OutputMismatch {
        job: ProductionJobId,
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
}

impl Display for ThermalJobValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingEquipmentProvider { job } => write!(
                formatter,
                "sensible-heating job {} has no equipment provider trace",
                job.value()
            ),
            Self::UnknownEquipmentDefinition { job } => write!(
                formatter,
                "sensible-heating job {} references an unavailable equipment definition",
                job.value()
            ),
            Self::MissingHeatingPowerCapability { job } => write!(
                formatter,
                "sensible-heating job {} provider lacks configured heating-power capability",
                job.value()
            ),
            Self::MissingMaximumTemperatureCapability { job } => write!(
                formatter,
                "sensible-heating job {} provider lacks configured maximum-temperature capability",
                job.value()
            ),
            Self::MissingMaximumBatchMassCapability { job } => write!(
                formatter,
                "sensible-heating job {} provider lacks configured maximum-batch-mass capability",
                job.value()
            ),
            Self::TargetExceedsEquipmentMaximum {
                job,
                target,
                maximum,
            } => write!(
                formatter,
                "sensible-heating job {} target {} mK exceeds provider maximum {} mK",
                job.value(),
                target.millikelvin(),
                maximum.millikelvin()
            ),
            Self::BatchMassExceedsEquipmentCapacity {
                job,
                selected,
                maximum,
            } => write!(
                formatter,
                "sensible-heating job {} batch {} mg exceeds provider capacity {} mg",
                job.value(),
                selected.milligrams(),
                maximum.milligrams()
            ),
            Self::MissingEnergy { job } => write!(
                formatter,
                "sensible-heating job {} has no consumed energy trace",
                job.value()
            ),
            Self::MixedOutputTemperatures { job } => write!(
                formatter,
                "sensible-heating job {} contains multiple committed target temperatures",
                job.value()
            ),
            Self::TargetBelowInputTemperature {
                job,
                current,
                target,
            } => write!(
                formatter,
                "sensible-heating job {} target {} mK is below consumed input temperature {} mK",
                job.value(),
                target.millikelvin(),
                current.millikelvin()
            ),
            Self::Heat { job, error } => write!(
                formatter,
                "sensible-heating job {} cannot reproduce sensible heat: {error}",
                job.value()
            ),
            Self::RequiredEnergyOverflow { job } => write!(
                formatter,
                "sensible-heating job {} required energy overflows authoritative storage",
                job.value()
            ),
            Self::EnergyMismatch {
                job,
                traced,
                required,
            } => write!(
                formatter,
                "sensible-heating job {} traces {} nJ but consumed matter requires {} nJ",
                job.value(),
                traced.nanojoules(),
                required.nanojoules()
            ),
            Self::OutputConstruction { job, error } => write!(
                formatter,
                "sensible-heating job {} cannot reconstruct output snapshot: {error}",
                job.value()
            ),
            Self::OutputMismatch { job } => write!(
                formatter,
                "sensible-heating job {} output snapshot does not preserve consumed mass/composition at one target temperature",
                job.value()
            ),
            Self::Duration { job, error } => write!(
                formatter,
                "sensible-heating job {} duration cannot be recomputed: {error}",
                job.value()
            ),
            Self::DurationMismatch {
                job,
                stored,
                required,
            } => write!(
                formatter,
                "sensible-heating job {} stores duration {} ticks but physics requires {} ticks",
                job.value(),
                stored.value(),
                required.value()
            ),
        }
    }
}

impl Error for ThermalJobValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Heat { error, .. } => Some(error),
            Self::OutputConstruction { error, .. } => Some(error),
            Self::Duration { error, .. } => Some(error),
            Self::MissingEquipmentProvider { .. }
            | Self::UnknownEquipmentDefinition { .. }
            | Self::MissingHeatingPowerCapability { .. }
            | Self::MissingMaximumTemperatureCapability { .. }
            | Self::MissingMaximumBatchMassCapability { .. }
            | Self::TargetExceedsEquipmentMaximum { .. }
            | Self::BatchMassExceedsEquipmentCapacity { .. }
            | Self::MissingEnergy { .. }
            | Self::MixedOutputTemperatures { .. }
            | Self::TargetBelowInputTemperature { .. }
            | Self::RequiredEnergyOverflow { .. }
            | Self::EnergyMismatch { .. }
            | Self::OutputMismatch { .. }
            | Self::DurationMismatch { .. } => None,
        }
    }
}

/// Recomputes the physical contract of an in-flight sensible-heating job from persisted input
/// traces. This prevents save tampering from changing either the consumed energy magnitude or the
/// committed heated outputs while retaining a superficially valid generic production record.
pub(crate) fn validate_loaded_thermal_job(
    registries: &Registries,
    job: &ProductionJobRecord,
) -> Result<(), ThermalJobValidationError> {
    if registries
        .thermal()
        .get_sensible_heating(job.process())
        .is_none()
    {
        return Ok(());
    }
    let Some(consumed_energy) = job.consumed_energy() else {
        return Err(ThermalJobValidationError::MissingEnergy { job: job.id() });
    };
    let Some(provider) = job.equipment_provider() else {
        return Err(ThermalJobValidationError::MissingEquipmentProvider { job: job.id() });
    };
    let Some(equipment_definition) = registries.equipment().get_equipment(provider.definition())
    else {
        return Err(ThermalJobValidationError::UnknownEquipmentDefinition { job: job.id() });
    };
    let Some(energy_definition) = registries.energy().get_store(consumed_energy.definition())
    else {
        return Err(ThermalJobValidationError::MissingEnergy { job: job.id() });
    };
    let thermal_definition = match registries.thermal().get_sensible_heating(job.process()) {
        Some(definition) => definition,
        None => return Ok(()),
    };
    let Some(first_output) = job.outputs().first() else {
        return Err(ThermalJobValidationError::OutputMismatch { job: job.id() });
    };
    let target = first_output.temperature();
    if job
        .outputs()
        .iter()
        .any(|output| output.temperature() != target)
    {
        return Err(ThermalJobValidationError::MixedOutputTemperatures { job: job.id() });
    }
    let heating_power = match equipment_definition
        .capabilities()
        .get_capability(thermal_definition.heating_power_capability())
    {
        Some(CapabilityValue::Power(power)) => power,
        Some(_) | None => {
            return Err(ThermalJobValidationError::MissingHeatingPowerCapability { job: job.id() });
        }
    };
    let maximum_temperature = match equipment_definition
        .capabilities()
        .get_capability(thermal_definition.max_temperature_capability())
    {
        Some(CapabilityValue::Temperature(temperature)) => temperature,
        Some(_) | None => {
            return Err(
                ThermalJobValidationError::MissingMaximumTemperatureCapability { job: job.id() },
            );
        }
    };
    if target > maximum_temperature {
        return Err(ThermalJobValidationError::TargetExceedsEquipmentMaximum {
            job: job.id(),
            target,
            maximum: maximum_temperature,
        });
    }
    let maximum_batch_mass = match equipment_definition
        .capabilities()
        .get_capability(thermal_definition.max_batch_mass_capability())
    {
        Some(CapabilityValue::Mass(mass)) => mass,
        Some(_) | None => {
            return Err(
                ThermalJobValidationError::MissingMaximumBatchMassCapability { job: job.id() },
            );
        }
    };
    if job.consumed_mass() > maximum_batch_mass {
        return Err(
            ThermalJobValidationError::BatchMassExceedsEquipmentCapacity {
                job: job.id(),
                selected: job.consumed_mass(),
                maximum: maximum_batch_mass,
            },
        );
    }

    let mut required_energy = Energy::ZERO;
    let mut output_masses = BTreeMap::new();
    for trace in job.consumed_inputs() {
        let profile = trace.profile();
        if target < profile.temperature() {
            return Err(ThermalJobValidationError::TargetBelowInputTemperature {
                job: job.id(),
                current: profile.temperature(),
                target,
            });
        }
        let heat = calculate_sensible_heat(
            registries.materials(),
            trace.mass(),
            profile.composition(),
            profile.temperature(),
            target,
        )
        .map_err(|error| ThermalJobValidationError::Heat {
            job: job.id(),
            error,
        })?;
        required_energy = required_energy
            .checked_add(heat.energy())
            .ok_or(ThermalJobValidationError::RequiredEnergyOverflow { job: job.id() })?;
        let key = (profile.commodity(), profile.composition().clone());
        let current = output_masses.get(&key).copied().unwrap_or(Mass::ZERO);
        output_masses.insert(
            key,
            current
                .checked_add(trace.mass())
                .ok_or(ThermalJobValidationError::RequiredEnergyOverflow { job: job.id() })?,
        );
    }
    if consumed_energy.energy() != required_energy {
        return Err(ThermalJobValidationError::EnergyMismatch {
            job: job.id(),
            traced: consumed_energy.energy(),
            required: required_energy,
        });
    }
    let transfer_power = heating_power.min(energy_definition.max_output_power());
    let required_duration = calculate_power_duration_ceiling(
        transfer_power,
        required_energy,
        registries.core().ticks_per_second(),
    )
    .map_err(|error| ThermalJobValidationError::Duration {
        job: job.id(),
        error,
    })?;
    let stored_duration = TickSpan::new(job.completes_at().value() - job.started_at().value());
    if stored_duration != required_duration {
        return Err(ThermalJobValidationError::DurationMismatch {
            job: job.id(),
            stored: stored_duration,
            required: required_duration,
        });
    }

    let mut expected_outputs = Vec::with_capacity(output_masses.len());
    for ((commodity, composition), mass) in output_masses {
        expected_outputs.push(
            MaterialLotSpec::with_composition(commodity, mass, target, composition).map_err(
                |error| ThermalJobValidationError::OutputConstruction {
                    job: job.id(),
                    error,
                },
            )?,
        );
    }
    expected_outputs.sort();
    let mut actual_outputs = job.outputs().to_vec();
    actual_outputs.sort();
    if actual_outputs != expected_outputs {
        return Err(ThermalJobValidationError::OutputMismatch { job: job.id() });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{CapabilityDefinition, CapabilityProfile, CapabilityValue};
    use crate::content::{
        FORM_LOG, FORM_ORE, MATERIAL_COPPER, MATERIAL_WOOD,
        make_test_registries_with_sensible_heating,
    };
    use crate::core::quantity::{Mass, Power};
    use crate::core::state::validate_loaded_state;
    use crate::core::time::WorldSeed;
    use crate::energy::{
        EnergyStoreDefinition, EnergyStoreDefinitionId, add_energy_store,
        add_energy_store_with_initial_for_test, calculate_explicit_energy_accounting,
    };
    use crate::equipment::{
        EquipmentConditionPlanError, EquipmentDefinition, EquipmentDefinitionId, add_equipment,
        decide_equipment_wear,
    };
    use crate::inventory::{add_stockpile, deposit_lot_for_test};
    use crate::maintenance::{Condition, MaintenanceThresholds};
    use crate::material::{CommodityKey, MaterialComposition};
    use crate::matter::calculate_matter_accounting;
    use crate::production::{ProcessDefinition, validate_start_process};
    use crate::simulation::advance_tick;

    const HEATING_POWER: CapabilityId = CapabilityId::new(920_001);
    const MAX_TEMPERATURE: CapabilityId = CapabilityId::new(920_002);
    const MAX_BATCH_MASS: CapabilityId = CapabilityId::new(920_003);
    const HEATER: EquipmentDefinitionId = EquipmentDefinitionId::new(920_001);
    const BATTERY: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(920_001);
    const PROCESS: ProcessId = ProcessId::new(920_001);

    fn condition(parts_per_million: u32) -> Condition {
        match Condition::new(parts_per_million) {
            Ok(condition) => condition,
            Err(error) => panic!("thermal test condition fixture failed: {error}"),
        }
    }

    fn make_registries_with_max_temperature(
        carrier: EnergyCarrier,
        maximum_temperature: Temperature,
    ) -> Registries {
        let capabilities = match CapabilityProfile::new([
            (
                HEATING_POWER,
                CapabilityValue::Power(Power::from_microwatts(1_000_000)),
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
            Err(error) => panic!("thermal capability fixture failed: {error}"),
        };
        let thresholds = match MaintenanceThresholds::new(condition(600_000), condition(250_000)) {
            Ok(thresholds) => thresholds,
            Err(error) => panic!("thermal maintenance fixture failed: {error}"),
        };
        let equipment = EquipmentDefinition::new(
            HEATER,
            "test resistive heater",
            Mass::from_milligrams(1_000_000),
            capabilities,
            thresholds,
        );
        let energy = EnergyStoreDefinition::new(
            BATTERY,
            "test finite battery",
            carrier,
            Energy::from_nanojoules(1_000_000_000),
            Power::from_microwatts(500_000),
        );
        let process =
            ProcessDefinition::new_selected_batch(PROCESS, "test sensible heating", Vec::new());
        make_test_registries_with_sensible_heating(
            vec![
                CapabilityDefinition::new(
                    HEATING_POWER,
                    "heating transfer power",
                    CapabilityValueKind::Power,
                ),
                CapabilityDefinition::new(
                    MAX_TEMPERATURE,
                    "maximum chamber temperature",
                    CapabilityValueKind::Temperature,
                ),
                CapabilityDefinition::new(
                    MAX_BATCH_MASS,
                    "maximum chamber batch mass",
                    CapabilityValueKind::Mass,
                ),
            ],
            equipment,
            energy,
            process,
            SensibleHeatingProcessDefinition::new(
                PROCESS,
                HEATING_POWER,
                MAX_TEMPERATURE,
                MAX_BATCH_MASS,
                EnergyCarrier::Electrical,
            ),
        )
    }

    fn make_registries(carrier: EnergyCarrier) -> Registries {
        make_registries_with_max_temperature(carrier, Temperature::from_millikelvin(400_000))
    }

    fn make_loaded_fixture_at(
        carrier: EnergyCarrier,
        input_temperature: Temperature,
        initial_energy: Energy,
    ) -> (
        Registries,
        AppState,
        StockpileId,
        StockpileId,
        EquipmentId,
        EnergyStoreId,
    ) {
        let registries = make_registries(carrier);
        let mut state = AppState::new(WorldSeed::new(0x9200_0001));
        let source = match add_stockpile(&mut state, Mass::from_milligrams(100)) {
            Ok(id) => id,
            Err(error) => panic!("thermal source fixture failed: {error}"),
        };
        let destination = match add_stockpile(&mut state, Mass::from_milligrams(100)) {
            Ok(id) => id,
            Err(error) => panic!("thermal destination fixture failed: {error}"),
        };
        if let Err(error) = deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(10),
            input_temperature,
        ) {
            panic!("thermal input fixture failed: {error}");
        }
        let equipment = match add_equipment(&registries, &mut state, HEATER, Condition::PRISTINE) {
            Ok(id) => id,
            Err(error) => panic!("thermal equipment fixture failed: {error}"),
        };
        let energy = match add_energy_store_with_initial_for_test(
            &registries,
            &mut state,
            BATTERY,
            initial_energy,
        ) {
            Ok(id) => id,
            Err(error) => panic!("thermal energy fixture failed: {error}"),
        };
        (registries, state, source, destination, equipment, energy)
    }

    fn make_loaded_fixture(
        carrier: EnergyCarrier,
    ) -> (
        Registries,
        AppState,
        StockpileId,
        StockpileId,
        EquipmentId,
        EnergyStoreId,
    ) {
        make_loaded_fixture_at(
            carrier,
            Temperature::from_millikelvin(300_000),
            Energy::from_nanojoules(500_000_000),
        )
    }

    fn resolve_test_sensible_heating_process(
        registries: &Registries,
        state: &AppState,
        process: ProcessId,
        source: StockpileId,
        equipment: EquipmentId,
        energy_store: EnergyStoreId,
        target: Temperature,
    ) -> Result<ResolvedSensibleHeating, SensibleHeatingResolutionError> {
        let lot = match state
            .inventory()
            .lots()
            .find(|lot| lot.stockpile() == source && lot.mass() >= Mass::from_milligrams(10))
        {
            Some(lot) => lot.id(),
            None => panic!("thermal test source has no selectable 10 mg lot"),
        };
        resolve_sensible_heating_process(
            registries,
            state,
            SensibleHeatingRequest::new(
                process,
                source,
                &[MaterialLotSelection::new(lot, Mass::from_milligrams(10))],
                equipment,
                energy_store,
                target,
            ),
        )
    }

    #[test]
    fn sensible_heating_consumes_exact_energy_and_completes_with_target_temperature() {
        let (registries, mut state, source, destination, equipment, energy_store) =
            make_loaded_fixture(EnergyCarrier::Electrical);
        let initial_explicit_energy =
            match calculate_explicit_energy_accounting(&registries, &state).and_then(|accounting| {
                accounting
                    .total()
                    .ok_or(crate::energy::ExplicitEnergyAccountingError::Overflow)
            }) {
                Ok(total) => total,
                Err(error) => panic!("initial explicit energy accounting failed: {error}"),
            };
        let target = Temperature::from_millikelvin(303_000);
        let expected_heat = match calculate_sensible_heat(
            registries.materials(),
            Mass::from_milligrams(10),
            &MaterialComposition::pure(MATERIAL_WOOD),
            Temperature::from_millikelvin(300_000),
            target,
        ) {
            Ok(heat) => heat.energy(),
            Err(error) => panic!("expected heat fixture failed: {error}"),
        };
        let resolved = match resolve_test_sensible_heating_process(
            &registries,
            &state,
            PROCESS,
            source,
            equipment,
            energy_store,
            target,
        ) {
            Ok(resolved) => resolved,
            Err(error) => panic!("sensible heating resolution failed: {error}"),
        };
        assert_eq!(resolved.required_energy(), expected_heat);
        assert_eq!(resolved.transfer_power(), Power::from_microwatts(500_000));
        let expected_duration = match calculate_power_duration_ceiling(
            resolved.transfer_power(),
            expected_heat,
            registries.core().ticks_per_second(),
        ) {
            Ok(duration) => duration,
            Err(error) => panic!("thermal duration fixture failed: {error}"),
        };
        assert_eq!(resolved.process_resolution().duration(), expected_duration);

        let before_energy = state
            .energy()
            .get_store(energy_store)
            .map(|store| store.stored());
        let token = match validate_start_process(
            &registries,
            &state,
            resolved.process_resolution(),
            source,
            destination,
        ) {
            Ok(token) => token,
            Err(error) => panic!("heated process start validation failed: {error}"),
        };
        let job = match token.commit(&mut state) {
            Ok(job) => job,
            Err(error) => panic!("heated process start commit failed: {error}"),
        };
        assert_eq!(
            state
                .energy()
                .get_store(energy_store)
                .map(|store| store.stored()),
            before_energy.and_then(|energy| energy.checked_sub(expected_heat))
        );
        assert_eq!(
            state
                .production()
                .get_job(job)
                .and_then(|record| record.consumed_energy()),
            resolved.process_resolution().energy_input()
        );
        let in_flight_explicit_energy =
            match calculate_explicit_energy_accounting(&registries, &state).and_then(|accounting| {
                accounting
                    .total()
                    .ok_or(crate::energy::ExplicitEnergyAccountingError::Overflow)
            }) {
                Ok(total) => total,
                Err(error) => panic!("in-flight explicit energy accounting failed: {error}"),
            };
        assert_eq!(in_flight_explicit_energy, initial_explicit_energy);

        for _ in 0..expected_duration.value() {
            if let Err(error) = advance_tick(&registries, &mut state) {
                panic!("heated process completion tick failed: {error}");
            }
        }
        assert!(state.production().get_job(job).is_none());
        let output = match state
            .inventory()
            .lots()
            .find(|lot| lot.stockpile() == destination)
        {
            Some(output) => output,
            None => panic!("heated output lot missing after completion"),
        };
        assert_eq!(output.mass(), Mass::from_milligrams(10));
        assert_eq!(output.temperature(), target);
        assert_eq!(
            output.composition(),
            &MaterialComposition::pure(MATERIAL_WOOD)
        );
        let final_explicit_energy = match calculate_explicit_energy_accounting(&registries, &state)
            .and_then(|accounting| {
                accounting
                    .total()
                    .ok_or(crate::energy::ExplicitEnergyAccountingError::Overflow)
            }) {
            Ok(total) => total,
            Err(error) => panic!("final explicit energy accounting failed: {error}"),
        };
        assert_eq!(final_explicit_energy, initial_explicit_energy);
    }

    #[test]
    fn sensible_heating_rejects_wrong_energy_carrier_before_mutation() {
        let (registries, state, source, _, equipment, energy_store) =
            make_loaded_fixture(EnergyCarrier::Thermal);
        let before = state.clone();

        assert_eq!(
            resolve_test_sensible_heating_process(
                &registries,
                &state,
                PROCESS,
                source,
                equipment,
                energy_store,
                Temperature::from_millikelvin(303_000),
            ),
            Err(SensibleHeatingResolutionError::WrongEnergyCarrier {
                required: EnergyCarrier::Electrical,
                provided: EnergyCarrier::Thermal,
            })
        );
        assert_eq!(state, before);
    }

    #[test]
    fn sensible_heating_rejects_target_above_equipment_limit() {
        let (registries, state, source, _, equipment, energy_store) =
            make_loaded_fixture(EnergyCarrier::Electrical);

        assert_eq!(
            resolve_test_sensible_heating_process(
                &registries,
                &state,
                PROCESS,
                source,
                equipment,
                energy_store,
                Temperature::from_millikelvin(401_000),
            ),
            Err(
                SensibleHeatingResolutionError::TargetExceedsEquipmentMaximum {
                    target: Temperature::from_millikelvin(401_000),
                    maximum: Temperature::from_millikelvin(400_000),
                }
            )
        );
    }

    #[test]
    fn warmer_input_reduces_required_energy_and_duration() {
        let (cold_registries, cold_state, cold_source, _, cold_equipment, cold_energy) =
            make_loaded_fixture_at(
                EnergyCarrier::Electrical,
                Temperature::from_millikelvin(300_000),
                Energy::from_nanojoules(500_000_000),
            );
        let (warm_registries, warm_state, warm_source, _, warm_equipment, warm_energy) =
            make_loaded_fixture_at(
                EnergyCarrier::Electrical,
                Temperature::from_millikelvin(302_000),
                Energy::from_nanojoules(500_000_000),
            );
        let target = Temperature::from_millikelvin(303_000);
        let cold = match resolve_test_sensible_heating_process(
            &cold_registries,
            &cold_state,
            PROCESS,
            cold_source,
            cold_equipment,
            cold_energy,
            target,
        ) {
            Ok(resolved) => resolved,
            Err(error) => panic!("cold heating resolution failed: {error}"),
        };
        let warm = match resolve_test_sensible_heating_process(
            &warm_registries,
            &warm_state,
            PROCESS,
            warm_source,
            warm_equipment,
            warm_energy,
            target,
        ) {
            Ok(resolved) => resolved,
            Err(error) => panic!("warm heating resolution failed: {error}"),
        };

        assert_eq!(cold.required_energy(), Energy::from_nanojoules(51_000_000));
        assert_eq!(warm.required_energy(), Energy::from_nanojoules(17_000_000));
        assert!(cold.required_energy() > warm.required_energy());
        assert_eq!(cold.process_resolution().duration().value(), 3);
        assert_eq!(warm.process_resolution().duration().value(), 1);
    }

    #[test]
    fn selected_batch_mass_changes_heating_energy_without_static_recipe_quantity() {
        let (registries, state, source, _, equipment, energy_store) =
            make_loaded_fixture(EnergyCarrier::Electrical);
        let lot = match state
            .inventory()
            .lots()
            .find(|lot| lot.stockpile() == source)
        {
            Some(lot) => lot.id(),
            None => panic!("selected-batch fixture lot missing"),
        };
        let target = Temperature::from_millikelvin(303_000);
        let five = match resolve_sensible_heating_process(
            &registries,
            &state,
            SensibleHeatingRequest::new(
                PROCESS,
                source,
                &[MaterialLotSelection::new(lot, Mass::from_milligrams(5))],
                equipment,
                energy_store,
                target,
            ),
        ) {
            Ok(resolved) => resolved,
            Err(error) => panic!("5 mg selected-batch heating failed: {error}"),
        };
        let ten = match resolve_sensible_heating_process(
            &registries,
            &state,
            SensibleHeatingRequest::new(
                PROCESS,
                source,
                &[MaterialLotSelection::new(lot, Mass::from_milligrams(10))],
                equipment,
                energy_store,
                target,
            ),
        ) {
            Ok(resolved) => resolved,
            Err(error) => panic!("10 mg selected-batch heating failed: {error}"),
        };

        assert_eq!(
            five.process_resolution().input_mass(),
            Mass::from_milligrams(5)
        );
        assert_eq!(
            ten.process_resolution().input_mass(),
            Mass::from_milligrams(10)
        );
        assert_eq!(five.required_energy(), Energy::from_nanojoules(25_500_000));
        assert_eq!(ten.required_energy(), Energy::from_nanojoules(51_000_000));
        assert_eq!(five.process_resolution().duration().value(), 2);
        assert_eq!(ten.process_resolution().duration().value(), 3);
    }

    #[test]
    fn selected_batch_heating_rejects_mass_above_equipment_capacity_without_mutation() {
        let (registries, mut state, _, _, equipment, energy_store) =
            make_loaded_fixture(EnergyCarrier::Electrical);
        let source = match add_stockpile(&mut state, Mass::from_milligrams(100)) {
            Ok(source) => source,
            Err(error) => panic!("batch-capacity source allocation failed: {error}"),
        };
        let lot = match deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(21),
            Temperature::from_millikelvin(300_000),
        ) {
            Ok(lot) => lot,
            Err(error) => panic!("batch-capacity material fixture failed: {error}"),
        };
        let before = state.clone();

        assert_eq!(
            resolve_sensible_heating_process(
                &registries,
                &state,
                SensibleHeatingRequest::new(
                    PROCESS,
                    source,
                    &[MaterialLotSelection::new(lot, Mass::from_milligrams(21))],
                    equipment,
                    energy_store,
                    Temperature::from_millikelvin(303_000),
                ),
            ),
            Err(
                SensibleHeatingResolutionError::BatchMassExceedsEquipmentCapacity {
                    selected: Mass::from_milligrams(21),
                    maximum: Mass::from_milligrams(20),
                }
            )
        );
        assert_eq!(state, before);
    }

    #[test]
    fn selected_batch_heating_uses_actual_material_heat_capacity() {
        let (registries, mut state, wood_source, _, equipment, energy_store) =
            make_loaded_fixture(EnergyCarrier::Electrical);
        let copper_source = match add_stockpile(&mut state, Mass::from_milligrams(100)) {
            Ok(source) => source,
            Err(error) => panic!("copper heating source allocation failed: {error}"),
        };
        let copper_lot = match deposit_lot_for_test(
            &registries,
            &mut state,
            copper_source,
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            Mass::from_milligrams(10),
            Temperature::from_millikelvin(300_000),
        ) {
            Ok(lot) => lot,
            Err(error) => panic!("copper heating input failed: {error}"),
        };
        let wood_lot = match state
            .inventory()
            .lots()
            .find(|lot| lot.stockpile() == wood_source)
        {
            Some(lot) => lot.id(),
            None => panic!("wood heating input disappeared"),
        };
        let target = Temperature::from_millikelvin(303_000);
        let wood = match resolve_sensible_heating_process(
            &registries,
            &state,
            SensibleHeatingRequest::new(
                PROCESS,
                wood_source,
                &[MaterialLotSelection::new(
                    wood_lot,
                    Mass::from_milligrams(10),
                )],
                equipment,
                energy_store,
                target,
            ),
        ) {
            Ok(resolved) => resolved,
            Err(error) => panic!("wood property heating resolution failed: {error}"),
        };
        let copper = match resolve_sensible_heating_process(
            &registries,
            &state,
            SensibleHeatingRequest::new(
                PROCESS,
                copper_source,
                &[MaterialLotSelection::new(
                    copper_lot,
                    Mass::from_milligrams(10),
                )],
                equipment,
                energy_store,
                target,
            ),
        ) {
            Ok(resolved) => resolved,
            Err(error) => panic!("copper property heating resolution failed: {error}"),
        };

        assert_eq!(wood.required_energy(), Energy::from_nanojoules(51_000_000));
        assert_eq!(
            copper.required_energy(),
            Energy::from_nanojoules(11_550_000)
        );
        assert_eq!(wood.process_resolution().duration().value(), 3);
        assert_eq!(copper.process_resolution().duration().value(), 1);
    }

    #[test]
    fn sensible_heating_stops_at_material_phase_boundary() {
        let registries = make_registries_with_max_temperature(
            EnergyCarrier::Electrical,
            Temperature::from_millikelvin(2_000_000),
        );
        let mut state = AppState::new(WorldSeed::new(0x9200_0020));
        let source = match add_stockpile(&mut state, Mass::from_milligrams(100)) {
            Ok(source) => source,
            Err(error) => panic!("phase-boundary source allocation failed: {error}"),
        };
        let lot = match deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            Mass::from_milligrams(10),
            Temperature::from_millikelvin(300_000),
        ) {
            Ok(lot) => lot,
            Err(error) => panic!("phase-boundary copper input failed: {error}"),
        };
        let equipment = match add_equipment(&registries, &mut state, HEATER, Condition::PRISTINE) {
            Ok(equipment) => equipment,
            Err(error) => panic!("phase-boundary heater allocation failed: {error}"),
        };
        let energy_store = match add_energy_store_with_initial_for_test(
            &registries,
            &mut state,
            BATTERY,
            Energy::from_nanojoules(500_000_000),
        ) {
            Ok(store) => store,
            Err(error) => panic!("phase-boundary energy fixture failed: {error}"),
        };
        let before = state.clone();

        assert!(matches!(
            resolve_sensible_heating_process(
                &registries,
                &state,
                SensibleHeatingRequest::new(
                    PROCESS,
                    source,
                    &[MaterialLotSelection::new(lot, Mass::from_milligrams(10))],
                    equipment,
                    energy_store,
                    Temperature::from_millikelvin(1_400_000),
                ),
            ),
            Err(SensibleHeatingResolutionError::Heat(
                SensibleHeatError::PhaseBoundaryCrossed { .. }
            ))
        ));
        assert_eq!(state, before);
    }

    #[test]
    fn selected_batch_heating_rejects_empty_selection_without_mutation() {
        let (registries, state, source, _, equipment, energy_store) =
            make_loaded_fixture(EnergyCarrier::Electrical);
        let before = state.clone();

        assert_eq!(
            resolve_sensible_heating_process(
                &registries,
                &state,
                SensibleHeatingRequest::new(
                    PROCESS,
                    source,
                    &[],
                    equipment,
                    energy_store,
                    Temperature::from_millikelvin(303_000),
                ),
            ),
            Err(SensibleHeatingResolutionError::Input(
                ProcessInputError::EmptySelection
            ))
        );
        assert_eq!(state, before);
    }

    #[test]
    fn sensible_heating_rejects_insufficient_finite_energy_without_mutation() {
        let (registries, state, source, _, equipment, energy_store) = make_loaded_fixture_at(
            EnergyCarrier::Electrical,
            Temperature::from_millikelvin(300_000),
            Energy::from_nanojoules(50_000_000),
        );
        let before = state.clone();

        assert_eq!(
            resolve_test_sensible_heating_process(
                &registries,
                &state,
                PROCESS,
                source,
                equipment,
                energy_store,
                Temperature::from_millikelvin(303_000),
            ),
            Err(SensibleHeatingResolutionError::Energy(
                EnergySupplyError::InsufficientEnergy {
                    store: energy_store,
                    available: Energy::from_nanojoules(50_000_000),
                    requested: Energy::from_nanojoules(51_000_000),
                }
            ))
        );
        assert_eq!(state, before);
    }

    #[test]
    fn resolved_heating_energy_becomes_stale_after_independent_energy_mutation() {
        let (registries, mut state, source, destination, equipment, energy_store) =
            make_loaded_fixture(EnergyCarrier::Electrical);
        let resolved = match resolve_test_sensible_heating_process(
            &registries,
            &state,
            PROCESS,
            source,
            equipment,
            energy_store,
            Temperature::from_millikelvin(303_000),
        ) {
            Ok(resolved) => resolved,
            Err(error) => panic!("stale heating fixture resolution failed: {error}"),
        };
        let expected_revision = state.energy().revision();
        if let Err(error) = add_energy_store(&registries, &mut state, BATTERY) {
            panic!("independent energy mutation failed: {error}");
        }
        let before = state.clone();

        assert_eq!(
            validate_start_process(
                &registries,
                &state,
                resolved.process_resolution(),
                source,
                destination,
            ),
            Err(crate::production::StartProcessError::StaleResolvedEnergy {
                expected_energy_revision: expected_revision,
                actual_energy_revision: expected_revision + 1,
            })
        );
        assert_eq!(state, before);
    }

    #[test]
    fn validated_heating_start_rejects_stale_energy_before_consuming_matter() {
        let (registries, mut state, source, destination, equipment, energy_store) =
            make_loaded_fixture(EnergyCarrier::Electrical);
        let resolved = match resolve_test_sensible_heating_process(
            &registries,
            &state,
            PROCESS,
            source,
            equipment,
            energy_store,
            Temperature::from_millikelvin(303_000),
        ) {
            Ok(resolved) => resolved,
            Err(error) => panic!("atomic heating fixture resolution failed: {error}"),
        };
        let token = match validate_start_process(
            &registries,
            &state,
            resolved.process_resolution(),
            source,
            destination,
        ) {
            Ok(token) => token,
            Err(error) => panic!("atomic heating start validation failed: {error}"),
        };
        let expected_revision = state.energy().revision();
        if let Err(error) = add_energy_store(&registries, &mut state, BATTERY) {
            panic!("independent energy mutation failed: {error}");
        }
        let before_commit = state.clone();

        assert_eq!(
            token.commit(&mut state),
            Err(
                crate::production::StartProcessCommitError::StaleEnergyRevision {
                    expected: expected_revision,
                    actual: expected_revision + 1,
                }
            )
        );
        assert_eq!(state, before_commit);
        assert_eq!(state.production().jobs().count(), 0);
    }

    fn run_sensible_heating_soak(seed: WorldSeed) -> AppState {
        let registries = make_registries(EnergyCarrier::Electrical);
        let mut state = AppState::new(seed);
        let source = match add_stockpile(&mut state, Mass::from_milligrams(200)) {
            Ok(id) => id,
            Err(error) => panic!("heating soak source allocation failed: {error}"),
        };
        let destination = match add_stockpile(&mut state, Mass::from_milligrams(200)) {
            Ok(id) => id,
            Err(error) => panic!("heating soak destination allocation failed: {error}"),
        };
        if let Err(error) = deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(150),
            Temperature::from_millikelvin(300_000),
        ) {
            panic!("heating soak input deposit failed: {error}");
        }
        let equipment = match add_equipment(&registries, &mut state, HEATER, Condition::PRISTINE) {
            Ok(id) => id,
            Err(error) => panic!("heating soak equipment allocation failed: {error}"),
        };
        let energy_store = match add_energy_store_with_initial_for_test(
            &registries,
            &mut state,
            BATTERY,
            Energy::from_nanojoules(800_000_000),
        ) {
            Ok(id) => id,
            Err(error) => panic!("heating soak energy allocation failed: {error}"),
        };
        let initial_matter = match calculate_matter_accounting(&state) {
            Ok(accounting) => accounting.total(),
            Err(error) => panic!("heating soak initial matter accounting failed: {error}"),
        };
        let initial_explicit_energy =
            match calculate_explicit_energy_accounting(&registries, &state).and_then(|accounting| {
                accounting
                    .total()
                    .ok_or(crate::energy::ExplicitEnergyAccountingError::Overflow)
            }) {
                Ok(total) => total,
                Err(error) => panic!("heating soak initial energy accounting failed: {error}"),
            };
        let wood = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
        let target = Temperature::from_millikelvin(303_000);

        for step in 0_u64..5_000 {
            let available = match state.inventory().get_stockpile(source) {
                Some(stockpile) => stockpile.get_mass(wood),
                None => panic!("heating soak source disappeared"),
            };
            if step.is_multiple_of(13) && available >= Mass::from_milligrams(10) {
                let resolved = match resolve_test_sensible_heating_process(
                    &registries,
                    &state,
                    PROCESS,
                    source,
                    equipment,
                    energy_store,
                    target,
                ) {
                    Ok(resolved) => resolved,
                    Err(error) => panic!("heating soak resolution failed at step {step}: {error}"),
                };
                let token = match validate_start_process(
                    &registries,
                    &state,
                    resolved.process_resolution(),
                    source,
                    destination,
                ) {
                    Ok(token) => token,
                    Err(error) => {
                        panic!("heating soak start validation failed at step {step}: {error}")
                    }
                };
                if let Err(error) = token.commit(&mut state) {
                    panic!("heating soak start commit failed at step {step}: {error}");
                }
            }
            if let Err(error) = advance_tick(&registries, &mut state) {
                panic!("heating soak tick {step} failed: {error}");
            }
            if step.is_multiple_of(97) {
                if let Err(error) = validate_loaded_state(&registries, &state) {
                    panic!("heating soak exhaustive audit failed at step {step}: {error}");
                }
                let matter = match calculate_matter_accounting(&state) {
                    Ok(accounting) => accounting.total(),
                    Err(error) => {
                        panic!("heating soak matter accounting failed at step {step}: {error}")
                    }
                };
                assert_eq!(matter, initial_matter);
                let explicit_energy =
                    match calculate_explicit_energy_accounting(&registries, &state).and_then(
                        |accounting| {
                            accounting
                                .total()
                                .ok_or(crate::energy::ExplicitEnergyAccountingError::Overflow)
                        },
                    ) {
                        Ok(total) => total,
                        Err(error) => panic!(
                            "heating soak explicit energy accounting failed at step {step}: {error}"
                        ),
                    };
                assert_eq!(explicit_energy, initial_explicit_energy);
            }
        }

        assert_eq!(state.production().jobs().count(), 0);
        assert_eq!(
            state
                .inventory()
                .get_stockpile(source)
                .map(|stockpile| stockpile.get_mass(wood)),
            Some(Mass::ZERO)
        );
        assert_eq!(
            state
                .inventory()
                .get_stockpile(destination)
                .map(|stockpile| stockpile.get_mass(wood)),
            Some(Mass::from_milligrams(150))
        );
        assert_eq!(
            state
                .energy()
                .get_store(energy_store)
                .map(|store| store.stored()),
            Some(Energy::from_nanojoules(35_000_000))
        );
        assert!(
            state
                .inventory()
                .lots()
                .filter(|lot| lot.stockpile() == destination)
                .all(|lot| lot.temperature() == target
                    && lot.composition() == &MaterialComposition::pure(MATERIAL_WOOD))
        );
        let final_matter = match calculate_matter_accounting(&state) {
            Ok(accounting) => accounting.total(),
            Err(error) => panic!("heating soak final matter accounting failed: {error}"),
        };
        assert_eq!(final_matter, initial_matter);
        let final_explicit_energy = match calculate_explicit_energy_accounting(&registries, &state)
            .and_then(|accounting| {
                accounting
                    .total()
                    .ok_or(crate::energy::ExplicitEnergyAccountingError::Overflow)
            }) {
            Ok(total) => total,
            Err(error) => panic!("heating soak final energy accounting failed: {error}"),
        };
        assert_eq!(final_explicit_energy, initial_explicit_energy);
        state
    }

    #[test]
    fn sensible_heating_soak_preserves_determinism_matter_and_finite_energy() {
        let seed = WorldSeed::new(0x9200_5000);
        let first = run_sensible_heating_soak(seed);
        let second = run_sensible_heating_soak(seed);

        assert_eq!(first, second);
        assert_eq!(first.tick().value(), 5_000);
    }

    #[test]
    fn heater_is_exclusive_while_job_runs_and_releases_on_completion() {
        let (registries, mut state, source, destination, equipment, energy_store) =
            make_loaded_fixture(EnergyCarrier::Electrical);
        if let Err(error) = deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(10),
            Temperature::from_millikelvin(300_000),
        ) {
            panic!("second heater occupancy input failed: {error}");
        }
        let second_energy_store = match add_energy_store_with_initial_for_test(
            &registries,
            &mut state,
            BATTERY,
            Energy::from_nanojoules(500_000_000),
        ) {
            Ok(store) => store,
            Err(error) => panic!("second heater occupancy energy fixture failed: {error}"),
        };
        let target = Temperature::from_millikelvin(303_000);
        let first = match resolve_test_sensible_heating_process(
            &registries,
            &state,
            PROCESS,
            source,
            equipment,
            energy_store,
            target,
        ) {
            Ok(resolved) => resolved,
            Err(error) => panic!("first heater occupancy resolution failed: {error}"),
        };
        let duration = first.process_resolution().duration();
        let first_token = match validate_start_process(
            &registries,
            &state,
            first.process_resolution(),
            source,
            destination,
        ) {
            Ok(token) => token,
            Err(error) => panic!("first heater occupancy validation failed: {error}"),
        };
        let first_job = match first_token.commit(&mut state) {
            Ok(job) => job,
            Err(error) => panic!("first heater occupancy commit failed: {error}"),
        };
        let completes_at = match state.production().get_job(first_job) {
            Some(job) => job.completes_at(),
            None => panic!("first heater occupancy job disappeared"),
        };
        assert_eq!(
            state
                .production()
                .get_job(first_job)
                .and_then(|job| job.equipment_provider()),
            first.process_resolution().equipment_input()
        );

        let second = match resolve_test_sensible_heating_process(
            &registries,
            &state,
            PROCESS,
            source,
            equipment,
            second_energy_store,
            target,
        ) {
            Ok(resolved) => resolved,
            Err(error) => panic!("second heater occupancy resolution failed: {error}"),
        };
        assert_eq!(
            validate_start_process(
                &registries,
                &state,
                second.process_resolution(),
                source,
                destination,
            ),
            Err(crate::production::StartProcessError::EquipmentBusy {
                equipment,
                job: first_job,
                completes_at,
            })
        );
        assert_eq!(
            decide_equipment_wear(&state, equipment, 1),
            Err(EquipmentConditionPlanError::EquipmentBusy {
                equipment,
                job: first_job,
                completes_at,
            })
        );

        for _ in 0..duration.value() {
            if let Err(error) = advance_tick(&registries, &mut state) {
                panic!("heater occupancy completion failed: {error}");
            }
        }
        assert!(state.production().get_job(first_job).is_none());
        assert!(decide_equipment_wear(&state, equipment, 1).is_ok());

        let after_release = match resolve_test_sensible_heating_process(
            &registries,
            &state,
            PROCESS,
            source,
            equipment,
            energy_store,
            target,
        ) {
            Ok(resolved) => resolved,
            Err(error) => panic!("post-release heater resolution failed: {error}"),
        };
        let token = match validate_start_process(
            &registries,
            &state,
            after_release.process_resolution(),
            source,
            destination,
        ) {
            Ok(token) => token,
            Err(error) => panic!("post-release heater start failed: {error}"),
        };
        if let Err(error) = token.commit(&mut state) {
            panic!("post-release heater commit failed: {error}");
        }
    }

    #[test]
    fn finite_energy_store_is_exclusive_while_its_discharge_power_is_reserved() {
        let (registries, mut state, source, destination, first_heater, energy_store) =
            make_loaded_fixture(EnergyCarrier::Electrical);
        if let Err(error) = deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(10),
            Temperature::from_millikelvin(300_000),
        ) {
            panic!("energy occupancy second input failed: {error}");
        }
        let second_heater =
            match add_equipment(&registries, &mut state, HEATER, Condition::PRISTINE) {
                Ok(equipment) => equipment,
                Err(error) => panic!("energy occupancy second heater failed: {error}"),
            };
        let target = Temperature::from_millikelvin(303_000);
        let first = match resolve_test_sensible_heating_process(
            &registries,
            &state,
            PROCESS,
            source,
            first_heater,
            energy_store,
            target,
        ) {
            Ok(resolved) => resolved,
            Err(error) => panic!("energy occupancy first resolution failed: {error}"),
        };
        let duration = first.process_resolution().duration();
        let token = match validate_start_process(
            &registries,
            &state,
            first.process_resolution(),
            source,
            destination,
        ) {
            Ok(token) => token,
            Err(error) => panic!("energy occupancy first validation failed: {error}"),
        };
        let first_job = match token.commit(&mut state) {
            Ok(job) => job,
            Err(error) => panic!("energy occupancy first commit failed: {error}"),
        };
        let completes_at = match state.production().get_job(first_job) {
            Some(job) => job.completes_at(),
            None => panic!("energy occupancy first job disappeared"),
        };

        assert_eq!(
            resolve_test_sensible_heating_process(
                &registries,
                &state,
                PROCESS,
                source,
                second_heater,
                energy_store,
                target,
            ),
            Err(SensibleHeatingResolutionError::Energy(
                EnergySupplyError::StoreBusy {
                    store: energy_store,
                    job: first_job,
                    completes_at,
                }
            ))
        );

        for _ in 0..duration.value() {
            if let Err(error) = advance_tick(&registries, &mut state) {
                panic!("energy occupancy completion failed: {error}");
            }
        }
        assert!(state.production().get_job(first_job).is_none());
        assert!(
            resolve_test_sensible_heating_process(
                &registries,
                &state,
                PROCESS,
                source,
                second_heater,
                energy_store,
                target,
            )
            .is_ok()
        );
    }

    #[test]
    fn validated_heating_start_rejects_stale_equipment_before_consuming_other_resources() {
        let (registries, mut state, source, destination, equipment, energy_store) =
            make_loaded_fixture(EnergyCarrier::Electrical);
        let resolved = match resolve_test_sensible_heating_process(
            &registries,
            &state,
            PROCESS,
            source,
            equipment,
            energy_store,
            Temperature::from_millikelvin(303_000),
        ) {
            Ok(resolved) => resolved,
            Err(error) => panic!("stale equipment fixture resolution failed: {error}"),
        };
        let token = match validate_start_process(
            &registries,
            &state,
            resolved.process_resolution(),
            source,
            destination,
        ) {
            Ok(token) => token,
            Err(error) => panic!("stale equipment fixture validation failed: {error}"),
        };
        let expected = state.equipment().revision();
        if let Err(error) = add_equipment(&registries, &mut state, HEATER, Condition::PRISTINE) {
            panic!("independent equipment mutation failed: {error}");
        }
        let before = state.clone();

        assert_eq!(
            token.commit(&mut state),
            Err(
                crate::production::StartProcessCommitError::StaleEquipmentRevision {
                    expected,
                    actual: expected + 1,
                }
            )
        );
        assert_eq!(state, before);
        assert_eq!(state.production().jobs().count(), 0);
    }
}
