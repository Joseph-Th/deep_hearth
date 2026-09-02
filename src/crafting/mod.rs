//! Manual shaping operations that reuse canonical timed production ownership.

use crate::capability::CapabilityValue;
use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::equipment::{EquipmentId, resolve_equipment_provider};
use crate::inventory::{MaterialLotSelection, StockpileId};
use crate::labor::{PlayerWork, ValidatedPlayerWorkStart, validate_player_work_start};
use crate::maintenance::calculate_usable_condition_after_active_ticks;
use crate::material::{MaterialComposition, MaterialLotSpec};
use crate::ore_processing::calculate_mass_flow_duration_ceiling;
use crate::production::{
    ProcessId, ProcessResolution, ProductionJobId, ValidatedStartProcess,
    validate_selected_process_inputs, validate_start_manual_process,
};
use crate::registry::Registries;
use crate::survival::{Vitality, assess_survival};

mod batch;
mod definitions;
mod errors;
mod registry;
mod validation;

pub use definitions::{ManualCraftDefinition, ManualCraftEquipmentProfile, ManualCraftOutput};
pub use errors::{ManualCraftCommitError, ManualCraftError, StartManualCraftError};
pub use registry::CraftingRegistry;
pub use validation::ManualCraftJobValidationError;
pub(crate) use validation::validate_loaded_manual_craft_job;

/// Exact hand-work request bound to explicit material-lot slices.
///
/// The selected mass determines the integral authored batch count. Callers therefore cannot ask
/// the generic inventory allocator to choose temperature- or provenance-sensitive matter on their
/// behalf, and cannot provide a batch count that disagrees with the conserved input selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManualCraftRequest {
    process: ProcessId,
    source: StockpileId,
    selections: Vec<MaterialLotSelection>,
    equipment: Option<EquipmentId>,
}

impl ManualCraftRequest {
    #[must_use]
    pub fn new(
        process: ProcessId,
        source: StockpileId,
        selections: Vec<MaterialLotSelection>,
    ) -> Self {
        Self {
            process,
            source,
            selections,
            equipment: None,
        }
    }

    #[must_use]
    pub fn with_equipment(mut self, equipment: EquipmentId) -> Self {
        self.equipment = Some(equipment);
        self
    }

    #[must_use]
    pub fn single(
        process: ProcessId,
        source: StockpileId,
        selection: MaterialLotSelection,
    ) -> Self {
        Self::new(process, source, vec![selection])
    }

    #[must_use]
    pub const fn process(&self) -> ProcessId {
        self.process
    }

    #[must_use]
    pub const fn source(&self) -> StockpileId {
        self.source
    }

    #[must_use]
    pub fn selections(&self) -> &[MaterialLotSelection] {
        &self.selections
    }

    #[must_use]
    pub const fn equipment(&self) -> Option<EquipmentId> {
        self.equipment
    }
}

/// Manual-work admission request including the destination for conserved outputs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManualCraftStartRequest {
    craft: ManualCraftRequest,
    destination: StockpileId,
}

impl ManualCraftStartRequest {
    #[must_use]
    pub const fn new(craft: ManualCraftRequest, destination: StockpileId) -> Self {
        Self { craft, destination }
    }

    #[must_use]
    pub fn single(
        process: ProcessId,
        source: StockpileId,
        selection: MaterialLotSelection,
        destination: StockpileId,
    ) -> Self {
        Self::new(
            ManualCraftRequest::single(process, source, selection),
            destination,
        )
    }
}

