//! Matched-world workshop policy counterfactuals and agency evidence.

use std::collections::BTreeSet;
use std::env;

use super::configuration::MaintainedAnchor;
#[cfg(not(test))]
use super::fresh_seed::fresh_root;
#[cfg(not(test))]
use super::output::has_verbose_output;
use super::report::{
    EnergyRecoveryPreference, MaintenancePreference, PowerPreference, ScenarioPolicyVariation,
    ScenarioReport, StructuralPreference,
};
use super::scenario::ScenarioVariation;
#[cfg(not(test))]
use super::seed::MAINTAINED_VARIATION_ROOT;
use super::seed::mix64;
use super::seed_input::parse_seed;
use super::workshop::runner::run_scenario;
use deep_hearth::content::build_registries;
use deep_hearth::core::quantity::{Energy, Mass};
use deep_hearth::registry::Registries;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AgencyPolicyVariant {
    Baseline,
    FinishSooner,
    SpendSurvival,
    DelayMaintenance,
    FailureOnlyStructure,
}

impl AgencyPolicyVariant {
    const fn label(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::FinishSooner => "finish-sooner-only",
            Self::SpendSurvival => "spend-survival-only",
            Self::DelayMaintenance => "delay-maintenance-only",
            Self::FailureOnlyStructure => "failure-only-structure",
        }
    }
}

fn agency_probe_policies() -> [(AgencyPolicyVariant, ScenarioPolicyVariation); 5] {
    let baseline = ScenarioPolicyVariation {
        power_preference: PowerPreference::PreserveReserve,
        energy_recovery_preference: EnergyRecoveryPreference::ProtectSurvival,
        maintenance_preference: MaintenancePreference::ServiceAtWarning,
        structural_preference: StructuralPreference::PreserveMargin,
    };
    [
        (AgencyPolicyVariant::Baseline, baseline),
        (
            AgencyPolicyVariant::FinishSooner,
            ScenarioPolicyVariation {
                power_preference: PowerPreference::FinishSooner,
                ..baseline
            },
        ),
        (
            AgencyPolicyVariant::SpendSurvival,
            ScenarioPolicyVariation {
                energy_recovery_preference: EnergyRecoveryPreference::SpendSurvivalReserve,
                ..baseline
            },
        ),
        (
            AgencyPolicyVariant::DelayMaintenance,
            ScenarioPolicyVariation {
                maintenance_preference: MaintenancePreference::ServiceAtCritical,
                ..baseline
            },
        ),
        (
            AgencyPolicyVariant::FailureOnlyStructure,
            ScenarioPolicyVariation {
                structural_preference: StructuralPreference::MoveOnlyForFailure,
                ..baseline
            },
        ),
    ]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AgencyFocus {
    PowerAndStructure,
    SurvivalRecovery,
    MaintenanceTiming,
    ShortOrderMaintenanceDeferral,
    OrganicVariation,
    OrganicPressureSearch,
}

impl AgencyFocus {
    #[cfg(not(test))]
    const fn label(self) -> &'static str {
        match self {
            Self::PowerAndStructure => "power+structure",
            Self::SurvivalRecovery => "survival-recovery",
            Self::MaintenanceTiming => "maintenance-timing",
            Self::ShortOrderMaintenanceDeferral => "maintenance-deferral",
            Self::OrganicVariation => "organic-unfiltered",
            Self::OrganicPressureSearch => "organic-pressure-qualified",
        }
    }
}

