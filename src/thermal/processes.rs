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
use crate::equipment::{
    EquipmentId, EquipmentProviderError, resolve_equipment_capability, resolve_equipment_provider,
};
use crate::inventory::{MaterialLotSelection, StockpileId};
use crate::maintenance::Condition;
use crate::material::{MaterialLotSpec, MaterialLotSpecError, MaterialPhase, MaterialRegistry};
use crate::production::{
    ProcessId, ProcessInputError, ProcessInputPolicy, ProcessResolution, ProcessResolutionError,
    ProductionJobId, ProductionJobRecord, ProductionRegistry, validate_selected_process_inputs,
};
use crate::registry::Registries;

use super::casting_execution::{
    CastingJobValidationError, CastingProcessDefinition, validate_loaded_casting_job,
};
use super::melting_execution::{
    MeltingJobValidationError, MeltingProcessDefinition, validate_loaded_melting_job,
};
use super::{
    HeatDirection, PhaseSensibleHeatError, calculate_phase_sensible_heat,
    condition_after_active_ticks,
};

/// Immutable declaration that one process is resolved as ideal sensible heating.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SensibleHeatingProcessDefinition {
    process: ProcessId,
    heating_power_capability: CapabilityId,
    max_temperature_capability: CapabilityId,
    max_batch_mass_capability: CapabilityId,
    energy_carrier: EnergyCarrier,
    condition_wear_ppm_per_active_tick: u32,
}

impl SensibleHeatingProcessDefinition {
    #[must_use]
    pub const fn new(
        process: ProcessId,
        heating_power_capability: CapabilityId,
        max_temperature_capability: CapabilityId,
        max_batch_mass_capability: CapabilityId,
        energy_carrier: EnergyCarrier,
        condition_wear_ppm_per_active_tick: u32,
    ) -> Self {
        Self {
            process,
            heating_power_capability,
            max_temperature_capability,
            max_batch_mass_capability,
            energy_carrier,
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

    /// Returns baseline condition loss for each authoritative tick spent actively running.
    #[must_use]
    pub const fn condition_wear_ppm_per_active_tick(self) -> u32 {
        self.condition_wear_ppm_per_active_tick
    }
}

/// Immutable lookup table for process-specific thermal resolution semantics.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThermalRegistry {
    sensible_heating: BTreeMap<ProcessId, SensibleHeatingProcessDefinition>,
    melting: BTreeMap<ProcessId, MeltingProcessDefinition>,
    casting: BTreeMap<ProcessId, CastingProcessDefinition>,
}

impl ThermalRegistry {
    pub(crate) fn new(
        sensible_heating_definitions: impl IntoIterator<Item = SensibleHeatingProcessDefinition>,
        melting_definitions: impl IntoIterator<Item = MeltingProcessDefinition>,
        casting_definitions: impl IntoIterator<Item = CastingProcessDefinition>,
    ) -> Self {
        let mut sensible_heating = BTreeMap::new();
        for definition in sensible_heating_definitions {
            let process = definition.process();
            assert!(
                sensible_heating.insert(process, definition).is_none(),
                "duplicate sensible-heating definition for process {}",
                process.value()
            );
        }
        let mut melting = BTreeMap::new();
        for definition in melting_definitions {
            let process = definition.process();
            assert!(
                !sensible_heating.contains_key(&process),
                "thermal process {} cannot be registered as both sensible heating and melting",
                process.value()
            );
            assert!(
                melting.insert(process, definition).is_none(),
                "duplicate melting definition for process {}",
                process.value()
            );
        }
        let mut casting = BTreeMap::new();
        for definition in casting_definitions {
            let process = definition.process();
            assert!(
                !sensible_heating.contains_key(&process) && !melting.contains_key(&process),
                "thermal process {} cannot be registered under multiple thermal resolvers",
                process.value()
            );
            assert!(
                casting.insert(process, definition).is_none(),
                "duplicate casting definition for process {}",
                process.value()
            );
        }
        Self {
            sensible_heating,
            melting,
            casting,
        }
    }

    #[must_use]
    pub fn get_sensible_heating(
        &self,
        process: ProcessId,
    ) -> Option<SensibleHeatingProcessDefinition> {
        self.sensible_heating.get(&process).copied()
    }

    #[must_use]
    pub fn get_melting(&self, process: ProcessId) -> Option<MeltingProcessDefinition> {
        self.melting.get(&process).copied()
    }

    #[must_use]
    pub fn get_casting(&self, process: ProcessId) -> Option<CastingProcessDefinition> {
        self.casting.get(&process).copied()
    }

