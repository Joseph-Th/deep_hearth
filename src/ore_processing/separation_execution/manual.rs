//! Direct player-labor constituent separation using the same conservative material projection as machinery.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Mass, MassFlow};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::inventory::{MaterialLotSelection, StockpileId};
use crate::labor::{
    PlayerWork, PlayerWorkCommitError, PlayerWorkStartError, ValidatedPlayerWorkStart,
    validate_player_work_start,
};
use crate::production::{
    ProcessId, ProcessInputError, ProcessOutputRoute, ProcessOutputStream, ProcessResolution,
    ProcessResolutionError, ProductionJobId, StartProcessCommitError, StartProcessError,
    ValidatedStartProcess, validate_selected_process_inputs, validate_start_manual_process_routed,
};
use crate::registry::Registries;

use super::{ConstituentSeparationBatchError, resolve_separation_outputs};
use crate::ore_processing::{
    ManualConstituentSeparationProcessDefinition, MassFlowDurationError,
    calculate_mass_flow_duration_ceiling,
};

/// Explicit selected-batch request for direct hand sorting.
#[derive(Clone, Copy, Debug)]
pub struct ManualConstituentSeparationRequest<'selection> {
    process: ProcessId,
    source: StockpileId,
    selections: &'selection [MaterialLotSelection],
}

impl<'selection> ManualConstituentSeparationRequest<'selection> {
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
        }
    }
}

/// Failure while resolving a manual separation batch before any player attention is reserved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManualConstituentSeparationResolutionError {
    UnknownProcess { process: ProcessId },
    Input(ProcessInputError),
    BatchMassExceeded { selected: Mass, maximum: Mass },
    Batch(ConstituentSeparationBatchError),
    ThroughputDuration(MassFlowDurationError),
    Resolution(ProcessResolutionError),
}

impl Display for ManualConstituentSeparationResolutionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownProcess { process } => write!(
                formatter,
                "process {} has no authored manual constituent-separation semantics",
                process.value()
            ),
            Self::Input(error) => write!(formatter, "manual separation input failed: {error}"),
            Self::BatchMassExceeded { selected, maximum } => write!(
                formatter,
                "selected manual separation batch {} mg exceeds hand-sorting maximum {} mg",
                selected.milligrams(),
                maximum.milligrams()
            ),
            Self::Batch(error) => write!(formatter, "manual separation batch failed: {error}"),
            Self::ThroughputDuration(error) => {
                write!(formatter, "manual separation duration failed: {error}")
            }
            Self::Resolution(error) => {
                write!(
                    formatter,
                    "manual separation process resolution failed: {error}"
                )
            }
        }
    }
}

impl Error for ManualConstituentSeparationResolutionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Input(error) => Some(error),
            Self::Batch(error) => Some(error),
            Self::ThroughputDuration(error) => Some(error),
            Self::Resolution(error) => Some(error),
            Self::UnknownProcess { .. } | Self::BatchMassExceeded { .. } => None,
        }
    }
}

/// Fully resolved manual sorting batch. No equipment or energy resources are present.
#[must_use]
#[derive(Debug)]
pub struct ResolvedManualConstituentSeparation {
    resolution: ProcessResolution,
    processing_rate: MassFlow,
    target_mass: Mass,
    residue_mass: Mass,
}

impl ResolvedManualConstituentSeparation {
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

    #[must_use]
    pub const fn target_mass(&self) -> Mass {
        self.target_mass
    }

    #[must_use]
    pub const fn residue_mass(&self) -> Mass {
        self.residue_mass
    }
}

