//! Selected-batch sensible-heating resolution against exact matter, equipment, and finite energy.

use super::*;

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
            Self::UnknownThermalProcess { process: _process } => None,
            Self::MissingHeatingPower {
                capability: _capability,
            }
            | Self::MissingMaximumTemperature {
                capability: _capability,
            }
            | Self::MissingMaximumBatchMass {
                capability: _capability,
            } => None,
            Self::TargetExceedsEquipmentMaximum {
                target: _target,
                maximum: _maximum,
            } => None,
            Self::TargetBelowInputTemperature {
                current: _current,
                target: _target,
            } => None,
            Self::BatchMassExceedsEquipmentCapacity {
                selected: _selected,
                maximum: _maximum,
            } => None,
            Self::WrongEnergyCarrier {
                required: _required,
                provided: _provided,
            } => None,
            Self::RequiredEnergyOverflow | Self::NoHeatingRequired => None,
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
        let key = (
            profile.commodity(),
            profile.composition().clone(),
            profile.particle_size_distribution().cloned(),
        );
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
    let equipment_condition_after = calculate_condition_after_active_ticks(
        definition.condition_wear_ppm_per_active_tick(),
        provider.condition(),
        duration,
    );

    let mut outputs = Vec::with_capacity(output_masses.len());
    for ((commodity, composition, particle_size), mass) in output_masses {
        let output = match particle_size {
            Some(particle_size) => MaterialLotSpec::with_composition_and_particle_size(
                commodity,
                mass,
                target,
                composition,
                particle_size,
            ),
            None => MaterialLotSpec::with_composition(commodity, mass, target, composition),
        }
        .map_err(SensibleHeatingResolutionError::Output)?;
        outputs.push(output);
    }
    let resolution = inputs
        .resolve_with_energy_and_equipment(
            duration,
            vec![ProcessOutputStream::new(
                ProcessOutputStreamId::PRIMARY,
                outputs,
            )],
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