    pub(crate) fn validate_references(
        &self,
        production: &ProductionRegistry,
        capabilities: &CapabilityRegistry,
        materials: &MaterialRegistry,
    ) {
        for definition in self.sensible_heating.values().copied() {
            validate_common_thermal_references(
                definition.process(),
                definition.heating_power_capability(),
                definition.max_temperature_capability(),
                definition.max_batch_mass_capability(),
                production,
                capabilities,
            );
        }
        for definition in self.casting.values().copied() {
            validate_common_thermal_references(
                definition.process(),
                definition.cooling_power_capability(),
                definition.max_temperature_capability(),
                definition.max_batch_mass_capability(),
                production,
                capabilities,
            );
            let Some(form) = materials.get_form(definition.solid_form()) else {
                panic!(
                    "casting process {} references missing output form {}",
                    definition.process().value(),
                    definition.solid_form().value()
                );
            };
            assert_eq!(
                form.phase(),
                MaterialPhase::Solid,
                "casting process {} output form {} must be solid",
                definition.process().value(),
                definition.solid_form().value()
            );
        }
        for definition in self.melting.values().copied() {
            validate_common_thermal_references(
                definition.process(),
                definition.heating_power_capability(),
                definition.max_temperature_capability(),
                definition.max_batch_mass_capability(),
                production,
                capabilities,
            );
            let Some(form) = materials.get_form(definition.liquid_form()) else {
                panic!(
                    "melting process {} references missing output form {}",
                    definition.process().value(),
                    definition.liquid_form().value()
                );
            };
            assert_eq!(
                form.phase(),
                MaterialPhase::Liquid,
                "melting process {} output form {} must be liquid",
                definition.process().value(),
                definition.liquid_form().value()
            );
        }
    }
}

fn validate_common_thermal_references(
    process: ProcessId,
    thermal_power_capability: CapabilityId,
    max_temperature_capability: CapabilityId,
    max_batch_mass_capability: CapabilityId,
    production: &ProductionRegistry,
    capabilities: &CapabilityRegistry,
) {
    let process_definition = match production.get_process(process) {
        Some(definition) => definition,
        None => panic!(
            "thermal definition references missing process {}",
            process.value()
        ),
    };
    assert!(
        matches!(
            process_definition.input_policy(),
            ProcessInputPolicy::SelectedBatch
        ),
        "thermal process {} must use selected-batch input policy",
        process.value()
    );
    let power = match capabilities.get_capability(thermal_power_capability) {
        Some(capability) => capability,
        None => panic!(
            "thermal process {} references missing thermal-transfer-power capability {}",
            process.value(),
            thermal_power_capability.value()
        ),
    };
    assert_eq!(
        power.kind(),
        CapabilityValueKind::Power,
        "thermal process {} thermal-transfer-power capability must be Power",
        process.value()
    );
    let maximum = match capabilities.get_capability(max_temperature_capability) {
        Some(capability) => capability,
        None => panic!(
            "thermal process {} references missing maximum-temperature capability {}",
            process.value(),
            max_temperature_capability.value()
        ),
    };
    assert_eq!(
        maximum.kind(),
        CapabilityValueKind::Temperature,
        "thermal process {} maximum-temperature capability must be Temperature",
        process.value()
    );
    let maximum_batch = match capabilities.get_capability(max_batch_mass_capability) {
        Some(capability) => capability,
        None => panic!(
            "thermal process {} references missing maximum-batch-mass capability {}",
            process.value(),
            max_batch_mass_capability.value()
        ),
    };
    assert_eq!(
        maximum_batch.kind(),
        CapabilityValueKind::Mass,
        "thermal process {} maximum-batch-mass capability must be Mass",
        process.value()
    );
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
    Heat(PhaseSensibleHeatError),
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
        &provider,
        process_definition.capability_requirements(),
    )
    .map_err(SensibleHeatingResolutionError::Capability)?;