/// Resolves a deterministic hand-sorting pass from exact selected composition.
pub fn resolve_manual_constituent_separation_process(
    registries: &Registries,
    state: &AppState,
    request: ManualConstituentSeparationRequest<'_>,
) -> Result<ResolvedManualConstituentSeparation, ManualConstituentSeparationResolutionError> {
    let ManualConstituentSeparationRequest {
        process,
        source,
        selections,
    } = request;
    let definition = registries
        .ore_processing()
        .get_manual_constituent_separation(process)
        .ok_or(ManualConstituentSeparationResolutionError::UnknownProcess { process })?;
    let inputs = validate_selected_process_inputs(registries, state, process, source, selections)
        .map_err(ManualConstituentSeparationResolutionError::Input)?;
    let selected_mass = inputs.input_mass();
    if selected_mass > definition.max_batch_mass() {
        return Err(
            ManualConstituentSeparationResolutionError::BatchMassExceeded {
                selected: selected_mass,
                maximum: definition.max_batch_mass(),
            },
        );
    }
    let target_particle_size_policy = registries
        .materials()
        .get_form(definition.target_output_form())
        .unwrap_or_else(|| {
            unreachable!("registered manual separation target output form must remain available")
        })
        .particle_size_policy();
    let outputs = resolve_separation_outputs(
        registries.materials(),
        definition.physics(),
        target_particle_size_policy,
        inputs.consumed_inputs(),
    )
    .map_err(ManualConstituentSeparationResolutionError::Batch)?;
    let duration = calculate_mass_flow_duration_ceiling(
        definition.processing_rate(),
        selected_mass,
        registries.core().physical_tick_duration(),
    )
    .map_err(ManualConstituentSeparationResolutionError::ThroughputDuration)?;
    let resolution = inputs
        .resolve_without_resources_routed(
            duration,
            vec![
                ProcessOutputStream::new(
                    ManualConstituentSeparationProcessDefinition::TARGET_STREAM,
                    outputs.target,
                ),
                ProcessOutputStream::new(
                    ManualConstituentSeparationProcessDefinition::RESIDUE_STREAM,
                    outputs.residue,
                ),
            ],
        )
        .map_err(ManualConstituentSeparationResolutionError::Resolution)?;
    Ok(ResolvedManualConstituentSeparation {
        resolution,
        processing_rate: definition.processing_rate(),
        target_mass: outputs.target_mass,
        residue_mass: outputs.residue_mass,
    })
}

/// Failure while reserving a resolved manual separation job and exclusive player labor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartManualConstituentSeparationError {
    Process(StartProcessError),
    Work(PlayerWorkStartError),
}

impl Display for StartManualConstituentSeparationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Process(error) => write!(formatter, "manual separation start failed: {error}"),
            Self::Work(error) => {
                write!(formatter, "manual separation labor is unavailable: {error}")
            }
        }
    }
}

impl Error for StartManualConstituentSeparationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Process(error) => Some(error),
            Self::Work(error) => Some(error),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManualConstituentSeparationCommitError {
    Process(StartProcessCommitError),
    Work(PlayerWorkCommitError),
}

impl Display for ManualConstituentSeparationCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Process(error) => write!(formatter, "manual separation commit failed: {error}"),
            Self::Work(error) => {
                write!(formatter, "manual separation labor commit failed: {error}")
            }
        }
    }
}

impl Error for ManualConstituentSeparationCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Process(error) => Some(error),
            Self::Work(error) => Some(error),
        }
    }
}

/// Consumed proof that routed output capacity and player labor were available together.
#[must_use]
pub struct ValidatedManualConstituentSeparationStart {
    process: ValidatedStartProcess,
    work: ValidatedPlayerWorkStart,
}

impl ValidatedManualConstituentSeparationStart {
    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<ProductionJobId, ManualConstituentSeparationCommitError> {
        self.work
            .precheck(state)
            .map_err(ManualConstituentSeparationCommitError::Work)?;
        let job = self
            .process
            .commit(state)
            .map_err(ManualConstituentSeparationCommitError::Process)?;
        self.work.apply(state);
        Ok(job)
    }
}

/// Admits one already-resolved hand-sorting operation with explicit target and residue routes.
pub fn validate_start_manual_constituent_separation(
    registries: &Registries,
    state: &AppState,
    resolved: &ResolvedManualConstituentSeparation,
    source: StockpileId,
    target_destination: StockpileId,
    residue_destination: StockpileId,
) -> Result<ValidatedManualConstituentSeparationStart, StartManualConstituentSeparationError> {
    let process_id = resolved.process_resolution().process();
    let definition = registries
        .ore_processing()
        .get_manual_constituent_separation(process_id)
        .unwrap_or_else(|| {
            panic!("runtime invariant broken: resolved manual separation definition disappeared")
        });
    let routes = [
        ProcessOutputRoute::new(
            ManualConstituentSeparationProcessDefinition::TARGET_STREAM,
            target_destination,
        ),
        ProcessOutputRoute::new(
            ManualConstituentSeparationProcessDefinition::RESIDUE_STREAM,
            residue_destination,
        ),
    ];
    let process = validate_start_manual_process_routed(
        registries,
        state,
        resolved.process_resolution(),
        source,
        &routes,
    )
    .map_err(StartManualConstituentSeparationError::Process)?;
    let work = validate_player_work_start(
        registries,
        state,
        PlayerWork::ManualProduction {
            job: process.job_id(),
        },
        resolved.duration(),
        definition.exertion(),
    )
    .map_err(StartManualConstituentSeparationError::Work)?;
    Ok(ValidatedManualConstituentSeparationStart { process, work })
}