#[derive(Clone, Copy)]
struct AgencyWorld {
    focus: AgencyFocus,
    world_seed: u64,
    anchor: Option<MaintainedAnchor>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
// Physical outcomes only. Policy bookkeeping must never manufacture an agency path.
struct AgencyPathSignature {
    processed_mass: Mass,
    operations_completed: u16,
    adaptive_batch_operations: u16,
    small_drive_batches: u16,
    large_drive_batches: u16,
    manual_recharges: u16,
    maintenance_services: u8,
    support_relocation: bool,
    production_suspension: bool,
    stranded_work_in_process: bool,
    structural_stop: bool,
    final_condition_ppm: u32,
    small_drive_remaining: Energy,
    large_drive_remaining: Energy,
    maintenance_stock_remaining: Mass,
    episode_end_tick: u64,
    elapsed_ticks: u64,
    metabolic_energy_spent: Energy,
    manual_power_metabolic_energy: Energy,
}

impl AgencyPathSignature {
    fn from_report(report: &ScenarioReport) -> Self {
        Self {
            processed_mass: report.progress.processed_mass,
            operations_completed: report.progress.operations_completed,
            adaptive_batch_operations: report.progress.adaptive_batch_operations,
            small_drive_batches: report.choices.small_drive_batches,
            large_drive_batches: report.choices.large_drive_batches,
            manual_recharges: report.choices.manual_recharges,
            maintenance_services: report.maintenance.services,
            support_relocation: report.structure.support_relocation,
            production_suspension: report.structure.production_suspension,
            stranded_work_in_process: report.structure.stranded_work_in_process,
            structural_stop: report.structure.structural_stop,
            final_condition_ppm: report.resources.final_condition_ppm,
            small_drive_remaining: report.resources.small_drive_remaining,
            large_drive_remaining: report.resources.large_drive_remaining,
            maintenance_stock_remaining: report.resources.maintenance_stock_remaining,
            episode_end_tick: report.resources.episode_end_tick,
            elapsed_ticks: report.resources.elapsed_ticks,
            metabolic_energy_spent: report.resources.metabolic_energy_spent,
            manual_power_metabolic_energy: report.resources.manual_power_metabolic_energy,
        }
    }
}

fn power_counterfactual_changed(baseline: &ScenarioReport, variant: &ScenarioReport) -> bool {
    baseline.choices.small_drive_batches != variant.choices.small_drive_batches
        || baseline.choices.large_drive_batches != variant.choices.large_drive_batches
        || baseline.resources.small_drive_remaining != variant.resources.small_drive_remaining
        || baseline.resources.large_drive_remaining != variant.resources.large_drive_remaining
        || baseline.resources.episode_end_tick != variant.resources.episode_end_tick
}

fn agency_report(
    reports: &[(AgencyPolicyVariant, ScenarioReport)],
    variant: AgencyPolicyVariant,
) -> &ScenarioReport {
    let mut matches = reports
        .iter()
        .filter(|(candidate, _)| *candidate == variant)
        .map(|(_, report)| report);
    let report = matches.next().unwrap_or_else(|| {
        panic!(
            "agency probe is missing the {} policy variant",
            variant.label()
        )
    });
    assert!(
        matches.next().is_none(),
        "agency probe contains duplicate {} policy variants",
        variant.label()
    );
    report
}

fn survival_counterfactual_changed(baseline: &ScenarioReport, variant: &ScenarioReport) -> bool {
    baseline.progress.processed_mass != variant.progress.processed_mass
        || baseline.limits.energy_stop != variant.limits.energy_stop
        || baseline.limits.manual_recovery_declined != variant.limits.manual_recovery_declined
        || baseline.resources.metabolic_energy_spent != variant.resources.metabolic_energy_spent
        || baseline.resources.hydration_spent != variant.resources.hydration_spent
}

fn maintenance_counterfactual_changed(baseline: &ScenarioReport, variant: &ScenarioReport) -> bool {
    baseline.maintenance.services != variant.maintenance.services
        || baseline.maintenance.replacement_spent != variant.maintenance.replacement_spent
        || baseline.resources.final_condition_ppm != variant.resources.final_condition_ppm
        || baseline.resources.episode_end_tick != variant.resources.episode_end_tick
        || baseline.progress.processed_mass != variant.progress.processed_mass
}

fn structure_counterfactual_changed(baseline: &ScenarioReport, variant: &ScenarioReport) -> bool {
    baseline.structure.support_relocation != variant.structure.support_relocation
        || baseline.structure.structural_damage_debt != variant.structure.structural_damage_debt
        || baseline.structure.structural_stop != variant.structure.structural_stop
        || baseline.structure.production_suspension != variant.structure.production_suspension
        || baseline.progress.processed_mass != variant.progress.processed_mass
        || baseline.resources.episode_end_tick != variant.resources.episode_end_tick
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AgencyEvidence {
    Actionable,
    ObjectiveResolved,
    StructuralCapacity,
    MaintenanceSupply,
    MaintenanceSafety,
    ManualRecoveryDeclined,
    ManualRecoverySurvivalLimited,
    StoredWorkInsufficient,
    DormantPolicyPressure,
}

impl AgencyEvidence {
    #[cfg(not(test))]
    const fn label(self) -> &'static str {
        match self {
            Self::Actionable => "actionable",
            Self::ObjectiveResolved => "non-actionable:objective-resolved",
            Self::StructuralCapacity => "terminal-world-constraint:structural-capacity",
            Self::MaintenanceSupply => "terminal-world-constraint:maintenance-supply",
            Self::MaintenanceSafety => "terminal-world-constraint:maintenance-safety",
            Self::ManualRecoveryDeclined => "terminal-world-constraint:survival-policy-floor",
            Self::ManualRecoverySurvivalLimited => {
                "terminal-world-constraint:survival-reserve-exhausted"
            }
            Self::StoredWorkInsufficient => "terminal-world-constraint:stored-work-insufficient",
            Self::DormantPolicyPressure => "dormant-policy-pressure",
        }
    }
}

fn terminal_evidence(report: &ScenarioReport) -> Option<AgencyEvidence> {
    if report.structure.structural_stop {
        Some(AgencyEvidence::StructuralCapacity)
    } else if report.limits.maintenance_stop
        && report.resources.maintenance_stock_remaining.is_zero()
    {
        Some(AgencyEvidence::MaintenanceSupply)
    } else if report.limits.maintenance_stop {
        Some(AgencyEvidence::MaintenanceSafety)
    } else if report.limits.energy_stop && report.limits.manual_recovery_declined {
        Some(AgencyEvidence::ManualRecoveryDeclined)
    } else if report.limits.energy_stop && report.limits.manual_recovery_survival_limited {
        Some(AgencyEvidence::ManualRecoverySurvivalLimited)
    } else if report.limits.energy_stop {
        Some(AgencyEvidence::StoredWorkInsufficient)
    } else {
        None
    }
}

fn classify_agency_evidence(
    reports: &[(AgencyPolicyVariant, ScenarioReport)],
    actionable: bool,
) -> AgencyEvidence {
    if actionable {
        return AgencyEvidence::Actionable;
    }
    if reports
        .iter()
        .all(|(_, report)| report.progress.processed_mass == report.progress.target_mass)
    {
        return AgencyEvidence::ObjectiveResolved;
    }
    let baseline = agency_report(reports, AgencyPolicyVariant::Baseline);
    if let Some(evidence) = terminal_evidence(baseline)
        && reports
            .iter()
            .all(|(_, report)| terminal_evidence(report) == Some(evidence))
    {
        return evidence;
    }
    AgencyEvidence::DormantPolicyPressure
}

// Exploration budgets constrain evidence only; they never authorize or prohibit production.
const ORGANIC_UNFILTERED_COUNT: usize = 3;
const ORGANIC_SEARCH_LIMIT: usize = 24;
const ORGANIC_QUALIFIED_TARGET: usize = 2;

struct AgencyWorldEvaluation {
    reports: Vec<(AgencyPolicyVariant, ScenarioReport)>,
    signatures: usize,
    processed_min: u64,
    processed_max: u64,
    power_effect: bool,
    survival_effect: bool,
    maintenance_effect: bool,
    structure_effect: bool,
}

impl AgencyWorldEvaluation {
    const fn actionable(&self) -> bool {
        self.power_effect
            || self.survival_effect
            || self.maintenance_effect
            || self.structure_effect
    }
}

#[derive(Default)]
struct AgencyProbeSummary {
    worlds_with_distinct_paths: usize,
    worlds_with_work_difference: usize,
    observed_power_effect: bool,
    observed_survival_effect: bool,
    observed_maintenance_effect: bool,
    observed_structure_effect: bool,
    demonstrated_short_order_maintenance_deferral: bool,
    organic_worlds: usize,
    organic_actionable_worlds: usize,
    organic_objective_resolved_worlds: usize,
    organic_terminal_worlds: usize,
    organic_dormant_worlds: usize,
    search_attempts: usize,
    qualified_seeds: Vec<u64>,
}

impl AgencyProbeSummary {
    fn admit(&mut self, world: AgencyWorld) -> bool {
        if world.focus != AgencyFocus::OrganicPressureSearch {
            return true;
        }
        if self.search_attempts == ORGANIC_SEARCH_LIMIT
            || self.qualified_seeds.len() == ORGANIC_QUALIFIED_TARGET
        {
            return false;
        }
        self.search_attempts += 1;
        true
    }

