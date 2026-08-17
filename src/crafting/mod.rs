//! Manual shaping operations that reuse canonical timed production ownership.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::inventory::StockpileId;
use crate::labor::{
    PlayerWork, PlayerWorkStartError, ValidatedPlayerWorkStart, validate_player_work_start,
};
use crate::material::{
    CommodityKey, MaterialComposition, MaterialLotSpec, MaterialLotSpecError, MaterialRegistry,
};
use crate::production::{
    ProcessId, ProcessInputError, ProcessResolution, ProcessResolutionError, ProductionJobId,
    ProductionRegistry, StartProcessCommitError, StartProcessError, ValidatedStartProcess,
    validate_process_inputs, validate_start_manual_process,
};
use crate::registry::Registries;
use crate::survival::{SurvivalExertion, Vitality, assess_survival};

mod validation;

pub use validation::ManualCraftJobValidationError;
pub(crate) use validation::validate_loaded_manual_craft_job;

/// One conserved output of a manual shaping operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ManualCraftOutput {
    commodity: CommodityKey,
    mass: Mass,
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
        Self { manual }
    }

    #[must_use]
    pub fn get_manual(&self, process: ProcessId) -> Option<&ManualCraftDefinition> {
        self.manual.get(&process)
    }

    pub fn definitions(&self) -> impl Iterator<Item = &ManualCraftDefinition> {
        self.manual.values()
    }

    pub(crate) fn process_ids(&self) -> impl Iterator<Item = ProcessId> + '_ {
        self.manual.keys().copied()
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
            for output in definition.outputs() {
                assert!(
                    materials.has_commodity(output.commodity()),
                    "manual craft {} references unknown output commodity {}",
                    definition.process().value(),
                    output.commodity().value()
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
        }
    }
}

/// Failure while resolving one exact manual shaping operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManualCraftError {
    SurvivalNotInitialized,
    PlayerDead,
    UnknownManualProcess { process: ProcessId },
    Input(ProcessInputError),
    UnsupportedComposition,
    MixedInputTemperature,
    MissingInputTrace,
    Output(MaterialLotSpecError),
    Resolution(ProcessResolutionError),
}

impl Display for ManualCraftError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SurvivalNotInitialized => {
                formatter.write_str("manual crafting requires initialized player survival")
            }
            Self::PlayerDead => formatter.write_str("dead player cannot perform manual crafting"),
            Self::UnknownManualProcess { process } => write!(
                formatter,
                "process {} is not authored as a manual craft",
                process.value()
            ),
            Self::Input(error) => write!(formatter, "manual craft input is invalid: {error}"),
            Self::UnsupportedComposition => {
                formatter.write_str("manual shaping requires pure input material")
            }
            Self::MixedInputTemperature => formatter.write_str(
                "manual shaping cannot combine different input temperatures without thermal physics",
            ),
            Self::MissingInputTrace => {
                formatter.write_str("manual shaping resolved no consumed input trace")
            }
            Self::Output(error) => write!(formatter, "manual craft output is invalid: {error}"),
            Self::Resolution(error) => write!(formatter, "manual craft resolution is invalid: {error}"),
        }
    }
}

impl Error for ManualCraftError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Input(error) => Some(error),
            Self::Output(error) => Some(error),
            Self::Resolution(error) => Some(error),
            Self::SurvivalNotInitialized
            | Self::PlayerDead
            | Self::UnknownManualProcess { process: _ }
            | Self::UnsupportedComposition
            | Self::MixedInputTemperature
            | Self::MissingInputTrace => None,
        }
    }
}

