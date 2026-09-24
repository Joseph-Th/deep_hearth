//! Direct player-labor comminution using the same material and particle projection as powered crushing.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::MassFlow;
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::equipment::EquipmentId;
use crate::inventory::{MaterialLotSelection, StockpileId};
use crate::labor::{
    PlayerWork, PlayerWorkCommitError, PlayerWorkStartError, ValidatedPlayerWorkStart,
    validate_player_work_start,
};
use crate::production::{
    ProcessId, ProcessInputError, ProcessOutputStream, ProcessOutputStreamId, ProcessResolution,
    ProcessResolutionError, ProductionJobId, StartProcessCommitError, StartProcessError,
    ValidatedStartProcess, validate_process_inputs, validate_start_manual_process,
};
use crate::registry::Registries;

use super::ComminutionBatchError;
use super::outputs::resolve_manual_comminution_outputs;
use crate::ore_processing::manual_physics::{
    project_manual_ore_duration, resolve_manual_ore_equipment, validate_manual_ore_batch,
};
use crate::ore_processing::{ManualOreEquipmentError, ManualOrePhysicsError};

/// Explicit selected-batch request for direct hand breaking of coarse material.
#[derive(Clone, Copy, Debug)]
pub struct ManualComminutionRequest<'selection> {
    process: ProcessId,
    source: StockpileId,
    selections: &'selection [MaterialLotSelection],
    equipment: Option<EquipmentId>,
}

impl<'selection> ManualComminutionRequest<'selection> {
    #[must_use]
    pub const fn new(
        process: ProcessId,
        source: StockpileId,
        selections: &'selection [MaterialLotSelection],
    ) -> Self {
        Self {
            process,
            source,
            selections,
            equipment: None,
        }
    }

    /// Uses a durable hand tool or workstation while preserving direct player attention.
    #[must_use]
    pub const fn with_equipment(mut self, equipment: EquipmentId) -> Self {
        self.equipment = Some(equipment);
        self
    }
}

/// Failure while resolving one direct-labor comminution batch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManualComminutionResolutionError {
    UnknownProcess { process: ProcessId },
    Input(ProcessInputError),
    Equipment(ManualOreEquipmentError),
    Physics(ManualOrePhysicsError),
    Batch(ComminutionBatchError),
    Resolution(ProcessResolutionError),
}

impl Display for ManualComminutionResolutionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownProcess { process } => write!(
                formatter,
                "process {} has no authored manual comminution semantics",
                process.value()
            ),
            Self::Input(error) => write!(formatter, "manual comminution input failed: {error}"),
            Self::Equipment(error) => {
                write!(formatter, "manual comminution equipment failed: {error}")
            }
            Self::Physics(error) => write!(formatter, "manual comminution physics failed: {error}"),
            Self::Batch(error) => write!(formatter, "manual comminution batch failed: {error}"),
            Self::Resolution(error) => {
                write!(formatter, "manual comminution resolution failed: {error}")
            }
        }
    }
}

impl Error for ManualComminutionResolutionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Input(error) => Some(error),
            Self::Equipment(error) => Some(error),
            Self::Physics(error) => Some(error),
            Self::Batch(error) => Some(error),
            Self::Resolution(error) => Some(error),
            Self::UnknownProcess { .. } => None,
        }
    }
}

/// Fully resolved direct-labor comminution batch with no equipment or stored-energy resource.
#[must_use]
#[derive(Debug)]
pub struct ResolvedManualComminution {
    resolution: ProcessResolution,
    processing_rate: MassFlow,
}

impl ResolvedManualComminution {
    pub const fn process_resolution(&self) -> &ProcessResolution {
        &self.resolution
    }

    #[must_use]
    pub const fn processing_rate(&self) -> MassFlow {
        self.processing_rate
    }

    #[must_use]
    pub const fn duration(&self) -> TickSpan {
        self.resolution.duration()
    }
}