    fn record_evaluation(&mut self, evaluation: &AgencyWorldEvaluation) {
        self.worlds_with_distinct_paths += usize::from(evaluation.signatures > 1);
        self.worlds_with_work_difference +=
            usize::from(evaluation.processed_min != evaluation.processed_max);
        self.observed_power_effect |= evaluation.power_effect;
        self.observed_survival_effect |= evaluation.survival_effect;
        self.observed_maintenance_effect |= evaluation.maintenance_effect;
        self.observed_structure_effect |= evaluation.structure_effect;
    }

    fn record_organic_evidence(&mut self, focus: AgencyFocus, evidence: AgencyEvidence) {
        if focus != AgencyFocus::OrganicVariation {
            return;
        }
        self.organic_worlds += 1;
        match evidence {
            AgencyEvidence::Actionable => self.organic_actionable_worlds += 1,
            AgencyEvidence::ObjectiveResolved => self.organic_objective_resolved_worlds += 1,
            AgencyEvidence::StructuralCapacity
            | AgencyEvidence::MaintenanceSupply
            | AgencyEvidence::MaintenanceSafety
            | AgencyEvidence::ManualRecoveryDeclined
            | AgencyEvidence::ManualRecoverySurvivalLimited
            | AgencyEvidence::StoredWorkInsufficient => self.organic_terminal_worlds += 1,
            AgencyEvidence::DormantPolicyPressure => self.organic_dormant_worlds += 1,
        }
    }