/// Resolves a fixed-feed hand operation into the same durable process-start path used by machines.
pub fn resolve_manual_craft(
    registries: &Registries,
    state: &AppState,
    process: ProcessId,
    source: StockpileId,
) -> Result<ProcessResolution, ManualCraftError> {
    let survival =
        assess_survival(registries, state).ok_or(ManualCraftError::SurvivalNotInitialized)?;
    if survival.vitality() == Vitality::ZERO {
        return Err(ManualCraftError::PlayerDead);
    }
    let definition = registries
        .crafting()
        .get_manual(process)
        .ok_or(ManualCraftError::UnknownManualProcess { process })?;
    let inputs = validate_process_inputs(registries, state, process, source)
        .map_err(ManualCraftError::Input)?;
    let expected_composition = MaterialComposition::pure(definition.input().material());
    let mut temperatures = BTreeSet::new();
    for trace in inputs.consumed_inputs() {
        if trace.profile().commodity() != definition.input()
            || trace.profile().composition() != &expected_composition
        {
            return Err(ManualCraftError::UnsupportedComposition);
        }
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
            MaterialLotSpec::with_composition(
                output.commodity(),
                output.mass(),
                temperature,
                MaterialComposition::pure(output.commodity().material()),
            )
            .map_err(ManualCraftError::Output)
        })
        .collect::<Result<Vec<_>, _>>()?;
    inputs
        .resolve_without_resources(definition.duration(), outputs)
        .map_err(ManualCraftError::Resolution)
}

/// Failure while admitting manual shaping into production and exclusive player labor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartManualCraftError {
    Resolution(ManualCraftError),
    Process(StartProcessError),
    Work(PlayerWorkStartError),
}

impl Display for StartManualCraftError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Resolution(error) => write!(formatter, "manual craft resolution failed: {error}"),
            Self::Process(error) => write!(formatter, "manual craft start failed: {error}"),
            Self::Work(error) => write!(formatter, "manual craft labor is unavailable: {error}"),
        }
    }
}

impl Error for StartManualCraftError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Resolution(error) => Some(error),
            Self::Process(error) => Some(error),
            Self::Work(error) => Some(error),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManualCraftCommitError {
    Process(StartProcessCommitError),
    Work(PlayerWorkStartError),
}

impl Display for ManualCraftCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Process(error) => {
                write!(formatter, "manual craft process commit failed: {error}")
            }
            Self::Work(error) => write!(formatter, "manual craft labor commit failed: {error}"),
        }
    }
}

