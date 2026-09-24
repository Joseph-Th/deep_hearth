//! Finite-mechanical-work execution for unattended crafting transformations.

use crate::capability::{CapabilityEvaluationError, CapabilityValue};
use crate::core::quantity::{Energy, Mass, MassFlow, Power};
use crate::core::state::AppState;
use crate::core::throughput::{MassFlowDurationError, calculate_mass_flow_duration_ceiling};
use crate::core::time::TickSpan;
use crate::energy::{
    EnergyStoreId, EnergySupplyAccess, PowerDurationError, assess_energy_supply_access,
    calculate_mass_specific_energy, calculate_power_duration_ceiling,
    validate_energy_supply_request,
};
use crate::equipment::{
    EquipmentDefinition, EquipmentId, ValidatedEquipmentUse,
    evaluate_equipment_capabilities_at_condition, resolve_equipment_capability,
    resolve_equipment_provider,
};
use crate::inventory::{MaterialLotSelection, StockpileId};
use crate::maintenance::{
    ActiveConditionDurationError, Condition, calculate_usable_condition_after_active_ticks,
};
use crate::production::{
    ProcessId, ProcessResolution, ValidatedStartProcess, validate_process_inputs,
    validate_start_process,
};
use crate::registry::Registries;

use super::batch::{build_manual_craft_outputs, validate_manual_craft_batch};
use super::definitions::{ManualCraftDefinition, PoweredCraftDefinition};

mod errors;

use errors::batch_error;
pub use errors::{PoweredCraftError, StartPoweredCraftError};

/// Exact powered crafting request with explicit matter, machine, and finite-work source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PoweredCraftRequest {
    process: ProcessId,
    source: StockpileId,
    selections: Vec<MaterialLotSelection>,
    equipment: EquipmentId,
    energy_store: EnergyStoreId,
}