    let heating_power = match provider.get_capability(definition.heating_power_capability()) {
        Some(CapabilityValue::Power(power)) => power,
        Some(_) | None => {
            return Err(SensibleHeatingResolutionError::MissingHeatingPower {
                capability: definition.heating_power_capability(),
            });
        }
    };
    let maximum_temperature = match provider.get_capability(definition.max_temperature_capability())
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
    let maximum_batch_mass = match provider.get_capability(definition.max_batch_mass_capability()) {
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
        let heat = calculate_phase_sensible_heat(
            registries.materials(),
            trace.mass(),
            profile.commodity(),
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
    let equipment_condition_after = condition_after_active_ticks(
        definition.condition_wear_ppm_per_active_tick(),
        provider.condition(),
        duration,
    );

    let mut outputs = Vec::with_capacity(output_masses.len());
    for ((commodity, composition), mass) in output_masses {
        let output = MaterialLotSpec::with_composition(commodity, mass, target, composition)
            .map_err(SensibleHeatingResolutionError::Output)?;
        outputs.push(output);
    }
    let resolution = inputs
        .resolve_with_energy_and_equipment(
            duration,
            outputs,
            energy_supply,
            equipment_use,
            equipment_condition_after,
        )
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
    Casting(CastingJobValidationError),
    Melting(MeltingJobValidationError),
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
    WrongEnergyCarrier {
        job: ProductionJobId,
        required: EnergyCarrier,
        provided: EnergyCarrier,
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
        error: PhaseSensibleHeatError,
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
    MissingEquipmentConditionOutcome {
        job: ProductionJobId,
    },
    EquipmentConditionOutcomeMismatch {
        job: ProductionJobId,
        stored: Condition,
        required: Condition,
    },
}

impl Display for ThermalJobValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Casting(error) => write!(formatter, "invalid casting job: {error}"),
            Self::Melting(error) => write!(formatter, "invalid melting job: {error}"),
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
            Self::WrongEnergyCarrier {
                job,
                required,
                provided,
            } => write!(
                formatter,
                "sensible-heating job {} requires {required:?} energy but traces {provided:?}",
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
            Self::MissingEquipmentConditionOutcome { job } => write!(
                formatter,
                "sensible-heating job {} has no post-operation equipment condition",
                job.value()
            ),
            Self::EquipmentConditionOutcomeMismatch {
                job,
                stored,
                required,
            } => write!(
                formatter,
                "sensible-heating job {} stores post-operation condition {} ppm but active-time wear requires {} ppm",
                job.value(),
                stored.parts_per_million(),
                required.parts_per_million()
            ),
        }
    }
}

impl Error for ThermalJobValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Casting(error) => Some(error),
            Self::Melting(error) => Some(error),
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
            | Self::WrongEnergyCarrier { .. }
            | Self::MixedOutputTemperatures { .. }
            | Self::TargetBelowInputTemperature { .. }
            | Self::RequiredEnergyOverflow { .. }
            | Self::EnergyMismatch { .. }
            | Self::OutputMismatch { .. }
            | Self::DurationMismatch { .. }
            | Self::MissingEquipmentConditionOutcome { .. }
            | Self::EquipmentConditionOutcomeMismatch { .. } => None,
        }
    }
}