impl Error for ManualCraftCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Process(error) => Some(error),
            Self::Work(error) => Some(error),
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
    process: ProcessId,
    source: StockpileId,
    destination: StockpileId,
) -> Result<ValidatedManualCraftStart, StartManualCraftError> {
    let resolution = resolve_manual_craft(registries, state, process, source)
        .map_err(StartManualCraftError::Resolution)?;
    let process =
        validate_start_manual_process(registries, state, &resolution, source, destination)
            .map_err(StartManualCraftError::Process)?;
    let work = validate_player_work_start(
        state,
        PlayerWork::ManualCraft {
            job: process.job_id(),
        },
    )
    .map_err(StartManualCraftError::Work)?;
    Ok(ValidatedManualCraftStart { process, work })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{
        FORM_CHIP, FORM_LUMP, FORM_TOOL, MATERIAL_STONE, PROCESS_KNAP_STONE_TOOL, build_registries,
    };
    use crate::core::quantity::Temperature;
    use crate::core::state::{StateValidationError, validate_loaded_state};
    use crate::core::time::WorldSeed;
    use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};
    use crate::matter::calculate_matter_accounting;
    use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
    use crate::production::{StartProcessError, validate_start_process};
    use crate::simulation::advance_tick;
    use crate::survival::{assess_survival, initialize_player_survival};

    fn stone_lump() -> CommodityKey {
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP)
    }

    fn make_fixture() -> (Registries, AppState, StockpileId, StockpileId) {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0xC4AF_7001));
        initialize_player_survival(&registries, &mut state)
            .unwrap_or_else(|error| panic!("manual craft survival initialization failed: {error}"));
        let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_000))
            .unwrap_or_else(|error| panic!("manual craft source fixture failed: {error}"));
        let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_000))
            .unwrap_or_else(|error| panic!("manual craft destination fixture failed: {error}"));
        deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            stone_lump(),
            Mass::from_milligrams(1_000),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("manual craft stone fixture failed: {error}"));
        (registries, state, source, destination)
    }

    #[test]
    fn stone_knapping_is_timed_conserved_hand_work() {
        let (registries, mut state, source, destination) = make_fixture();
        let matter_before = calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("manual craft initial accounting failed: {error}"));
        let survival_before = assess_survival(&registries, &state)
            .unwrap_or_else(|| panic!("manual craft survival state is missing"));
        let resolution = resolve_manual_craft(&registries, &state, PROCESS_KNAP_STONE_TOOL, source)
            .unwrap_or_else(|error| panic!("stone knapping resolution failed: {error}"));
        assert_eq!(resolution.duration(), TickSpan::new(40));
        assert_eq!(
            validate_start_process(&registries, &state, &resolution, source, destination),
            Err(StartProcessError::ManualCraftRequiresPlayerWork {
                process: PROCESS_KNAP_STONE_TOOL,
            })
        );
        let token = validate_start_manual_craft(
            &registries,
            &state,
            PROCESS_KNAP_STONE_TOOL,
            source,
            destination,
        )
        .unwrap_or_else(|error| panic!("stone knapping start failed: {error}"));
        token
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("stone knapping commit failed: {error}"));
        assert!(matches!(
            state.player_work().active(),
            Some(PlayerWork::ManualCraft { .. })
        ));

        for _ in 0..resolution.duration().value() {
            advance_tick(&registries, &mut state)
                .unwrap_or_else(|error| panic!("stone knapping tick failed: {error}"));
        }
        assert_eq!(state.player_work().active(), None);

        let destination_record = state
            .inventory()
            .get_stockpile(destination)
            .unwrap_or_else(|| panic!("stone knapping destination disappeared"));
        assert_eq!(
            destination_record.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_TOOL)),
            Mass::from_milligrams(800)
        );
        assert_eq!(
            destination_record.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_CHIP)),
            Mass::from_milligrams(200)
        );
        let matter_after = calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("manual craft final accounting failed: {error}"));
        assert_eq!(matter_before.total(), matter_after.total());
        assert!(
            assess_survival(&registries, &state)
                .unwrap_or_else(|| panic!("manual craft survival state disappeared"))
                .metabolic_energy()
                < survival_before.metabolic_energy()
        );
        validate_loaded_state(&registries, &state)
            .unwrap_or_else(|error| panic!("stone knapping final audit failed: {error}"));
    }

    #[test]
    fn manual_craft_load_audit_rejects_forged_duration() {
        let (registries, mut state, source, destination) = make_fixture();
        let token = validate_start_manual_craft(
            &registries,
            &state,
            PROCESS_KNAP_STONE_TOOL,
            source,
            destination,
        )
        .unwrap_or_else(|error| panic!("manual craft tamper start failed: {error}"));
        let job = token
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("manual craft tamper commit failed: {error}"));
        let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
            .unwrap_or_else(|error| panic!("manual craft tamper serialization failed: {error}"));
        encoded["state"]["systems"]["production"]["jobs"][job.value().to_string()]["schedule"]["active_duration"] =
            serde_json::json!(41_u64);
        let tampered: LoadedSaveEnvelope = serde_json::from_value(encoded)
            .unwrap_or_else(|error| panic!("manual craft tamper decode failed: {error}"));

        assert_eq!(
            tampered.into_state(&registries),
            Err(LoadError::InvalidState(
                StateValidationError::ManualCraftJob(
                    ManualCraftJobValidationError::DurationMismatch {
                        job,
                        stored: TickSpan::new(41),
                        authored: TickSpan::new(40),
                    }
                )
            ))
        );
    }
}
