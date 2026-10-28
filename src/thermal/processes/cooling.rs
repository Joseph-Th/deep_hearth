//! Exact solid sensible cooling, using one finite thermal sink rather than discarding heat.

mod errors;
mod validation;

pub use errors::{SensibleCoolingJobValidationError, SensibleCoolingResolutionError};
pub(in crate::thermal) use validation::validate_loaded_sensible_cooling_job;

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Mass, Power, PreciseEnergy, Temperature};
use crate::core::state::AppState;
use crate::energy::{EnergyStoreId, validate_energy_sink_access, validate_energy_sink_release};
use crate::equipment::EquipmentId;
use crate::inventory::{ConsumedMaterialTrace, MaterialLotSelection, StockpileId};
use crate::material::{CommodityKey, MaterialLotSpec, MaterialLotSpecError, MaterialRegistry};
use crate::production::{
    ProcessId, ProcessOutputStream, ProcessOutputStreamId, ProcessResolution,
    validate_process_inputs,
};
use crate::registry::Registries;

use super::super::equipment_physics::{
    ThermalEquipmentRequest, ThermalEquipmentSetupError, ThermalTransferTimingError,
    resolve_runtime_thermal_equipment, resolve_thermal_transfer_timing,
};
use super::super::physics::calculate_phase_sensible_heat_precise;
use super::super::{HeatDirection, PhaseSensibleHeatError, SensibleHeatError};
use super::registry::SensibleCoolingProcessDefinition;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SensibleCoolingBatchError {
    WrongCommodity {
        expected: CommodityKey,
        actual: CommodityKey,
    },
    InputNotHotter {
        input: Temperature,
        target: Temperature,
    },
    Heat(PhaseSensibleHeatError),
    ArithmeticOverflow,
    Output(MaterialLotSpecError),
}

impl Display for SensibleCoolingBatchError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongCommodity { expected, actual } => write!(
                formatter,
                "cooling requires commodity {} but selected commodity {}",
                expected.value(),
                actual.value()
            ),
            Self::InputNotHotter { input, target } => write!(
                formatter,
                "solid at {} mK is not hotter than cooling target {} mK",
                input.millikelvin(),
                target.millikelvin()
            ),
            Self::Heat(error) => write!(formatter, "sensible-heat calculation failed: {error}"),
            Self::ArithmeticOverflow => {
                formatter.write_str("cooling batch mass or energy overflowed")
            }
            Self::Output(error) => write!(formatter, "cooled output construction failed: {error}"),
        }
    }
}