/// Recomputes the physical contract of an in-flight thermal job from persisted input traces.
///
/// Operation-specific validators use the same pure physical derivation used during runtime
/// resolution so save tampering cannot silently alter required energy, duration, wear, or output.
pub(crate) fn validate_loaded_thermal_job(
    registries: &Registries,
    job: &ProductionJobRecord,
) -> Result<(), ThermalJobValidationError> {
    if let Some(definition) = registries.thermal().get_casting(job.process()) {
        return validate_loaded_casting_job(registries, job, definition)
            .map_err(ThermalJobValidationError::Casting);
    }
    if let Some(definition) = registries.thermal().get_melting(job.process()) {
        return validate_loaded_melting_job(registries, job, definition)
            .map_err(ThermalJobValidationError::Melting);
    }
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
    if consumed_energy.carrier() != thermal_definition.energy_carrier() {
        return Err(ThermalJobValidationError::WrongEnergyCarrier {
            job: job.id(),
            required: thermal_definition.energy_carrier(),
            provided: consumed_energy.carrier(),
        });
    }
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
    let heating_power = match resolve_equipment_capability(
        equipment_definition,
        provider.condition(),
        thermal_definition.heating_power_capability(),
    ) {
        Some(CapabilityValue::Power(power)) => power,
        Some(_) | None => {
            return Err(ThermalJobValidationError::MissingHeatingPowerCapability { job: job.id() });
        }
    };
    let maximum_temperature = match resolve_equipment_capability(
        equipment_definition,
        provider.condition(),
        thermal_definition.max_temperature_capability(),
    ) {
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
    let maximum_batch_mass = match resolve_equipment_capability(
        equipment_definition,
        provider.condition(),
        thermal_definition.max_batch_mass_capability(),
    ) {
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
        let heat = calculate_phase_sensible_heat(
            registries.materials(),
            trace.mass(),
            profile.commodity(),
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
    let required_condition_after = condition_after_active_ticks(
        thermal_definition.condition_wear_ppm_per_active_tick(),
        provider.condition(),
        required_duration,
    );
    let Some(stored_condition_after) = job.equipment_condition_after() else {
        return Err(ThermalJobValidationError::MissingEquipmentConditionOutcome { job: job.id() });
    };
    if stored_condition_after != required_condition_after {
        return Err(
            ThermalJobValidationError::EquipmentConditionOutcomeMismatch {
                job: job.id(),
                stored: stored_condition_after,
                required: required_condition_after,
            },
        );
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
        FORM_LOG, FORM_MOLTEN, FORM_ORE, MATERIAL_COPPER, MATERIAL_WOOD,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION, make_test_registries_with_sensible_heating,
    };
    use crate::core::quantity::{Area, Force, Mass, Power};
    use crate::core::state::validate_loaded_state;
    use crate::core::time::{SimulationTick, WorldSeed};
    use crate::energy::{
        EnergyStoreDefinition, EnergyStoreDefinitionId, add_energy_store,
        add_energy_store_with_initial_for_test, calculate_explicit_energy_accounting,
    };
    use crate::equipment::{
        CapabilityConditionCurve, CapabilityConditionPoint, EquipmentConditionCommitError,
        EquipmentConditionPlanError, EquipmentDefinition, EquipmentDefinitionId,
        EquipmentSupportCommitError, add_equipment, apply_equipment_condition_plan,
        decide_equipment_wear, validate_mount_equipment,
    };
    use crate::inventory::{
        StockpileStorageProfile, add_stockpile, add_stockpile_with_storage_profile,
        deposit_lot_for_test,
    };
    use crate::maintenance::{Condition, MaintenanceThresholds};
    use crate::material::{CommodityKey, MaterialComposition};
    use crate::matter::calculate_matter_accounting;
    use crate::production::{
        CompletionCommitError, ProcessDefinition, StartProcessCommitError, StartProcessError,
        apply_completion_plan, decide_due_completions, validate_start_process,
    };
    use crate::simulation::advance_tick;
    use crate::spatial::{VoxelBounds, VoxelCoord};
    use crate::structural::{
        StructuralElementId, StructuralLifecycle, StructuralLoadKind, add_structural_element,
        materialize_structural_element_for_test, validate_activate_structural_element,
        validate_set_structural_load,
    };

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

    #[test]
    fn sensible_heating_can_superheat_liquid_without_reapplying_fusion_energy() {
        let registries = make_registries_with_max_temperature(
            EnergyCarrier::Electrical,
            Temperature::from_millikelvin(1_500_000),
        );
        let mut state = AppState::new(WorldSeed::new(0x9200_0101));
        let liquid_profile = match StockpileStorageProfile::new(
            false,
            true,
            Temperature::from_millikelvin(1_500_000),
        ) {
            Ok(profile) => profile,
            Err(error) => panic!("liquid heating storage profile failed: {error}"),
        };
        let source = match add_stockpile_with_storage_profile(
            &mut state,
            Mass::from_milligrams(100),
            liquid_profile,
        ) {
            Ok(source) => source,
            Err(error) => panic!("liquid heating source failed: {error}"),
        };
        let destination = match add_stockpile_with_storage_profile(
            &mut state,
            Mass::from_milligrams(100),
            liquid_profile,
        ) {
            Ok(destination) => destination,
            Err(error) => panic!("liquid heating destination failed: {error}"),
        };
        let melting_point = Temperature::from_millikelvin(1_357_770);
        let target = Temperature::from_millikelvin(1_400_000);
        let lot = match deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN),
            Mass::from_milligrams(10),
            melting_point,
        ) {
            Ok(lot) => lot,
            Err(error) => panic!("liquid heating input failed: {error}"),
        };
        let equipment = match add_equipment(&registries, &mut state, HEATER, Condition::PRISTINE) {
            Ok(equipment) => equipment,
            Err(error) => panic!("liquid heating equipment failed: {error}"),
        };
        let energy_store = match add_energy_store_with_initial_for_test(
            &registries,
            &mut state,
            BATTERY,
            Energy::from_nanojoules(1_000_000_000),
        ) {
            Ok(store) => store,
            Err(error) => panic!("liquid heating energy store failed: {error}"),
        };
        let initial_energy = match calculate_explicit_energy_accounting(&registries, &state)
            .and_then(|accounting| {
                accounting
                    .total()
                    .ok_or(crate::energy::ExplicitEnergyAccountingError::Overflow)
            }) {
            Ok(total) => total,
            Err(error) => panic!("liquid heating initial accounting failed: {error}"),
        };
        let expected_heat = match calculate_phase_sensible_heat(
            registries.materials(),
            Mass::from_milligrams(10),
            CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN),
            &MaterialComposition::pure(MATERIAL_COPPER),
            melting_point,
            target,
        ) {
            Ok(heat) => heat.energy(),
            Err(error) => panic!("liquid heating expected heat failed: {error}"),
        };

        let resolved = match resolve_sensible_heating_process(
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
            Err(error) => panic!("liquid sensible-heating resolution failed: {error}"),
        };
        assert_eq!(resolved.required_energy(), expected_heat);
        assert_eq!(
            resolved.process_resolution().outputs()[0].commodity(),
            CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN)
        );
        let duration = resolved.process_resolution().duration();
        let token = match validate_start_process(
            &registries,
            &state,
            resolved.process_resolution(),
            source,
            destination,
        ) {
            Ok(token) => token,
            Err(error) => panic!("liquid heating start validation failed: {error}"),
        };
        if let Err(error) = token.commit(&mut state) {
            panic!("liquid heating start commit failed: {error}");
        }
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
        assert_eq!(
            calculate_explicit_energy_accounting(&registries, &state)
                .ok()
                .and_then(|accounting| accounting.total()),
            Some(initial_energy)
        );

        for _ in 0..duration.value() {
            if let Err(error) = advance_tick(&registries, &mut state) {
                panic!("liquid heating completion failed: {error}");
            }
        }
        let output = match state
            .inventory()
            .lots()
            .find(|candidate| candidate.stockpile() == destination)
        {
            Some(output) => output,
            None => panic!("liquid heating output missing"),
        };
        assert_eq!(
            output.commodity(),
            CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN)
        );
        assert_eq!(output.temperature(), target);
        assert_eq!(
            calculate_explicit_energy_accounting(&registries, &state)
                .ok()
                .and_then(|accounting| accounting.total()),
            Some(initial_energy)
        );
    }

    fn make_registries_with_max_temperature(
        carrier: EnergyCarrier,
        maximum_temperature: Temperature,
    ) -> Registries {
        make_registries_with_condition_curves(carrier, maximum_temperature, Vec::new())
    }

    fn make_registries_with_condition_curves(
        carrier: EnergyCarrier,
        maximum_temperature: Temperature,
        curves: Vec<CapabilityConditionCurve>,
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
        let equipment = EquipmentDefinition::new_with_capability_condition_curves(
            HEATER,
            "test resistive heater",
            Mass::from_milligrams(1_000_000),
            capabilities,
            thresholds,
            curves,
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
                1_000,
            ),
        )
    }

    fn add_active_support(
        registries: &Registries,
        state: &mut AppState,
        x: i64,
    ) -> StructuralElementId {
        let bounds = match VoxelBounds::new(VoxelCoord::new(x, 0, 0), VoxelCoord::new(x + 1, 1, 1))
        {
            Ok(bounds) => bounds,
            Err(error) => panic!("heater-support bounds fixture failed: {error}"),
        };
        let support = match add_structural_element(
            registries,
            state,
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            crate::structural::make_test_structural_geometry(
                bounds,
                crate::core::quantity::Length::from_micrometers(1),
                Area::from_square_millimeters(1_000),
            ),
            true,
        ) {
            Ok(element) => element,
            Err(error) => panic!("heater-support structural fixture failed: {error}"),
        };
        materialize_structural_element_for_test(registries, state, support, FORM_LOG);
        let activation = match validate_activate_structural_element(registries, state, support) {
            Ok(token) => token,
            Err(error) => panic!("heater-support activation validation failed: {error}"),
        };
        if let Err(error) = activation.commit(state) {
            panic!("heater-support activation commit failed: {error}");
        }
        support
    }

    fn fail_support(registries: &Registries, state: &mut AppState, support: StructuralElementId) {
        let overload = match validate_set_structural_load(
            registries,
            state,
            support,
            StructuralLoadKind::Snow,
            Force::from_millinewtons(50_000_000),
        ) {
            Ok(token) => token,
            Err(error) => panic!("heater-support overload validation failed: {error}"),
        };
        if let Err(error) = overload.commit(state) {
            panic!("heater-support overload commit failed: {error}");
        }
        assert_eq!(
            state
                .structures()
                .get_element(support)
                .map(|record| record.lifecycle()),
            Some(StructuralLifecycle::Failed)
        );
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
        make_loaded_fixture_with_registries(
            registries,
            Condition::PRISTINE,
            input_temperature,
            initial_energy,
        )
    }

    fn make_loaded_fixture_with_registries(
        registries: Registries,
        equipment_condition: Condition,
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
        let equipment = match add_equipment(&registries, &mut state, HEATER, equipment_condition) {
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
        let expected_heat = match calculate_phase_sensible_heat(
            registries.materials(),
            Mass::from_milligrams(10),
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
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
        assert_eq!(
            resolved.process_resolution().equipment_condition_after(),
            Some(condition(997_000))
        );

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
        assert_eq!(
            state
                .production()
                .get_job(job)
                .and_then(|record| record.equipment_condition_after()),
            Some(condition(997_000))
        );
        assert_eq!(
            state
                .equipment()
                .get_equipment(equipment)
                .map(|record| record.condition()),
            Some(Condition::PRISTINE)
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
        assert_eq!(
            state
                .equipment()
                .get_equipment(equipment)
                .map(|record| record.condition()),
            Some(condition(997_000))
        );
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
    fn worn_heater_derates_transfer_power_and_persisted_duration_contract() {
        let curve = CapabilityConditionCurve::new(
            HEATING_POWER,
            vec![
                CapabilityConditionPoint::new(
                    Condition::FAILED,
                    CapabilityValue::Power(Power::from_microwatts(100_000)),
                ),
                CapabilityConditionPoint::new(
                    condition(500_000),
                    CapabilityValue::Power(Power::from_microwatts(250_000)),
                ),
            ],
        );
        let registries = make_registries_with_condition_curves(
            EnergyCarrier::Electrical,
            Temperature::from_millikelvin(400_000),
            vec![curve],
        );
        let (registries, mut state, source, destination, equipment, energy_store) =
            make_loaded_fixture_with_registries(
                registries,
                condition(500_000),
                Temperature::from_millikelvin(300_000),
                Energy::from_nanojoules(500_000_000),
            );

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
            Err(error) => panic!("worn-heater resolution failed: {error}"),
        };
        assert_eq!(
            resolved.required_energy(),
            Energy::from_nanojoules(51_000_000)
        );
        assert_eq!(resolved.transfer_power(), Power::from_microwatts(250_000));
        assert_eq!(resolved.process_resolution().duration().value(), 5);

        let token = match validate_start_process(
            &registries,
            &state,
            resolved.process_resolution(),
            source,
            destination,
        ) {
            Ok(token) => token,
            Err(error) => panic!("worn-heater process start validation failed: {error}"),
        };
        let job = match token.commit(&mut state) {
            Ok(job) => job,
            Err(error) => panic!("worn-heater process start commit failed: {error}"),
        };
        let provider = match state
            .production()
            .get_job(job)
            .and_then(|record| record.equipment_provider())
        {
            Some(provider) => provider,
            None => panic!("worn-heater job lost its equipment trace"),
        };
        assert_eq!(provider.condition(), condition(500_000));
        assert_eq!(
            state
                .production()
                .get_job(job)
                .and_then(|record| record.equipment_condition_after()),
            Some(condition(495_000))
        );
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

        for _ in 0..5 {
            if let Err(error) = advance_tick(&registries, &mut state) {
                panic!("worn-heater completion failed: {error}");
            }
        }
        assert_eq!(
            state
                .equipment()
                .get_equipment(equipment)
                .map(|record| record.condition()),
            Some(condition(495_000))
        );
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
                PhaseSensibleHeatError::InvalidTargetState(
                    crate::material::MaterialPhaseStateError::SolidAboveMeltingPoint { .. }
                )
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
        assert_eq!(
            first
                .equipment()
                .get_equipment(EquipmentId::new(1))
                .map(|record| record.condition()),
            Some(condition(955_000))
        );
    }

    #[test]
    fn same_tick_heating_completions_apply_all_wear_under_one_equipment_revision() {
        let (registries, mut state, source, destination, first_equipment, first_energy) =
            make_loaded_fixture(EnergyCarrier::Electrical);
        if let Err(error) = deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(10),
            Temperature::from_millikelvin(300_000),
        ) {
            panic!("same-tick wear second input fixture failed: {error}");
        }
        let second_equipment =
            match add_equipment(&registries, &mut state, HEATER, Condition::PRISTINE) {
                Ok(equipment) => equipment,
                Err(error) => panic!("same-tick wear second equipment fixture failed: {error}"),
            };
        let second_energy = match add_energy_store_with_initial_for_test(
            &registries,
            &mut state,
            BATTERY,
            Energy::from_nanojoules(500_000_000),
        ) {
            Ok(store) => store,
            Err(error) => panic!("same-tick wear second energy fixture failed: {error}"),
        };
        let target = Temperature::from_millikelvin(303_000);

        let first = match resolve_test_sensible_heating_process(
            &registries,
            &state,
            PROCESS,
            source,
            first_equipment,
            first_energy,
            target,
        ) {
            Ok(resolved) => resolved,
            Err(error) => panic!("same-tick wear first resolution failed: {error}"),
        };
        let duration = first.process_resolution().duration();
        let first_start = match validate_start_process(
            &registries,
            &state,
            first.process_resolution(),
            source,
            destination,
        ) {
            Ok(token) => token,
            Err(error) => panic!("same-tick wear first start validation failed: {error}"),
        };
        if let Err(error) = first_start.commit(&mut state) {
            panic!("same-tick wear first start commit failed: {error}");
        }

        let second = match resolve_test_sensible_heating_process(
            &registries,
            &state,
            PROCESS,
            source,
            second_equipment,
            second_energy,
            target,
        ) {
            Ok(resolved) => resolved,
            Err(error) => panic!("same-tick wear second resolution failed: {error}"),
        };
        assert_eq!(second.process_resolution().duration(), duration);
        let second_start = match validate_start_process(
            &registries,
            &state,
            second.process_resolution(),
            source,
            destination,
        ) {
            Ok(token) => token,
            Err(error) => panic!("same-tick wear second start validation failed: {error}"),
        };
        if let Err(error) = second_start.commit(&mut state) {
            panic!("same-tick wear second start commit failed: {error}");
        }

        let equipment_revision_before_completion = state.equipment().revision();
        for _ in 1..duration.value() {
            let outcome = match advance_tick(&registries, &mut state) {
                Ok(outcome) => outcome,
                Err(error) => panic!("same-tick wear pre-completion tick failed: {error}"),
            };
            assert!(outcome.production_completions().is_empty());
            assert_eq!(
                state.equipment().revision(),
                equipment_revision_before_completion
            );
        }

        let completion = match advance_tick(&registries, &mut state) {
            Ok(outcome) => outcome,
            Err(error) => panic!("same-tick wear completion tick failed: {error}"),
        };
        assert_eq!(completion.production_completions().len(), 2);
        assert_eq!(
            state.equipment().revision(),
            equipment_revision_before_completion + 1
        );
        for equipment in [first_equipment, second_equipment] {
            assert_eq!(
                state
                    .equipment()
                    .get_equipment(equipment)
                    .map(|record| record.condition()),
                Some(condition(997_000))
            );
        }
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }

    #[test]
    fn sensible_heating_rejects_heater_after_mounted_support_fails() {
        let (registries, mut state, source, _, equipment, energy_store) =
            make_loaded_fixture(EnergyCarrier::Electrical);
        let support = add_active_support(&registries, &mut state, 0);
        let mount = match validate_mount_equipment(&registries, &state, equipment, support) {
            Ok(token) => token,
            Err(error) => panic!("heater-support mount validation failed: {error}"),
        };
        if let Err(error) = mount.commit(&mut state) {
            panic!("heater-support mount commit failed: {error}");
        }
        fail_support(&registries, &mut state, support);

        assert!(matches!(
            resolve_test_sensible_heating_process(
                &registries,
                &state,
                PROCESS,
                source,
                equipment,
                energy_store,
                Temperature::from_millikelvin(303_000),
            ),
            Err(SensibleHeatingResolutionError::Equipment(
                EquipmentProviderError::StructuralSupportNotActive {
                    equipment: rejected_equipment,
                    element,
                    lifecycle: StructuralLifecycle::Failed,
                }
            )) if rejected_equipment == equipment && element == support
        ));
    }

    #[test]
    fn resolved_heating_becomes_stale_when_support_changes_before_start_validation() {
        let (registries, mut state, source, destination, equipment, energy_store) =
            make_loaded_fixture(EnergyCarrier::Electrical);
        let support = add_active_support(&registries, &mut state, 0);
        let mount = match validate_mount_equipment(&registries, &state, equipment, support) {
            Ok(token) => token,
            Err(error) => panic!("stale-support mount validation failed: {error}"),
        };
        if let Err(error) = mount.commit(&mut state) {
            panic!("stale-support mount commit failed: {error}");
        }
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
            Err(error) => panic!("stale-support heating resolution failed: {error}"),
        };
        let expected_structure_revision = state.structures().revision();
        fail_support(&registries, &mut state, support);

        assert_eq!(
            validate_start_process(
                &registries,
                &state,
                resolved.process_resolution(),
                source,
                destination,
            ),
            Err(StartProcessError::StaleResolvedStructure {
                expected_structure_revision,
                actual_structure_revision: expected_structure_revision + 1,
            })
        );
    }

    #[test]
    fn validated_heating_start_rejects_support_change_before_commit_without_consuming_resources() {
        let (registries, mut state, source, destination, equipment, energy_store) =
            make_loaded_fixture(EnergyCarrier::Electrical);
        let support = add_active_support(&registries, &mut state, 0);
        let mount = match validate_mount_equipment(&registries, &state, equipment, support) {
            Ok(token) => token,
            Err(error) => panic!("commit-race mount validation failed: {error}"),
        };
        if let Err(error) = mount.commit(&mut state) {
            panic!("commit-race mount commit failed: {error}");
        }
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
            Err(error) => panic!("commit-race heating resolution failed: {error}"),
        };
        let start = match validate_start_process(
            &registries,
            &state,
            resolved.process_resolution(),
            source,
            destination,
        ) {
            Ok(token) => token,
            Err(error) => panic!("commit-race start validation failed: {error}"),
        };
        let expected_structure_revision = state.structures().revision();
        fail_support(&registries, &mut state, support);
        let before = state.clone();

        assert_eq!(
            start.commit(&mut state),
            Err(StartProcessCommitError::StaleStructureRevision {
                expected: expected_structure_revision,
                actual: expected_structure_revision + 1,
            })
        );
        assert_eq!(state, before);
    }

    #[test]
    fn prevalidated_maintenance_and_mount_are_blocked_if_job_starts_first() {
        let (registries, mut state, source, destination, equipment, energy_store) =
            make_loaded_fixture(EnergyCarrier::Electrical);
        let support = add_active_support(&registries, &mut state, 0);
        let wear = match decide_equipment_wear(&state, equipment, 1) {
            Ok(plan) => plan,
            Err(error) => panic!("occupancy-race wear validation failed: {error}"),
        };
        let mount = match validate_mount_equipment(&registries, &state, equipment, support) {
            Ok(token) => token,
            Err(error) => panic!("occupancy-race mount validation failed: {error}"),
        };
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
            Err(error) => panic!("occupancy-race heating resolution failed: {error}"),
        };
        let start = match validate_start_process(
            &registries,
            &state,
            resolved.process_resolution(),
            source,
            destination,
        ) {
            Ok(token) => token,
            Err(error) => panic!("occupancy-race start validation failed: {error}"),
        };
        let job = match start.commit(&mut state) {
            Ok(job) => job,
            Err(error) => panic!("occupancy-race start commit failed: {error}"),
        };
        let completes_at = match state.production().get_job(job) {
            Some(record) => record.completes_at(),
            None => panic!("occupancy-race job disappeared"),
        };

        let before_wear = state.clone();
        assert_eq!(
            apply_equipment_condition_plan(&mut state, wear),
            Err(EquipmentConditionCommitError::EquipmentBusy {
                equipment,
                job,
                completes_at,
            })
        );
        assert_eq!(state, before_wear);

        let before_mount = state.clone();
        assert_eq!(
            mount.commit(&mut state),
            Err(EquipmentSupportCommitError::EquipmentBusy {
                equipment,
                job,
                completes_at,
            })
        );
        assert_eq!(state, before_mount);
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
    fn due_heating_completion_rejects_stale_equipment_revision_atomically() {
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
            Err(error) => panic!("completion-race heating resolution failed: {error}"),
        };
        let duration = resolved.process_resolution().duration();
        let token = match validate_start_process(
            &registries,
            &state,
            resolved.process_resolution(),
            source,
            destination,
        ) {
            Ok(token) => token,
            Err(error) => panic!("completion-race start validation failed: {error}"),
        };
        let job = match token.commit(&mut state) {
            Ok(job) => job,
            Err(error) => panic!("completion-race start commit failed: {error}"),
        };
        for _ in 1..duration.value() {
            if let Err(error) = advance_tick(&registries, &mut state) {
                panic!("completion-race pre-due tick failed: {error}");
            }
        }
        assert_eq!(state.tick(), SimulationTick::new(duration.value() - 1));
        let due = match state.production().get_job(job) {
            Some(record) => record.completes_at(),
            None => panic!("completion-race job disappeared before due planning"),
        };
        let plan = match decide_due_completions(&state, due) {
            Ok(plan) => plan,
            Err(error) => panic!("completion-race due planning failed: {error:?}"),
        };
        let expected = state.equipment().revision();
        if let Err(error) = add_equipment(&registries, &mut state, HEATER, Condition::PRISTINE) {
            panic!("completion-race independent equipment mutation failed: {error}");
        }
        let before = state.clone();

        assert_eq!(
            apply_completion_plan(&mut state, plan),
            Err(CompletionCommitError::EquipmentRevisionConflict {
                expected,
                actual: expected + 1,
            })
        );
        assert_eq!(state, before);
        assert!(state.production().get_job(job).is_some());
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