    fn assert_partition(&self) {
        assert_eq!(
            self.organic_actionable_worlds
                + self.organic_objective_resolved_worlds
                + self.organic_terminal_worlds
                + self.organic_dormant_worlds,
            self.organic_worlds,
            "organic agency evidence classes must partition sampled worlds"
        );
    }
}

fn run_policy_reports(
    registries: &Registries,
    world: AgencyWorld,
    behavior_seed: u64,
    observation_horizon: Option<u64>,
) -> Vec<(AgencyPolicyVariant, ScenarioReport)> {
    let policies = agency_probe_policies();
    let mut reports = Vec::with_capacity(policies.len());
    for (variant, policy) in policies {
        let mut variation = ScenarioVariation::from_seeds(
            registries,
            world.world_seed,
            behavior_seed,
            world.anchor,
        );
        variation.policy = policy;
        let report = run_scenario(registries, variation, observation_horizon);
        assert_eq!(
            report.world_seed, world.world_seed,
            "agency counterfactual must preserve the matched world seed"
        );
        assert_eq!(
            report.behavior_seed, behavior_seed,
            "agency counterfactual must preserve the matched behavior seed"
        );
        if let Some(horizon) = observation_horizon {
            assert_eq!(
                report.resources.elapsed_ticks, horizon,
                "agency counterfactual branches must use one policy-independent observation horizon"
            );
        }
        reports.push((variant, report));
    }
    reports
}

fn evaluate_agency_world(registries: &Registries, world: AgencyWorld) -> AgencyWorldEvaluation {
    let behavior_seed = mix64(world.world_seed ^ 0xA63E_4E43_5900_0001);
    let preliminary_reports = run_policy_reports(registries, world, behavior_seed, None);
    let comparison_horizon = preliminary_reports
        .iter()
        .map(|(_, report)| report.resources.episode_end_tick)
        .max()
        .unwrap_or_else(|| unreachable!("agency probe policy set is nonempty"));
    let matched_inputs = preliminary_reports
        .first()
        .map(|(_, report)| report.inputs)
        .unwrap_or_else(|| unreachable!("agency probe policy set is nonempty"));
    assert!(
        preliminary_reports
            .iter()
            .all(|(_, report)| report.inputs == matched_inputs),
        "agency policy variants must preserve the same physical setup and controlled-event schedule"
    );

    let reports = run_policy_reports(registries, world, behavior_seed, Some(comparison_horizon));
    assert!(
        reports
            .iter()
            .all(|(_, report)| report.inputs == matched_inputs),
        "agency counterfactual rerun must preserve the matched physical setup"
    );
    let initial_support_choice = reports
        .first()
        .map(|(_, report)| report.choices.chose_compact_support)
        .unwrap_or_else(|| unreachable!("agency probe policy set is nonempty"));
    assert!(
        reports
            .iter()
            .all(|(_, report)| report.choices.chose_compact_support == initial_support_choice),
        "one-factor agency policies must not alter the policy-independent initial structural choice"
    );
    let processed_min = reports
        .iter()
        .map(|(_, report)| report.progress.processed_mass.milligrams())
        .min()
        .unwrap_or_else(|| unreachable!("agency probe policy set is nonempty"));
    let processed_max = reports
        .iter()
        .map(|(_, report)| report.progress.processed_mass.milligrams())
        .max()
        .unwrap_or_else(|| unreachable!("agency probe policy set is nonempty"));
    let signatures = reports
        .iter()
        .map(|(_, report)| AgencyPathSignature::from_report(report))
        .collect::<BTreeSet<_>>()
        .len();
    let baseline = agency_report(&reports, AgencyPolicyVariant::Baseline);
    AgencyWorldEvaluation {
        signatures,
        processed_min,
        processed_max,
        power_effect: power_counterfactual_changed(
            baseline,
            agency_report(&reports, AgencyPolicyVariant::FinishSooner),
        ),
        survival_effect: survival_counterfactual_changed(
            baseline,
            agency_report(&reports, AgencyPolicyVariant::SpendSurvival),
        ),
        maintenance_effect: maintenance_counterfactual_changed(
            baseline,
            agency_report(&reports, AgencyPolicyVariant::DelayMaintenance),
        ),
        structure_effect: structure_counterfactual_changed(
            baseline,
            agency_report(&reports, AgencyPolicyVariant::FailureOnlyStructure),
        ),
        reports,
    }
}

fn assert_focus_contract(focus: AgencyFocus, evaluation: &AgencyWorldEvaluation) -> bool {
    let baseline = agency_report(&evaluation.reports, AgencyPolicyVariant::Baseline);
    match focus {
        AgencyFocus::PowerAndStructure => {
            assert!(
                evaluation.power_effect && evaluation.structure_effect,
                "maintained power+structure agency world must make both one-factor choices consequential"
            );
        }
        AgencyFocus::SurvivalRecovery => {
            let spend_survival =
                agency_report(&evaluation.reports, AgencyPolicyVariant::SpendSurvival);
            assert!(
                evaluation.survival_effect
                    && baseline.limits.manual_recovery_declined
                    && spend_survival.progress.processed_mass > baseline.progress.processed_mass,
                "maintained survival-recovery agency world must trade protected reserves against additional useful work"
            );
        }
        AgencyFocus::MaintenanceTiming => {
            let delay_maintenance =
                agency_report(&evaluation.reports, AgencyPolicyVariant::DelayMaintenance);
            assert!(
                evaluation.maintenance_effect,
                "maintained condition-pressure agency world must make warning-service versus critical-only timing consequential"
            );
            assert!(
                baseline.maintenance.services > 0
                    && baseline.maintenance.replacement_spent > Mass::ZERO,
                "warning-service policy must perform real preventive maintenance under maintained condition pressure"
            );
            assert!(
                baseline.progress.condition_adaptive_batch_operations
                    < delay_maintenance
                        .progress
                        .condition_adaptive_batch_operations,
                "preventive warning service must avoid condition-limited batch adaptation that critical-only timing incurs"
            );
        }
        AgencyFocus::ShortOrderMaintenanceDeferral => {
            assert!(
                !evaluation.maintenance_effect,
                "in this short world safe batches are shorter than authored service, so warning-deferral and critical-only policies must rationally agree"
            );
            assert_eq!(
                baseline.progress.processed_mass, baseline.progress.target_mass,
                "maintained short-order maintenance-deferral world must still complete its demand"
            );
            assert_eq!(
                baseline.maintenance.services, 0,
                "warning-deferral policy must not pay authored service during a short order"
            );
            assert!(
                baseline.maintenance.replacement_spent.is_zero(),
                "deferred warning service must preserve replacement stock"
            );
            return true;
        }
        AgencyFocus::OrganicVariation | AgencyFocus::OrganicPressureSearch => {}
    }
    false
}

#[cfg(not(test))]
fn report_unqualified_search(world_seed: u64, evidence: AgencyEvidence) {
    if has_verbose_output() {
        std::println!(
            "AGENCY SEARCH world=0x{world_seed:016X} qualification=unqualified evidence={}",
            evidence.label(),
        );
    }
}

#[cfg(test)]
fn report_unqualified_search(_world_seed: u64, _evidence: AgencyEvidence) {}

#[cfg(not(test))]
fn min_max<T: Copy + Ord>(values: impl IntoIterator<Item = T>) -> (T, T) {
    let mut values = values.into_iter();
    let first = values
        .next()
        .unwrap_or_else(|| unreachable!("agency probe policy set is nonempty"));
    values.fold((first, first), |(minimum, maximum), value| {
        (minimum.min(value), maximum.max(value))
    })
}

#[cfg(not(test))]
struct AgencyReportMetrics {
    adaptive: (u64, u64),
    high_power: (u64, u64),
    manual_recharges: (u64, u64),
    services: (u64, u64),
    condition: (u64, u64),
    episode_end: (u64, u64),
    survival_energy: (u128, u128),
    relocations: usize,
    suspensions: usize,
    horizon: u64,
}

#[cfg(not(test))]
fn agency_report_metrics(reports: &[(AgencyPolicyVariant, ScenarioReport)]) -> AgencyReportMetrics {
    let adaptive = min_max(
        reports
            .iter()
            .map(|(_, report)| u64::from(report.progress.adaptive_batch_operations)),
    );
    let high_power = min_max(
        reports
            .iter()
            .map(|(_, report)| u64::from(report.choices.large_drive_batches)),
    );
    let manual_recharges = min_max(
        reports
            .iter()
            .map(|(_, report)| u64::from(report.choices.manual_recharges)),
    );
    let services = min_max(
        reports
            .iter()
            .map(|(_, report)| u64::from(report.maintenance.services)),
    );
    let condition = min_max(
        reports
            .iter()
            .map(|(_, report)| u64::from(report.resources.final_condition_ppm)),
    );
    let episode_end = min_max(
        reports
            .iter()
            .map(|(_, report)| report.resources.episode_end_tick),
    );
    let survival_energy = min_max(
        reports
            .iter()
            .map(|(_, report)| report.resources.metabolic_energy_spent.nanojoules()),
    );
    let relocations = reports
        .iter()
        .filter(|(_, report)| report.structure.support_relocation)
        .count();
    let suspensions = reports
        .iter()
        .filter(|(_, report)| report.structure.production_suspension)
        .count();
    let horizon = reports
        .first()
        .map(|(_, report)| report.resources.elapsed_ticks)
        .unwrap_or_else(|| unreachable!("agency probe policy set is nonempty"));
    AgencyReportMetrics {
        adaptive,
        high_power,
        manual_recharges,
        services,
        condition,
        episode_end,
        survival_energy,
        relocations,
        suspensions,
        horizon,
    }
}

#[cfg(not(test))]
fn report_agency_paths(world: AgencyWorld, reports: &[(AgencyPolicyVariant, ScenarioReport)]) {
    let policy_paths = reports
        .iter()
        .map(|(variant, report)| {
            format!(
                "{}:ore{}/{}-ops{}-adapt{}-hi{}-manual{}-maint{}-reloc{}-susp{}-choices[p:{} f:{}]-episode{}-horizon{}-body{}-manualbody{}-c{}-lo{}-hi{}",
                variant.label(),
                report.progress.processed_mass.milligrams(),
                report.progress.target_mass.milligrams(),
                report.progress.operations_completed,
                report.progress.adaptive_batch_operations,
                report.choices.large_drive_batches,
                report.choices.manual_recharges,
                report.maintenance.services,
                u8::from(report.structure.support_relocation),
                u8::from(report.structure.production_suspension),
                report.choices.policy_power_choices,
                report.choices.single_source_power_choices,
                report.resources.episode_end_tick,
                report.resources.elapsed_ticks,
                report.resources.metabolic_energy_spent.nanojoules(),
                report.resources.manual_power_metabolic_energy.nanojoules(),
                report.resources.final_condition_ppm,
                report.resources.small_drive_remaining.nanojoules(),
                report.resources.large_drive_remaining.nanojoules(),
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    std::println!(
        "AGENCY PATHS focus={} world=0x{:016X} paths=[{policy_paths}]",
        world.focus.label(),
        world.world_seed,
    );
}

#[cfg(not(test))]
fn report_agency_world(
    world: AgencyWorld,
    evaluation: &AgencyWorldEvaluation,
    evidence: AgencyEvidence,
) {
    if !has_verbose_output() {
        return;
    }
    let reports = &evaluation.reports;
    let metrics = agency_report_metrics(reports);
    std::println!(
        "AGENCY focus={} world=0x{:016X} variants={} physical-paths={} evidence={} horizon={}t actionable=[power:{} survival:{} maintenance:{} structure:{}] policy-effects=[processed:{}..{}mg adaptive:{}..{} high-power:{}..{} manual-recharges:{}..{} services:{}..{} final-condition:{}..{}ppm relocations:{}/{} suspensions:{}/{} episode-end:{}..{}t survival-energy:{}..{}nJ]",
        world.focus.label(),
        world.world_seed,
        reports.len(),
        evaluation.signatures,
        evidence.label(),
        metrics.horizon,
        evaluation.power_effect,
        evaluation.survival_effect,
        evaluation.maintenance_effect,
        evaluation.structure_effect,
        evaluation.processed_min,
        evaluation.processed_max,
        metrics.adaptive.0,
        metrics.adaptive.1,
        metrics.high_power.0,
        metrics.high_power.1,
        metrics.manual_recharges.0,
        metrics.manual_recharges.1,
        metrics.services.0,
        metrics.services.1,
        metrics.condition.0,
        metrics.condition.1,
        metrics.relocations,
        reports.len(),
        metrics.suspensions,
        reports.len(),
        metrics.episode_end.0,
        metrics.episode_end.1,
        metrics.survival_energy.0,
        metrics.survival_energy.1,
    );
    report_agency_paths(world, reports);
}

#[cfg(test)]
fn report_agency_world(
    _world: AgencyWorld,
    _evaluation: &AgencyWorldEvaluation,
    _evidence: AgencyEvidence,
) {
}

fn run_agency_probe(registries: &Registries, worlds: &[AgencyWorld]) -> Vec<u64> {
    let mut summary = AgencyProbeSummary::default();
    for &world in worlds {
        if !summary.admit(world) {
            continue;
        }
        let evaluation = evaluate_agency_world(registries, world);
        let actionable = evaluation.actionable();
        let evidence = classify_agency_evidence(&evaluation.reports, actionable);
        if world.focus == AgencyFocus::OrganicPressureSearch && !actionable {
            report_unqualified_search(world.world_seed, evidence);
            continue;
        }
        if world.focus == AgencyFocus::OrganicPressureSearch {
            summary.qualified_seeds.push(world.world_seed);
        }
        summary.record_evaluation(&evaluation);
        summary.demonstrated_short_order_maintenance_deferral |=
            assert_focus_contract(world.focus, &evaluation);
        summary.record_organic_evidence(world.focus, evidence);
        report_agency_world(world, &evaluation, evidence);
    }
    summary.assert_partition();
    std::println!(
        "AGENCY SUMMARY worlds={} worlds-with-multiple-signatures={} processed-work-differences={} observed-counterfactual-effects=[power:{} survival:{} maintenance:{} structure:{}] maintained-contracts=[short-order-maintenance-deferral:{}] organic-unfiltered=[actionable:{}/{} objective-resolved:{} terminal-constraint:{} dormant-policy-pressure:{}] organic-search=[qualified:{} target:{} unqualified:{} attempted:{} limit:{}] search-basis=outcome-selected-not-prevalence search-bound=evidence-not-production-legality basis=matched-world-one-factor-counterfactual+shared-observation-horizon+reason-specific-absence-classification",
        worlds
            .iter()
            .filter(|world| world.focus != AgencyFocus::OrganicPressureSearch)
            .count()
            + summary.qualified_seeds.len(),
        summary.worlds_with_distinct_paths,
        summary.worlds_with_work_difference,
        summary.observed_power_effect,
        summary.observed_survival_effect,
        summary.observed_maintenance_effect,
        summary.observed_structure_effect,
        summary.demonstrated_short_order_maintenance_deferral,
        summary.organic_actionable_worlds,
        summary.organic_worlds,
        summary.organic_objective_resolved_worlds,
        summary.organic_terminal_worlds,
        summary.organic_dormant_worlds,
        summary.qualified_seeds.len(),
        ORGANIC_QUALIFIED_TARGET,
        summary.search_attempts - summary.qualified_seeds.len(),
        summary.search_attempts,
        ORGANIC_SEARCH_LIMIT,
    );
    summary.qualified_seeds
}

fn organic_agency_worlds(variation_root: u64, count: usize) -> Vec<AgencyWorld> {
    let mut worlds = Vec::with_capacity(count);
    let mut world_seed = variation_root ^ 0xA63E_4E43_594F_5247;
    for index in 0..count {
        world_seed = mix64(
            world_seed
                ^ (u64::try_from(index + 1)
                    .unwrap_or_else(|_| unreachable!("bounded agency sample index fits u64"))
                    .wrapping_mul(0xD1B5_4A32_D192_ED03)),
        );
        worlds.push(AgencyWorld {
            focus: AgencyFocus::OrganicVariation,
            world_seed,
            anchor: None,
        });
    }
    worlds
}

fn exploratory_agency_worlds(variation_root: u64) -> Vec<AgencyWorld> {
    let mut worlds = maintained_agency_worlds();
    // Keep the bounded unfiltered prefix, then search the continuation of the same
    // deterministic stream. No fixture mutation or fresh entropy during search.
    worlds.extend(
        organic_agency_worlds(
            variation_root,
            ORGANIC_UNFILTERED_COUNT + ORGANIC_SEARCH_LIMIT,
        )
        .into_iter()
        .enumerate()
        .map(|(index, mut world)| {
            if index >= ORGANIC_UNFILTERED_COUNT {
                world.focus = AgencyFocus::OrganicPressureSearch;
            }
            world
        }),
    );
    worlds
}

fn configured_agency_root() -> Option<u64> {
    env::var("DEEP_HEARTH_GAMEPLAY_VARIATION_SEED")
        .ok()
        .map(|raw| {
            parse_seed(&raw)
                .unwrap_or_else(|| panic!("agency gameplay variation seed is invalid: {raw:?}"))
        })
}

#[cfg(not(test))]
fn exploratory_agency_root() -> u64 {
    configured_agency_root()
        .unwrap_or_else(|| fresh_root(MAINTAINED_VARIATION_ROOT ^ 0xA63E_4E43_595F_4652))
}

fn maintained_agency_worlds() -> Vec<AgencyWorld> {
    vec![
        AgencyWorld {
            focus: AgencyFocus::PowerAndStructure,
            world_seed: 1,
            anchor: Some(MaintainedAnchor::NormalBaseline),
        },
        AgencyWorld {
            focus: AgencyFocus::SurvivalRecovery,
            world_seed: 0x1F65_DBFE_4A87_A054,
            anchor: Some(MaintainedAnchor::SurvivalRecovery),
        },
        AgencyWorld {
            focus: AgencyFocus::MaintenanceTiming,
            world_seed: 29,
            anchor: Some(MaintainedAnchor::ConditionPressure),
        },
        AgencyWorld {
            focus: AgencyFocus::ShortOrderMaintenanceDeferral,
            world_seed: 4,
            anchor: Some(MaintainedAnchor::WarningMaintenance),
        },
    ]
}

#[cfg(test)]
pub(super) fn run_gameplay_agency_counterfactuals() {
    let registries = build_registries();
    let variation_root = configured_agency_root();
    let mut worlds = maintained_agency_worlds();
    if let Some(root) = variation_root {
        worlds.extend(organic_agency_worlds(root, 1));
    }
    let variation_label = variation_root
        .map(|root| format!("0x{root:016X}"))
        .unwrap_or_else(|| "n/a".to_owned());
    std::println!(
        "AGENCY INPUT mode=gate organic={} variation_root={variation_label}",
        usize::from(variation_root.is_some())
    );
    run_agency_probe(&registries, &worlds);
}

#[cfg(not(test))]
pub(super) fn run_exploratory_agency_counterfactuals() {
    let registries = build_registries();
    let variation_root = exploratory_agency_root();
    std::println!(
        "AGENCY INPUT mode=explore organic={ORGANIC_UNFILTERED_COUNT} variation_root=0x{variation_root:016X} organic-kind=unfiltered search-target={ORGANIC_QUALIFIED_TARGET} search-limit={ORGANIC_SEARCH_LIMIT}"
    );
    let worlds = exploratory_agency_worlds(variation_root);
    run_agency_probe(&registries, &worlds);
}

#[test]
fn gameplay_agency_bounded_search_preserves_unfiltered_replay() {
    let registries = build_registries();
    let root = 0x16F6_C93F_A53A_1C98;
    let worlds = exploratory_agency_worlds(root);
    let maintained_count = maintained_agency_worlds().len();
    let unfiltered = organic_agency_worlds(root, ORGANIC_UNFILTERED_COUNT);
    for (actual, original) in worlds
        .iter()
        .skip(maintained_count)
        .take(ORGANIC_UNFILTERED_COUNT)
        .zip(&unfiltered)
    {
        assert_eq!(actual.world_seed, original.world_seed);
        assert_eq!(actual.focus, AgencyFocus::OrganicVariation);
        assert_eq!(actual.anchor, None);
    }
    assert_eq!(
        worlds.len(),
        maintained_count + ORGANIC_UNFILTERED_COUNT + ORGANIC_SEARCH_LIMIT
    );
    let selected = run_agency_probe(&registries, &worlds);
    assert_eq!(selected.len(), ORGANIC_QUALIFIED_TARGET);
    assert!(selected.iter().all(|seed| {
        worlds
            .iter()
            .skip(maintained_count + ORGANIC_UNFILTERED_COUNT)
            .any(|world| world.world_seed == *seed && world.anchor.is_none())
    }));
    assert_eq!(selected, run_agency_probe(&registries, &worlds));
    // Exhaustion is an evidence gap, not an assertion of production unavailability. Cutting the
    // deterministic stream right after the first qualified world must reproduce exactly that
    // seed; the second qualification lives past the cut and is honestly missing.
    let first_qualified_index = worlds
        .iter()
        .position(|world| world.world_seed == selected[0])
        .unwrap_or_else(|| {
            unreachable!("qualified agency seed must come from the searched worlds")
        });
    let exhausted = run_agency_probe(&registries, &worlds[..first_qualified_index + 1]);
    assert_eq!(exhausted, selected[..1]);
    assert!(exhausted.len() < ORGANIC_QUALIFIED_TARGET);
}

#[test]
fn gameplay_agency_counterfactuals() {
    run_gameplay_agency_counterfactuals();
}
