//! Manual shaping operations that reuse canonical timed production ownership.

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::inventory::StockpileId;
use crate::labor::{PlayerWork, ValidatedPlayerWorkStart, validate_player_work_start};
use crate::material::{
    CommodityKey, MaterialComposition, MaterialLotSpec, MaterialRegistry, ParticleSizeStatePolicy,
};
use crate::production::{
    ProcessId, ProcessResolution, ProductionJobId, ProductionRegistry, ValidatedStartProcess,
    validate_repeated_process_inputs, validate_start_manual_process,
};
use crate::registry::Registries;
use crate::survival::{SurvivalExertion, Vitality, assess_survival};

mod errors;
mod validation;

pub use errors::{ManualCraftCommitError, ManualCraftError, StartManualCraftError};
pub use validation::ManualCraftJobValidationError;
pub(crate) use validation::validate_loaded_manual_craft_job;

/// One conserved output of a manual shaping operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ManualCraftOutput {
    commodity: CommodityKey,
    mass: Mass,
}

/// Exact hand-work request. Repetition reduces command repetition but does not reduce authored
/// material, active time, or per-tick exertion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManualCraftRequest {
    process: ProcessId,
    source: StockpileId,
    batches: NonZeroU64,
}

impl ManualCraftRequest {
    #[must_use]
    pub const fn new(process: ProcessId, source: StockpileId, batches: NonZeroU64) -> Self {
        Self {
            process,
            source,
            batches,
        }
    }

    #[must_use]
    pub const fn single(process: ProcessId, source: StockpileId) -> Self {
        Self::new(process, source, NonZeroU64::MIN)
    }

    #[must_use]
    pub const fn process(self) -> ProcessId {
        self.process
    }

    #[must_use]
    pub const fn source(self) -> StockpileId {
        self.source
    }

    #[must_use]
    pub const fn batches(self) -> NonZeroU64 {
        self.batches
    }
}

/// Manual-work admission request including the destination for conserved outputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    pub const fn single(process: ProcessId, source: StockpileId, destination: StockpileId) -> Self {
        Self::new(ManualCraftRequest::single(process, source), destination)
    }
}

impl ManualCraftOutput {
    #[must_use]
    pub fn new(commodity: CommodityKey, mass: Mass) -> Self {
        assert!(!mass.is_zero(), "manual craft output mass must be nonzero");
        Self { commodity, mass }
    }

    #[must_use]
    pub const fn commodity(self) -> CommodityKey {
        self.commodity
    }

    #[must_use]
    pub const fn mass(self) -> Mass {
        self.mass
    }
}

/// Immutable physical shaping rule for a no-machine process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManualCraftDefinition {
    process: ProcessId,
    input: CommodityKey,
    input_mass: Mass,
    duration: TickSpan,
    exertion: SurvivalExertion,
    outputs: Vec<ManualCraftOutput>,
}

impl ManualCraftDefinition {
    #[must_use]
    pub fn new(
        process: ProcessId,
        input: CommodityKey,
        input_mass: Mass,
        duration: TickSpan,
        exertion: SurvivalExertion,
        mut outputs: Vec<ManualCraftOutput>,
    ) -> Self {
        assert!(
            !input_mass.is_zero(),
            "manual craft input mass must be nonzero"
        );
        assert!(!duration.is_zero(), "manual craft duration must be nonzero");
        assert!(
            !exertion.energy_cost_per_tick().is_zero(),
            "manual craft exertion must consume metabolic energy"
        );
        assert!(
            !outputs.is_empty(),
            "manual craft must produce conserved output matter"
        );
        outputs.sort();
        for pair in outputs.windows(2) {
            assert!(
                pair[0].commodity() != pair[1].commodity(),
                "manual craft {} contains duplicate output commodity {}",
                process.value(),
                pair[0].commodity().value()
            );
        }
        let mut output_mass = Mass::ZERO;
        for output in &outputs {
            assert_eq!(
                output.commodity().material(),
                input.material(),
                "manual craft {} may change form but not material identity",
                process.value()
            );
            output_mass = output_mass.checked_add(output.mass()).unwrap_or_else(|| {
                panic!("manual craft {} output mass overflows", process.value())
            });
        }
        assert_eq!(
            output_mass,
            input_mass,
            "manual craft {} must conserve exact input mass",
            process.value()
        );
        Self {
            process,
            input,
            input_mass,
            duration,
            exertion,
            outputs,
        }
    }