impl Error for SensibleCoolingBatchError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Heat(error) => Some(error),
            Self::Output(error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SensibleCoolingBatch {
    energy: Energy,
    outputs: Vec<MaterialLotSpec>,
    hottest_input: Temperature,
}

fn resolve_sensible_cooling_batch(
    materials: &MaterialRegistry,
    definition: SensibleCoolingProcessDefinition,
    traces: &[ConsumedMaterialTrace],
) -> Result<SensibleCoolingBatch, SensibleCoolingBatchError> {
    let target = definition.target_temperature();
    let mut energy = PreciseEnergy::ZERO;
    let mut hottest_input = target;
    let mut output_mass_by_composition = BTreeMap::new();
    for trace in traces {
        let profile = trace.profile();
        if profile.commodity() != definition.commodity() {
            return Err(SensibleCoolingBatchError::WrongCommodity {
                expected: definition.commodity(),
                actual: profile.commodity(),
            });
        }
        if profile.temperature() <= target {
            return Err(SensibleCoolingBatchError::InputNotHotter {
                input: profile.temperature(),
                target,
            });
        }
        hottest_input = hottest_input.max(profile.temperature());
        let (released, direction) = calculate_phase_sensible_heat_precise(
            materials,
            trace.mass(),
            profile.commodity(),
            profile.composition(),
            profile.temperature(),
            target,
        )
        .map_err(SensibleCoolingBatchError::Heat)?;
        debug_assert_eq!(direction, HeatDirection::OutOfMaterial);
        energy = energy
            .checked_add(released)
            .ok_or(SensibleCoolingBatchError::ArithmeticOverflow)?;
        let current = output_mass_by_composition
            .get(profile.composition())
            .copied()
            .unwrap_or(Mass::ZERO);
        output_mass_by_composition.insert(
            profile.composition().clone(),
            current
                .checked_add(trace.mass())
                .ok_or(SensibleCoolingBatchError::ArithmeticOverflow)?,
        );
    }
    let outputs = output_mass_by_composition
        .into_iter()
        .map(|(composition, mass)| {
            MaterialLotSpec::with_composition(definition.commodity(), mass, target, composition)
                .map_err(SensibleCoolingBatchError::Output)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let femtojoule_remainder = energy.femtojoule_remainder();
    let energy = energy
        .whole_nanojoules()
        .ok_or(SensibleCoolingBatchError::Heat(
            PhaseSensibleHeatError::Heat(SensibleHeatError::FractionalNanojoule {
                femtojoule_remainder,
            }),
        ))?;
    Ok(SensibleCoolingBatch {
        energy,
        outputs,
        hottest_input,
    })
}

/// One selected solid-cooling job with an explicit physical apparatus and finite heat receiver.
#[derive(Clone, Copy, Debug)]
pub struct SensibleCoolingRequest<'selection> {
    process: ProcessId,
    source: StockpileId,
    selections: &'selection [MaterialLotSelection],
    equipment: EquipmentId,
    energy_sink: EnergyStoreId,
}

impl<'selection> SensibleCoolingRequest<'selection> {
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

/// A resolved cooling operation with exact released heat, duration, and material outputs.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct ResolvedSensibleCooling {
    resolution: ProcessResolution,
    equipment: EquipmentId,
    energy: Energy,
    target: Temperature,
    transfer_power: Power,
}

impl ResolvedSensibleCooling {
    pub const fn process_resolution(&self) -> &ProcessResolution {
        &self.resolution
    }
    #[must_use]
    pub const fn equipment(&self) -> EquipmentId {
        self.equipment
    }
    #[must_use]
    pub const fn released_energy(&self) -> Energy {
        self.energy
    }
    #[must_use]
    pub const fn target(&self) -> Temperature {
        self.target
    }
    #[must_use]
    pub const fn transfer_power(&self) -> Power {
        self.transfer_power
    }
}

fn map_equipment_error(error: ThermalEquipmentSetupError) -> SensibleCoolingResolutionError {
    use SensibleCoolingResolutionError as E;
    match error {
        ThermalEquipmentSetupError::Equipment(error) => E::Equipment(error),
        ThermalEquipmentSetupError::Capability(error) => E::Capability(error),
        ThermalEquipmentSetupError::MissingTransferPower { capability } => {
            E::MissingCoolingPower { capability }
        }
        ThermalEquipmentSetupError::MissingMaximumTemperature { capability } => {
            E::MissingMaximumTemperature { capability }
        }
        ThermalEquipmentSetupError::MissingMaximumBatchMass { capability } => {
            E::MissingMaximumBatchMass { capability }
        }
        ThermalEquipmentSetupError::BatchMassExceeded { selected, maximum } => {
            E::BatchMassExceedsEquipmentCapacity { selected, maximum }
        }
    }
}

/// Releases sensible heat from a hotter solid into a finite sink through actual equipment.
/// Output material is cooler only when the scheduled release completes.
pub fn resolve_sensible_cooling_process(
    registries: &Registries,
    state: &AppState,
    request: SensibleCoolingRequest<'_>,
) -> Result<ResolvedSensibleCooling, SensibleCoolingResolutionError> {
    use SensibleCoolingResolutionError as E;
    let definition = registries
        .thermal()
        .get_sensible_cooling(request.process)
        .ok_or(E::UnknownThermalProcess {
            process: request.process,
        })?;
    let inputs =
        validate_process_inputs(state, request.process, request.source, request.selections)
            .map_err(E::Input)?;
    let batch = resolve_sensible_cooling_batch(
        registries.materials(),
        definition,
        inputs.consumed_inputs(),
    )
    .map_err(E::Batch)?;
    let thermal_equipment = resolve_runtime_thermal_equipment(
        registries,
        state,
        ThermalEquipmentRequest::new(
            request.process,
            request.equipment,
            definition.cooling_power_capability(),
            definition.max_temperature_capability(),
            definition.max_batch_mass_capability(),
            inputs.input_mass(),
        ),
    )
    .map_err(map_equipment_error)?;
    let limits = thermal_equipment.limits();
    if batch.hottest_input > limits.maximum_temperature() {
        return Err(E::InputTemperatureExceedsEquipmentMaximum {
            input: batch.hottest_input,
            maximum: limits.maximum_temperature(),
        });
    }
    let sink = validate_energy_sink_access(registries, state, request.energy_sink)
        .map_err(E::EnergySink)?;
    if sink.carrier() != definition.energy_carrier() {
        return Err(E::WrongEnergyCarrier {
            required: definition.energy_carrier(),
            provided: sink.carrier(),
        });
    }
    let timing = resolve_thermal_transfer_timing(
        registries,
        limits.transfer_power(),
        sink.max_input_power(),
        batch.energy,
        definition.condition_wear_ppm_per_active_tick(),
        thermal_equipment.provider().condition(),
    )
    .map_err(|error| match error {
        ThermalTransferTimingError::Duration(error) => E::Duration(error),
        ThermalTransferTimingError::ConditionDuration(error) => E::ConditionDuration(error),
    })?;
    let release = validate_energy_sink_release(registries, sink, batch.energy, timing.duration())
        .map_err(E::EnergySink)?;
    let resolution = inputs
        .resolve_with_equipment_and_energy_release(
            timing.duration(),
            vec![ProcessOutputStream::new(
                ProcessOutputStreamId::PRIMARY,
                batch.outputs,
            )],
            release,
            thermal_equipment.equipment_use(),
            timing.condition_after(),
        )
        .map_err(E::Resolution)?;
    Ok(ResolvedSensibleCooling {
        resolution,
        equipment: request.equipment,
        energy: batch.energy,
        target: definition.target_temperature(),
        transfer_power: timing.transfer_power(),
    })
}