impl PoweredCraftRequest {
    #[must_use]
    pub fn new(
        process: ProcessId,
        source: StockpileId,
        selections: Vec<MaterialLotSelection>,
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

    #[must_use]
    pub fn single(
        process: ProcessId,
        source: StockpileId,
        selection: MaterialLotSelection,
        equipment: EquipmentId,
        energy_store: EnergyStoreId,
    ) -> Self {
        Self::new(process, source, vec![selection], equipment, energy_store)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PoweredCraftTimingError {
    ThroughputDuration(MassFlowDurationError),
    EnergyDuration(PowerDurationError),
    EquipmentCondition(ActiveConditionDurationError),
}

pub(super) fn resolve_powered_craft_timing(
    registries: &Registries,
    rate: MassFlow,
    mass: Mass,
    required_energy: Energy,
    available_power: Power,
    wear_ppm_per_active_tick: u32,
    condition_before: Condition,
) -> Result<(TickSpan, Condition), PoweredCraftTimingError> {
    let throughput = calculate_mass_flow_duration_ceiling(
        rate,
        mass,
        registries.core().physical_tick_duration(),
    )
    .map_err(PoweredCraftTimingError::ThroughputDuration)?;
    let energy = calculate_power_duration_ceiling(
        available_power,
        required_energy,
        registries.core().physical_tick_duration(),
    )
    .map_err(PoweredCraftTimingError::EnergyDuration)?;
    let duration = std::cmp::max(throughput, energy);
    let condition_after = calculate_usable_condition_after_active_ticks(
        wear_ppm_per_active_tick,
        condition_before,
        duration,
    )
    .map_err(PoweredCraftTimingError::EquipmentCondition)?;
    Ok((duration, condition_after))
}

/// Read-side physical schedule for one powered-craft work order after replenishing the selected
/// finite-work store to whatever level the order requires, bounded by that store's real capacity.
///
/// This projection validates the current machine, its condition-adjusted capabilities, process
/// requirements, the store carrier/output power, whole transform batches, and equipment wear. It
/// deliberately does not inspect a concrete material selection or reserve energy. Runtime
/// authorization must still use [`resolve_powered_craft`] / [`validate_start_powered_craft`].
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoweredCraftWorkProjection {
    required_energy: Energy,
    duration: TickSpan,
    condition_after: Condition,
}

impl PoweredCraftWorkProjection {
    #[must_use]
    pub const fn required_energy(self) -> Energy {
        self.required_energy
    }

    #[must_use]
    pub const fn duration(self) -> TickSpan {
        self.duration
    }

    #[must_use]
    pub const fn condition_after(self) -> Condition {
        self.condition_after
    }
}

struct ResolvedPoweredCraftProvider {
    equipment: ValidatedEquipmentUse,
    rate: MassFlow,
    condition: Condition,
}

struct PoweredCraftEnergyAssessment {
    access: EnergySupplyAccess,
    required_energy: Energy,
}

fn resolve_powered_craft_definitions(
    registries: &Registries,
    process: ProcessId,
) -> Result<(PoweredCraftDefinition, &ManualCraftDefinition), PoweredCraftError> {
    let definition = registries
        .crafting()
        .get_powered(process)
        .ok_or(PoweredCraftError::UnknownProcess { process })?;
    let transform = registries
        .crafting()
        .get_manual(definition.transform())
        .ok_or(PoweredCraftError::MissingTransform { process })?;
    Ok((definition, transform))
}

fn resolve_powered_craft_provider(
    registries: &Registries,
    state: &AppState,
    definition: PoweredCraftDefinition,
    equipment: EquipmentId,
) -> Result<ResolvedPoweredCraftProvider, PoweredCraftError> {
    let provider = resolve_equipment_provider(registries, state, equipment)
        .map_err(PoweredCraftError::Equipment)?;
    let rate = resolve_powered_craft_rate(
        registries,
        definition,
        provider.definition(),
        provider.condition(),
    )
    .map_err(PoweredCraftError::Capability)?;
    Ok(ResolvedPoweredCraftProvider {
        equipment: provider.validated_use(),
        rate,
        condition: provider.condition(),
    })
}

pub(super) fn resolve_powered_craft_rate(
    registries: &Registries,
    definition: PoweredCraftDefinition,
    equipment: &EquipmentDefinition,
    condition: Condition,
) -> Result<MassFlow, CapabilityEvaluationError> {
    let process_definition = registries
        .production()
        .get_process(definition.process())
        .unwrap_or_else(|| unreachable!("validated powered-craft process disappeared"));
    evaluate_equipment_capabilities_at_condition(
        registries.capabilities(),
        equipment,
        condition,
        process_definition.capability_requirements(),
    )?;
    let capability = definition.mass_flow_capability();
    let rate = match resolve_equipment_capability(equipment, condition, capability) {
        Some(CapabilityValue::MassFlow(rate)) => rate,
        Some(_) | None => unreachable!(
            "validated powered-craft provider lost its resolver-owned throughput capability"
        ),
    };
    Ok(rate)
}

fn assess_powered_craft_energy(
    registries: &Registries,
    state: &AppState,
    definition: PoweredCraftDefinition,
    input_mass: Mass,
    energy_store: EnergyStoreId,
) -> Result<PoweredCraftEnergyAssessment, PoweredCraftError> {
    let access = assess_energy_supply_access(registries, state, energy_store)
        .map_err(PoweredCraftError::Energy)?;
    if access.carrier() != definition.energy_carrier() {
        return Err(PoweredCraftError::WrongEnergyCarrier {
            required: definition.energy_carrier(),
            provided: access.carrier(),
        });
    }
    let required_energy = calculate_mass_specific_energy(input_mass, definition.specific_energy());
    Ok(PoweredCraftEnergyAssessment {
        access,
        required_energy,
    })
}

fn resolve_powered_craft_work(
    registries: &Registries,
    definition: PoweredCraftDefinition,
    input_mass: Mass,
    rate: MassFlow,
    condition_before: Condition,
    required_energy: Energy,
    available_power: Power,
) -> Result<PoweredCraftWorkProjection, PoweredCraftError> {
    let (duration, condition_after) = resolve_powered_craft_timing(
        registries,
        rate,
        input_mass,
        required_energy,
        available_power,
        definition.condition_wear_ppm_per_active_tick(),
        condition_before,
    )
    .map_err(|error| match error {
        PoweredCraftTimingError::ThroughputDuration(error) => {
            PoweredCraftError::ThroughputDuration(error)
        }
        PoweredCraftTimingError::EnergyDuration(error) => PoweredCraftError::EnergyDuration(error),
        PoweredCraftTimingError::EquipmentCondition(error) => {
            PoweredCraftError::EquipmentCondition(error)
        }
    })?;
    Ok(PoweredCraftWorkProjection {
        required_energy,
        duration,
        condition_after,
    })
}

/// Projects powered-craft work from current observable machine/store state without requiring the
/// store to already contain the projected work.
pub fn project_powered_craft_work(
    registries: &Registries,
    state: &AppState,
    process: ProcessId,
    input_mass: Mass,
    equipment: EquipmentId,
    energy_store: EnergyStoreId,
) -> Result<PoweredCraftWorkProjection, PoweredCraftError> {
    if input_mass.is_zero() {
        return Err(PoweredCraftError::EmptyInput);
    }
    let (definition, transform) = resolve_powered_craft_definitions(registries, process)?;
    if !input_mass
        .milligrams()
        .is_multiple_of(transform.input_mass().milligrams())
    {
        return Err(PoweredCraftError::InputMassNotWholeBatches {
            consumed: input_mass,
            batch_mass: transform.input_mass(),
        });
    }
    let provider = resolve_powered_craft_provider(registries, state, definition, equipment)?;
    let energy =
        assess_powered_craft_energy(registries, state, definition, input_mass, energy_store)?;
    if energy.required_energy > energy.access.capacity() {
        return Err(PoweredCraftError::EnergyCapacityExceeded {
            store: energy_store,
            capacity: energy.access.capacity(),
            requested: energy.required_energy,
        });
    }
    resolve_powered_craft_work(
        registries,
        definition,
        input_mass,
        provider.rate,
        provider.condition,
        energy.required_energy,
        energy.access.max_output_power(),
    )
}

/// Resolves one powered crafting operation without claiming player attention.
pub fn resolve_powered_craft(
    registries: &Registries,
    state: &AppState,
    request: &PoweredCraftRequest,
) -> Result<ProcessResolution, PoweredCraftError> {
    let (definition, transform) = resolve_powered_craft_definitions(registries, request.process)?;
    let inputs =
        validate_process_inputs(state, request.process, request.source, &request.selections)
            .map_err(PoweredCraftError::Input)?;
    let batch =
        validate_manual_craft_batch(transform, inputs.input_mass(), inputs.consumed_inputs())
            .map_err(batch_error)?;
    let outputs = build_manual_craft_outputs(transform, batch.batches(), batch.temperature())
        .map_err(|error| match error {
            super::batch::ManualCraftOutputError::MassOverflow { .. } => {
                PoweredCraftError::OutputMassOverflow
            }
            super::batch::ManualCraftOutputError::Construction { error, .. } => {
                PoweredCraftError::Output(error)
            }
        })?;

    let provider =
        resolve_powered_craft_provider(registries, state, definition, request.equipment)?;
    let energy = assess_powered_craft_energy(
        registries,
        state,
        definition,
        inputs.input_mass(),
        request.energy_store,
    )?;
    let energy_supply = validate_energy_supply_request(energy.access, energy.required_energy)
        .map_err(PoweredCraftError::Energy)?;
    let work = resolve_powered_craft_work(
        registries,
        definition,
        inputs.input_mass(),
        provider.rate,
        provider.condition,
        energy.required_energy,
        energy_supply.max_output_power(),
    )?;
    inputs
        .resolve_with_energy_and_equipment(
            work.duration,
            vec![crate::production::ProcessOutputStream::new(
                crate::production::ProcessOutputStreamId::PRIMARY,
                outputs,
            )],
            energy_supply,
            provider.equipment,
            work.condition_after,
        )
        .map_err(PoweredCraftError::Resolution)
}

/// Resolves and admits one unattended powered craft through canonical production ownership.
pub fn validate_start_powered_craft(
    registries: &Registries,
    state: &AppState,
    request: PoweredCraftRequest,
    destination: StockpileId,
) -> Result<ValidatedStartProcess, StartPoweredCraftError> {
    let source = request.source;
    let resolution = resolve_powered_craft(registries, state, &request)
        .map_err(StartPoweredCraftError::Resolution)?;
    validate_start_process(registries, state, &resolution, source, destination)
        .map_err(StartPoweredCraftError::Process)
}
