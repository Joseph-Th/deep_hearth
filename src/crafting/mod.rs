//! Manual shaping operations that reuse canonical timed production ownership.

use crate::core::state::AppState;
use crate::equipment::{EquipmentId, resolve_equipment_provider};
use crate::inventory::{MaterialLotSelection, StockpileId};
use crate::labor::{PlayerWork, ValidatedPlayerWorkStart, validate_player_work_start};
use crate::logistics::validate_player_stockpile_access;
use crate::production::{
    ProcessId, ProcessResolution, ProductionJobId, ValidatedStartProcess, validate_process_inputs,
    validate_start_manual_process,
};
use crate::registry::Registries;
use crate::survival::{Vitality, assess_survival};

mod batch;
mod definitions;
mod errors;
mod input_planning;
mod physics;
mod powered;
mod projection;
mod registry;
mod validation;

use physics::{
    ManualCraftEquipmentResolutionError, ManualCraftEquipmentScheduleError,
    resolve_manual_craft_hand_duration,
};

pub use definitions::{
    ManualCraftDefinition, ManualCraftEquipmentProfile, ManualCraftOutput, PoweredCraftDefinition,
};
pub use errors::{
    ManualCraftCommitError, ManualCraftEquipmentProjectionError, ManualCraftError,
    ManualCraftHandProjectionError, StartManualCraftError,
};
pub use input_planning::{
    ManualCraftInputAvailability, ManualCraftInputMode, ManualCraftInputPlanError,
    ManualCraftStockpileOption, assess_manual_craft_inputs, manual_craft_options_from_stockpile,
    plan_manual_craft_from_stockpile,
};
pub(crate) use physics::resolve_manual_craft_equipment_physics;
pub use powered::{
    PoweredCraftError, PoweredCraftRequest, PoweredCraftWorkProjection, StartPoweredCraftError,
    project_powered_craft_work, resolve_powered_craft, validate_start_powered_craft,
};
pub use projection::{
    ManualCraftEquipmentProjection, ManualCraftHandProjection, project_manual_craft_equipment,
    project_manual_craft_hand_work,
};
pub use registry::CraftingRegistry;
pub use validation::CraftingJobValidationError;
pub(crate) use validation::validate_loaded_crafting_job;

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

    /// Sends conserved craft output back to the same inventory custody that supplied the input.
    ///
    /// This is the familiar personal-inventory/container crafting path. Canonical production
    /// admission still projects outgoing input before reserving incoming output, so identical
    /// source/destination does not bypass finite capacity.
    #[must_use]
    pub fn in_place(craft: ManualCraftRequest) -> Self {
        let destination = craft.source();
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

    #[must_use]
    pub const fn destination(&self) -> StockpileId {
        self.destination
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
    let inputs = validate_process_inputs(state, process, source, request.selections())
        .map_err(ManualCraftError::Input)?;
    let batch = batch::validate_manual_craft_batch(
        definition,
        inputs.input_mass(),
        inputs.consumed_inputs(),
    )
    .map_err(|error| ManualCraftError::from_batch_error(error, definition))?;
    let batches = batch.batches();
    let temperature = batch.temperature();
    let outputs =
        batch::build_manual_craft_outputs(definition, batches, temperature).map_err(|error| {
            match error {
                batch::ManualCraftOutputError::MassOverflow { commodity, batches } => {
                    ManualCraftError::OutputMassOverflow { commodity, batches }
                }
                batch::ManualCraftOutputError::Construction { error, .. } => {
                    ManualCraftError::Output(error)
                }
            }
        })?;
    match request.equipment() {
        None => {
            if definition
                .equipment_profile()
                .is_some_and(ManualCraftEquipmentProfile::requires_equipment)
            {
                return Err(ManualCraftError::RequiredEquipmentMissing { process });
            }
            let duration = resolve_manual_craft_hand_duration(definition.duration(), batches)
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
            let schedule = resolve_manual_craft_equipment_physics(
                provider.definition(),
                provider.condition(),
                profile.mass_flow_capability(),
                inputs.input_mass(),
                registries.core().physical_tick_duration(),
                profile.condition_wear_ppm_per_active_tick(),
            )
            .map_err(|error| match error {
                ManualCraftEquipmentResolutionError::MissingCapability { capability } => {
                    ManualCraftError::MissingEquipmentCapability {
                        equipment,
                        capability,
                    }
                }
                ManualCraftEquipmentResolutionError::CapabilityKindMismatch {
                    capability,
                    found,
                } => ManualCraftError::EquipmentCapabilityKindMismatch {
                    equipment,
                    capability,
                    found,
                },
                ManualCraftEquipmentResolutionError::Schedule(error) => match error {
                    ManualCraftEquipmentScheduleError::Duration(error) => {
                        ManualCraftError::EquipmentDuration(error)
                    }
                    ManualCraftEquipmentScheduleError::Condition(error) => {
                        ManualCraftError::EquipmentCondition(error)
                    }
                },
            })?;
            inputs
                .resolve_with_equipment(
                    schedule.duration(),
                    outputs,
                    provider.validated_use(),
                    schedule.condition_after(),
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
    expected_logistics_revision: u64,
}

impl ValidatedManualCraftStart {
    pub fn commit(self, state: &mut AppState) -> Result<ProductionJobId, ManualCraftCommitError> {
        let actual_logistics_revision = state.logistics().revision();
        if actual_logistics_revision != self.expected_logistics_revision {
            return Err(ManualCraftCommitError::StaleLogisticsRevision {
                expected: self.expected_logistics_revision,
                actual: actual_logistics_revision,
            });
        }
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
    validate_player_stockpile_access(state, source).map_err(StartManualCraftError::Access)?;
    validate_player_stockpile_access(state, destination).map_err(StartManualCraftError::Access)?;
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
    Ok(ValidatedManualCraftStart {
        process,
        work,
        expected_logistics_revision: state.logistics().revision(),
    })
}

#[cfg(test)]
#[path = "index_tests.rs"]
mod index_tests;

#[cfg(test)]
#[path = "input_planning_tests.rs"]
mod input_planning_tests;

#[cfg(test)]
#[path = "wood_recovery_tests.rs"]
mod wood_recovery_tests;

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