/// Resolves an explicitly selected hand-work batch into canonical durable production.
pub fn resolve_manual_craft(
    registries: &Registries,
    state: &AppState,
    request: &ManualCraftRequest,
) -> Result<ProcessResolution, ManualCraftError> {
    let process = request.process();
    let source = request.source();
    let survival =
        assess_survival(registries, state).ok_or(ManualCraftError::SurvivalNotInitialized)?;
    if survival.vitality() == Vitality::ZERO {
        return Err(ManualCraftError::PlayerDead);
    }
    let definition = registries
        .crafting()
        .get_manual(process)
        .ok_or(ManualCraftError::UnknownManualProcess { process })?;
    let inputs =
        validate_selected_process_inputs(registries, state, process, source, request.selections())
            .map_err(ManualCraftError::Input)?;
    let batch = batch::validate_manual_craft_batch(
        definition,
        inputs.input_mass(),
        inputs.consumed_inputs(),
    )
    .map_err(|error| ManualCraftError::from_batch_error(error, definition))?;
    let batches = batch.batches();
    let temperature = batch.temperature();
    let outputs = definition
        .outputs()
        .iter()
        .map(|output| {
            let mass = output
                .mass()
                .milligrams()
                .checked_mul(batches.get())
                .map(Mass::from_milligrams)
                .ok_or(ManualCraftError::OutputMassOverflow {
                    commodity: output.commodity(),
                    batches,
                })?;
            MaterialLotSpec::with_composition(
                output.commodity(),
                mass,
                temperature,
                MaterialComposition::pure(output.commodity().material()),
            )
            .map_err(ManualCraftError::Output)
        })
        .collect::<Result<Vec<_>, _>>()?;
    match request.equipment() {
        None => {
            if definition
                .equipment_profile()
                .is_some_and(ManualCraftEquipmentProfile::requires_equipment)
            {
                return Err(ManualCraftError::RequiredEquipmentMissing { process });
            }
            let duration = definition
                .duration()
                .value()
                .checked_mul(batches.get())
                .map(TickSpan::new)
                .ok_or(ManualCraftError::DurationOverflow { batches })?;
            inputs
                .resolve_without_resources(duration, outputs)
                .map_err(ManualCraftError::Resolution)
        }
        Some(equipment) => {
            let profile = definition
                .equipment_profile()
                .ok_or(ManualCraftError::EquipmentNotSupported { process, equipment })?;
            let provider = resolve_equipment_provider(registries, state, equipment)
                .map_err(ManualCraftError::Equipment)?;
            let capability = profile.mass_flow_capability();
            let rate = match provider.get_capability(capability) {
                Some(CapabilityValue::MassFlow(rate)) => rate,
                Some(value) => {
                    return Err(ManualCraftError::EquipmentCapabilityKindMismatch {
                        equipment,
                        capability,
                        found: value.kind(),
                    });
                }
                None => {
                    return Err(ManualCraftError::MissingEquipmentCapability {
                        equipment,
                        capability,
                    });
                }
            };
            let duration = calculate_mass_flow_duration_ceiling(
                rate,
                inputs.input_mass(),
                registries.core().physical_tick_duration(),
            )
            .map_err(ManualCraftError::EquipmentDuration)?;
            let condition_after = calculate_usable_condition_after_active_ticks(
                profile.condition_wear_ppm_per_active_tick(),
                provider.condition(),
                duration,
            )
            .map_err(ManualCraftError::EquipmentCondition)?;
            inputs
                .resolve_with_equipment(
                    duration,
                    outputs,
                    provider.validated_use(),
                    condition_after,
                )
                .map_err(ManualCraftError::Resolution)
        }
    }
}

/// Consumed proof that both the process and the player's labor were available at validation time.
#[must_use]
pub struct ValidatedManualCraftStart {
    process: ValidatedStartProcess,
    work: ValidatedPlayerWorkStart,
}

impl ValidatedManualCraftStart {
    pub fn commit(self, state: &mut AppState) -> Result<ProductionJobId, ManualCraftCommitError> {
        self.work
            .precheck(state)
            .map_err(ManualCraftCommitError::Work)?;
        let job = self
            .process
            .commit(state)
            .map_err(ManualCraftCommitError::Process)?;
        self.work.apply(state);
        Ok(job)
    }
}

/// Resolves and admits one manual craft while reserving the player's exclusive work time.
pub fn validate_start_manual_craft(
    registries: &Registries,
    state: &AppState,
    request: ManualCraftStartRequest,
) -> Result<ValidatedManualCraftStart, StartManualCraftError> {
    let ManualCraftStartRequest { craft, destination } = request;
    let process_id = craft.process();
    let source = craft.source();
    let resolution = resolve_manual_craft(registries, state, &craft)
        .map_err(StartManualCraftError::Resolution)?;
    let process =
        validate_start_manual_process(registries, state, &resolution, source, destination)
            .map_err(StartManualCraftError::Process)?;
    let exertion = registries
        .crafting()
        .get_manual(process_id)
        .unwrap_or_else(|| {
            panic!("runtime invariant broken: resolved manual craft definition disappeared")
        })
        .exertion();
    let work = validate_player_work_start(
        registries,
        state,
        PlayerWork::ManualProduction {
            job: process.job_id(),
        },
        resolution.duration(),
        exertion,
    )
    .map_err(StartManualCraftError::Work)?;
    Ok(ValidatedManualCraftStart { process, work })
}

#[cfg(test)]
#[path = "index_tests.rs"]
mod index_tests;

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