/// Resolves hand breaking from exact selected matter while preserving composition and temperature.
pub fn resolve_manual_comminution_process(
    registries: &Registries,
    state: &AppState,
    request: ManualComminutionRequest<'_>,
) -> Result<ResolvedManualComminution, ManualComminutionResolutionError> {
    let ManualComminutionRequest {
        process,
        source,
        selections,
        equipment,
    } = request;
    let definition = registries
        .ore_processing()
        .get_manual_comminution(process)
        .ok_or(ManualComminutionResolutionError::UnknownProcess { process })?;
    let inputs = validate_process_inputs(state, process, source, selections)
        .map_err(ManualComminutionResolutionError::Input)?;
    let selected_mass = inputs.input_mass();
    let outputs = resolve_manual_comminution_outputs(definition, inputs.consumed_inputs())
        .map_err(ManualComminutionResolutionError::Batch)?;
    let output_streams = vec![ProcessOutputStream::new(
        ProcessOutputStreamId::PRIMARY,
        outputs,
    )];
    let (resolution, processing_rate) = match equipment {
        None => {
            let duration = project_manual_ore_duration(
                registries.core().physical_tick_duration(),
                definition.operating_profile(),
                selected_mass,
            )
            .map_err(ManualComminutionResolutionError::Physics)?;
            let resolution = inputs
                .resolve_without_resources_routed(duration, output_streams)
                .map_err(ManualComminutionResolutionError::Resolution)?;
            (resolution, definition.processing_rate())
        }
        Some(equipment) => {
            validate_manual_ore_batch(definition.operating_profile(), selected_mass)
                .map_err(ManualComminutionResolutionError::Physics)?;
            let assisted = resolve_manual_ore_equipment(
                registries,
                state,
                process,
                definition.operating_profile(),
                selected_mass,
                equipment,
            )
            .map_err(ManualComminutionResolutionError::Equipment)?;
            let duration = assisted.duration();
            let resolution = inputs
                .resolve_with_equipment_routed(
                    duration,
                    output_streams,
                    assisted.equipment_use(),
                    assisted.condition_after(),
                )
                .map_err(ManualComminutionResolutionError::Resolution)?;
            (resolution, assisted.processing_rate())
        }
    };
    Ok(ResolvedManualComminution {
        resolution,
        processing_rate,
    })
}

/// Failure while reserving a resolved hand-breaking job and exclusive player labor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartManualComminutionError {
    Process(StartProcessError),
    Work(PlayerWorkStartError),
}

impl Display for StartManualComminutionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Process(error) => write!(formatter, "manual comminution start failed: {error}"),
            Self::Work(error) => write!(
                formatter,
                "manual comminution labor is unavailable: {error}"
            ),
        }
    }
}

impl Error for StartManualComminutionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Process(error) => Some(error),
            Self::Work(error) => Some(error),
        }
    }
}

/// Failure while committing a validated hand-breaking start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManualComminutionCommitError {
    Process(StartProcessCommitError),
    Work(PlayerWorkCommitError),
}

impl Display for ManualComminutionCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Process(error) => write!(formatter, "manual comminution commit failed: {error}"),
            Self::Work(error) => {
                write!(formatter, "manual comminution labor commit failed: {error}")
            }
        }
    }
}

impl Error for ManualComminutionCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Process(error) => Some(error),
            Self::Work(error) => Some(error),
        }
    }
}

/// Consumed proof that output capacity and player labor were available together.
#[must_use]
pub struct ValidatedManualComminutionStart {
    process: ValidatedStartProcess,
    work: ValidatedPlayerWorkStart,
}

impl ValidatedManualComminutionStart {
    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<ProductionJobId, ManualComminutionCommitError> {
        self.work
            .precheck(state)
            .map_err(ManualComminutionCommitError::Work)?;
        let job = self
            .process
            .commit(state)
            .map_err(ManualComminutionCommitError::Process)?;
        self.work.apply(state);
        Ok(job)
    }
}

/// Admits one resolved direct-labor comminution operation into production and player work.
pub fn validate_start_manual_comminution(
    registries: &Registries,
    state: &AppState,
    resolved: &ResolvedManualComminution,
    source: StockpileId,
    destination: StockpileId,
) -> Result<ValidatedManualComminutionStart, StartManualComminutionError> {
    let process_id = resolved.process_resolution().process();
    let definition = registries
        .ore_processing()
        .get_manual_comminution(process_id)
        .ok_or(StartManualComminutionError::Process(
            StartProcessError::UnknownProcess {
                process: process_id,
            },
        ))?;
    let process = validate_start_manual_process(
        registries,
        state,
        resolved.process_resolution(),
        source,
        destination,
    )
    .map_err(StartManualComminutionError::Process)?;
    let work = validate_player_work_start(
        registries,
        state,
        PlayerWork::ManualProduction {
            job: process.job_id(),
        },
        resolved.duration(),
        definition.exertion(),
    )
    .map_err(StartManualComminutionError::Work)?;
    Ok(ValidatedManualComminutionStart { process, work })
}