    #[must_use]
    pub const fn process(&self) -> ProcessId {
        self.process
    }

    #[must_use]
    pub const fn input(&self) -> CommodityKey {
        self.input
    }

    #[must_use]
    pub const fn input_mass(&self) -> Mass {
        self.input_mass
    }

    #[must_use]
    pub const fn duration(&self) -> TickSpan {
        self.duration
    }

    #[must_use]
    pub const fn exertion(&self) -> SurvivalExertion {
        self.exertion
    }

    #[must_use]
    pub fn outputs(&self) -> &[ManualCraftOutput] {
        &self.outputs
    }
}

/// Deterministic immutable lookup for manual shaping semantics.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CraftingRegistry {
    manual: BTreeMap<ProcessId, ManualCraftDefinition>,
    producers_by_output: BTreeMap<CommodityKey, Vec<ProcessId>>,
    consumers_by_input: BTreeMap<CommodityKey, Vec<ProcessId>>,
}

impl CraftingRegistry {
    pub(crate) fn new(definitions: impl IntoIterator<Item = ManualCraftDefinition>) -> Self {
        let mut manual = BTreeMap::new();
        for definition in definitions {
            let process = definition.process();
            assert!(
                manual.insert(process, definition).is_none(),
                "duplicate manual craft process {}",
                process.value()
            );
        }
        let mut producers_by_output: BTreeMap<CommodityKey, Vec<ProcessId>> = BTreeMap::new();
        let mut consumers_by_input: BTreeMap<CommodityKey, Vec<ProcessId>> = BTreeMap::new();
        for definition in manual.values() {
            consumers_by_input
                .entry(definition.input())
                .or_default()
                .push(definition.process());
            for output in definition.outputs() {
                producers_by_output
                    .entry(output.commodity())
                    .or_default()
                    .push(definition.process());
            }
        }
        Self {
            manual,
            producers_by_output,
            consumers_by_input,
        }
    }

    #[must_use]
    pub fn get_manual(&self, process: ProcessId) -> Option<&ManualCraftDefinition> {
        self.manual.get(&process)
    }

    pub fn definitions(&self) -> impl Iterator<Item = &ManualCraftDefinition> {
        self.manual.values()
    }

    /// Iterates manual processes that directly produce the requested commodity in stable process-ID
    /// order. This is a direct authored edge, not a transitive reachability claim.
    pub fn manual_producers(
        &self,
        output: CommodityKey,
    ) -> impl Iterator<Item = &ManualCraftDefinition> {
        self.producers_by_output
            .get(&output)
            .into_iter()
            .flat_map(|processes| processes.iter())
            .map(|process| {
                self.manual.get(process).unwrap_or_else(|| {
                    unreachable!("crafting producer index references known process")
                })
            })
    }

    /// Iterates manual processes that directly consume the requested commodity in stable process-ID
    /// order. This is a direct authored edge, not a transitive reachability claim.
    pub fn manual_consumers(
        &self,
        input: CommodityKey,
    ) -> impl Iterator<Item = &ManualCraftDefinition> {
        self.consumers_by_input
            .get(&input)
            .into_iter()
            .flat_map(|processes| processes.iter())
            .map(|process| {
                self.manual.get(process).unwrap_or_else(|| {
                    unreachable!("crafting consumer index references known process")
                })
            })
    }

    pub(crate) fn validate_references(
        &self,
        production: &ProductionRegistry,
        materials: &MaterialRegistry,
    ) {
        for definition in self.manual.values() {
            assert!(
                materials.has_commodity(definition.input()),
                "manual craft {} references unknown input commodity {}",
                definition.process().value(),
                definition.input().value()
            );
            let input_form = materials
                .get_form(definition.input().form())
                .unwrap_or_else(|| {
                    panic!(
                        "manual craft {} references unknown input form {}",
                        definition.process().value(),
                        definition.input().form().value()
                    )
                });
            for output in definition.outputs() {
                assert!(
                    materials.has_commodity(output.commodity()),
                    "manual craft {} references unknown output commodity {}",
                    definition.process().value(),
                    output.commodity().value()
                );
                let output_form = materials
                    .get_form(output.commodity().form())
                    .unwrap_or_else(|| {
                        panic!(
                            "manual craft {} references unknown output form {}",
                            definition.process().value(),
                            output.commodity().form().value()
                        )
                    });
                assert_eq!(
                    output_form.phase(),
                    input_form.phase(),
                    "manual craft {} cannot change material phase from {:?} to {:?} without thermal physics",
                    definition.process().value(),
                    input_form.phase(),
                    output_form.phase()
                );
                assert_eq!(
                    output_form.particle_size_policy(),
                    ParticleSizeStatePolicy::Untracked,
                    "manual craft {} output form {} cannot require particle-size state because manual shaping has no authored particulate output distribution",
                    definition.process().value(),
                    output.commodity().form().value()
                );
            }
            let process = production
                .get_process(definition.process())
                .unwrap_or_else(|| {
                    panic!(
                        "manual craft {} has no production definition",
                        definition.process().value()
                    )
                });
            assert!(
                process.capability_requirements().is_empty(),
                "manual craft {} cannot require machine capabilities",
                definition.process().value()
            );
            let inputs = process.fixed_inputs().unwrap_or_else(|| {
                panic!(
                    "manual craft {} must use a fixed material input",
                    definition.process().value()
                )
            });
            assert_eq!(
                inputs.len(),
                1,
                "manual craft {} must have one exact input commodity",
                definition.process().value()
            );
            assert_eq!(inputs[0].commodity(), definition.input());
            assert_eq!(inputs[0].mass(), definition.input_mass());
            assert!(
                inputs[0].requires_pure_material(),
                "manual craft {} production input must require pure host material",
                definition.process().value()
            );
        }
    }
}

/// Resolves a fixed-feed hand operation into the same durable process-start path used by machines.
pub fn resolve_manual_craft(
    registries: &Registries,
    state: &AppState,
    request: ManualCraftRequest,
) -> Result<ProcessResolution, ManualCraftError> {
    let ManualCraftRequest {
        process,
        source,
        batches,
    } = request;
    let survival =
        assess_survival(registries, state).ok_or(ManualCraftError::SurvivalNotInitialized)?;
    if survival.vitality() == Vitality::ZERO {
        return Err(ManualCraftError::PlayerDead);
    }
    let definition = registries
        .crafting()
        .get_manual(process)
        .ok_or(ManualCraftError::UnknownManualProcess { process })?;
    let inputs = validate_repeated_process_inputs(registries, state, process, source, batches)
        .map_err(ManualCraftError::Input)?;
    let mut temperatures = BTreeSet::new();
    for trace in inputs.consumed_inputs() {
        temperatures.insert(trace.profile().temperature());
    }
    if temperatures.len() != 1 {
        return Err(ManualCraftError::MixedInputTemperature);
    }
    let Some(&temperature) = temperatures.first() else {
        return Err(ManualCraftError::MissingInputTrace);
    };
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
    let resolution = resolve_manual_craft(registries, state, craft)
        .map_err(StartManualCraftError::Resolution)?;
    let process =
        validate_start_manual_process(registries, state, &resolution, craft.source(), destination)
            .map_err(StartManualCraftError::Process)?;
    let exertion = registries
        .crafting()
        .get_manual(craft.process)
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
