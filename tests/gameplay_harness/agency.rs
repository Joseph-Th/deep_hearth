//! Matched-world workshop policy counterfactuals and agency evidence.

use std::collections::BTreeSet;
use std::env;

use super::configuration::MaintainedAnchor;
use super::fresh_seed::fresh_root;
use super::output::has_verbose_output;
use super::report::{
    EnergyRecoveryPreference, MaintenancePreference, PowerPreference, ScenarioPolicyVariation,
    ScenarioReport, StructuralPreference,
};
use super::scenario::ScenarioVariation;
use super::seed::{MAINTAINED_VARIATION_ROOT, mix64};
use super::seed_input::parse_seed;
use super::workshop::run_scenario;
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
    OrganicVariation,
}

impl AgencyFocus {
    const fn label(self) -> &'static str {
        match self {
            Self::PowerAndStructure => "power+structure",
            Self::SurvivalRecovery => "survival-recovery",
            Self::MaintenanceTiming => "maintenance-timing",
            Self::OrganicVariation => "organic-variation",
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

fn run_agency_probe(registries: &Registries, worlds: &[AgencyWorld]) {
    let policies = agency_probe_policies();
    let mut worlds_with_distinct_paths = 0_usize;
    let mut worlds_with_work_difference = 0_usize;
    let mut observed_power_effect = false;
    let mut observed_survival_effect = false;
    let mut observed_maintenance_effect = false;
    let mut observed_structure_effect = false;
    let mut organic_worlds = 0_usize;
    let mut organic_actionable_worlds = 0_usize;
    let mut organic_objective_resolved_worlds = 0_usize;
    let mut organic_terminal_worlds = 0_usize;
    let mut organic_dormant_worlds = 0_usize;
    for world in worlds {
        let focus = world.focus.label();
        let world_seed = world.world_seed;
        let behavior_seed = mix64(world_seed ^ 0xA63E_4E43_5900_0001);
        let mut preliminary_reports = Vec::with_capacity(policies.len());
        for (variant, policy) in policies {
            let mut variation =
                ScenarioVariation::from_seeds(registries, world_seed, behavior_seed, world.anchor);
            variation.policy = policy;
            let report = run_scenario(registries, variation, None);
            assert_eq!(
                report.world_seed, world_seed,
                "agency counterfactual must preserve the matched world seed"
            );
            assert_eq!(
                report.behavior_seed, behavior_seed,
                "agency counterfactual must preserve the matched behavior seed"
            );
            preliminary_reports.push((variant, report));
        }
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

        let mut reports = Vec::with_capacity(policies.len());
        for (variant, policy) in policies {
            let mut variation =
                ScenarioVariation::from_seeds(registries, world_seed, behavior_seed, world.anchor);
            variation.policy = policy;
            let report = run_scenario(registries, variation, Some(comparison_horizon));
            assert_eq!(
                report.inputs, matched_inputs,
                "agency counterfactual rerun must preserve the matched physical setup"
            );
            assert_eq!(
                report.resources.elapsed_ticks, comparison_horizon,
                "agency counterfactual branches must use one policy-independent observation horizon"
            );
            reports.push((variant, report));
        }
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
        let episode_end_min = reports
            .iter()
            .map(|(_, report)| report.resources.episode_end_tick)
            .min()
            .unwrap_or_else(|| unreachable!("agency probe policy set is nonempty"));
        let episode_end_max = reports
            .iter()
            .map(|(_, report)| report.resources.episode_end_tick)
            .max()
            .unwrap_or_else(|| unreachable!("agency probe policy set is nonempty"));
        let processed_max = reports
            .iter()
            .map(|(_, report)| report.progress.processed_mass.milligrams())
            .max()
            .unwrap_or_else(|| unreachable!("agency probe policy set is nonempty"));
        let high_power_min = reports
            .iter()
            .map(|(_, report)| report.choices.large_drive_batches)
            .min()
            .unwrap_or_else(|| unreachable!("agency probe policy set is nonempty"));
        let high_power_max = reports
            .iter()
            .map(|(_, report)| report.choices.large_drive_batches)
            .max()
            .unwrap_or_else(|| unreachable!("agency probe policy set is nonempty"));
        let service_min = reports
            .iter()
            .map(|(_, report)| report.maintenance.services)
            .min()
            .unwrap_or_else(|| unreachable!("agency probe policy set is nonempty"));
        let service_max = reports
            .iter()
            .map(|(_, report)| report.maintenance.services)
            .max()
            .unwrap_or_else(|| unreachable!("agency probe policy set is nonempty"));
        let condition_min = reports
            .iter()
            .map(|(_, report)| report.resources.final_condition_ppm)
            .min()
            .unwrap_or_else(|| unreachable!("agency probe policy set is nonempty"));
        let condition_max = reports
            .iter()
            .map(|(_, report)| report.resources.final_condition_ppm)
            .max()
            .unwrap_or_else(|| unreachable!("agency probe policy set is nonempty"));
        let manual_min = reports
            .iter()
            .map(|(_, report)| report.choices.manual_recharges)
            .min()
            .unwrap_or_else(|| unreachable!("agency probe policy set is nonempty"));
        let manual_max = reports
            .iter()
            .map(|(_, report)| report.choices.manual_recharges)
            .max()
            .unwrap_or_else(|| unreachable!("agency probe policy set is nonempty"));
        let adaptive_min = reports
            .iter()
            .map(|(_, report)| report.progress.adaptive_batch_operations)
            .min()
            .unwrap_or_else(|| unreachable!("agency probe policy set is nonempty"));
        let adaptive_max = reports
            .iter()
            .map(|(_, report)| report.progress.adaptive_batch_operations)
            .max()
            .unwrap_or_else(|| unreachable!("agency probe policy set is nonempty"));
        let relocations = reports
            .iter()
            .filter(|(_, report)| report.structure.support_relocation)
            .count();
        let suspensions = reports
            .iter()
            .filter(|(_, report)| report.structure.production_suspension)
            .count();
        let elapsed_min = reports
            .iter()
            .map(|(_, report)| report.resources.elapsed_ticks)
            .min()
            .unwrap_or_else(|| unreachable!("agency probe policy set is nonempty"));
        let elapsed_max = reports
            .iter()
            .map(|(_, report)| report.resources.elapsed_ticks)
            .max()
            .unwrap_or_else(|| unreachable!("agency probe policy set is nonempty"));
        assert_eq!(elapsed_min, comparison_horizon);
        assert_eq!(elapsed_max, comparison_horizon);
        let survival_energy_min = reports
            .iter()
            .map(|(_, report)| report.resources.metabolic_energy_spent.nanojoules())
            .min()
            .unwrap_or_else(|| unreachable!("agency probe policy set is nonempty"));
        let survival_energy_max = reports
            .iter()
            .map(|(_, report)| report.resources.metabolic_energy_spent.nanojoules())
            .max()
            .unwrap_or_else(|| unreachable!("agency probe policy set is nonempty"));
        let signatures = reports
            .iter()
            .map(|(_, report)| AgencyPathSignature::from_report(report))
            .collect::<BTreeSet<_>>();
        if signatures.len() > 1 {
            worlds_with_distinct_paths += 1;
        }
        if processed_min != processed_max {
            worlds_with_work_difference += 1;
        }
        let baseline = agency_report(&reports, AgencyPolicyVariant::Baseline);
        let finish_sooner = agency_report(&reports, AgencyPolicyVariant::FinishSooner);
        let spend_survival = agency_report(&reports, AgencyPolicyVariant::SpendSurvival);
        let delay_maintenance = agency_report(&reports, AgencyPolicyVariant::DelayMaintenance);
        let failure_only_structure =
            agency_report(&reports, AgencyPolicyVariant::FailureOnlyStructure);
        let power_effect = power_counterfactual_changed(baseline, finish_sooner);
        let survival_effect = survival_counterfactual_changed(baseline, spend_survival);
        let maintenance_effect = maintenance_counterfactual_changed(baseline, delay_maintenance);
        let structure_effect = structure_counterfactual_changed(baseline, failure_only_structure);
        observed_power_effect |= power_effect;
        observed_survival_effect |= survival_effect;
        observed_maintenance_effect |= maintenance_effect;
        observed_structure_effect |= structure_effect;
        let actionable = power_effect || survival_effect || maintenance_effect || structure_effect;
        match world.focus {
            AgencyFocus::PowerAndStructure => {
                assert!(
                    power_effect && structure_effect,
                    "maintained power+structure agency world must make both one-factor choices consequential"
                );
            }
            AgencyFocus::SurvivalRecovery => {
                assert!(
                    survival_effect
                        && baseline.limits.manual_recovery_declined
                        && spend_survival.progress.processed_mass
                            > baseline.progress.processed_mass,
                    "maintained survival-recovery agency world must trade protected reserves against additional useful work"
                );
            }
            AgencyFocus::MaintenanceTiming => {
                assert!(
                    maintenance_effect,
                    "maintained maintenance-timing agency world must make preventive versus delayed service consequential"
                );
            }
            AgencyFocus::OrganicVariation => {
                organic_worlds += 1;
            }
        }
        let evidence = classify_agency_evidence(&reports, actionable);
        if world.focus == AgencyFocus::OrganicVariation {
            match evidence {
                AgencyEvidence::Actionable => organic_actionable_worlds += 1,
                AgencyEvidence::ObjectiveResolved => organic_objective_resolved_worlds += 1,
                AgencyEvidence::StructuralCapacity
                | AgencyEvidence::MaintenanceSupply
                | AgencyEvidence::MaintenanceSafety
                | AgencyEvidence::ManualRecoveryDeclined
                | AgencyEvidence::ManualRecoverySurvivalLimited
                | AgencyEvidence::StoredWorkInsufficient => organic_terminal_worlds += 1,
                AgencyEvidence::DormantPolicyPressure => organic_dormant_worlds += 1,
            }
        }
        let evidence = evidence.label();
        if has_verbose_output() {
            std::println!(
                "AGENCY focus={focus} world=0x{world_seed:016X} variants={} physical-paths={} evidence={evidence} horizon={}t actionable=[power:{} survival:{} maintenance:{} structure:{}] policy-effects=[processed:{}..{}mg adaptive:{}..{} high-power:{}..{} manual-recharges:{}..{} services:{}..{} final-condition:{}..{}ppm relocations:{}/{} suspensions:{}/{} episode-end:{}..{}t survival-energy:{}..{}nJ]",
                reports.len(),
                signatures.len(),
                comparison_horizon,
                power_effect,
                survival_effect,
                maintenance_effect,
                structure_effect,
                processed_min,
                processed_max,
                adaptive_min,
                adaptive_max,
                high_power_min,
                high_power_max,
                manual_min,
                manual_max,
                service_min,
                service_max,
                condition_min,
                condition_max,
                relocations,
                reports.len(),
                suspensions,
                reports.len(),
                episode_end_min,
                episode_end_max,
                survival_energy_min,
                survival_energy_max,
            );
            let policy_paths = reports
                .iter()
                .map(|(variant, report)| {
                    let label = variant.label();
                    format!(
                        "{label}:ore{}/{}-ops{}-adapt{}-hi{}-manual{}-maint{}-reloc{}-susp{}-choices[p:{} f:{}]-episode{}-horizon{}-body{}-manualbody{}-c{}-lo{}-hi{}",
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
                "AGENCY PATHS focus={focus} world=0x{world_seed:016X} paths=[{policy_paths}]"
            );
        }
    }
    assert_eq!(
        organic_actionable_worlds
            + organic_objective_resolved_worlds
            + organic_terminal_worlds
            + organic_dormant_worlds,
        organic_worlds,
        "organic agency evidence classes must partition sampled worlds"
    );
    std::println!(
        "AGENCY SUMMARY worlds={} distinct-physical-paths={} processed-work-differences={} demonstrated-choice-effects=[power:{} survival:{} maintenance:{} structure:{}] organic=[actionable:{}/{} objective-resolved:{} terminal-constraint:{} dormant-policy-pressure:{}] basis=matched-world-one-factor-counterfactual+shared-observation-horizon+reason-specific-absence-classification",
        worlds.len(),
        worlds_with_distinct_paths,
        worlds_with_work_difference,
        observed_power_effect,
        observed_survival_effect,
        observed_maintenance_effect,
        observed_structure_effect,
        organic_actionable_worlds,
        organic_worlds,
        organic_objective_resolved_worlds,
        organic_terminal_worlds,
        organic_dormant_worlds,
    );
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

fn replayable_agency_root() -> u64 {
    env::var("DEEP_HEARTH_GAMEPLAY_VARIATION_SEED")
        .ok()
        .map(|raw| {
            parse_seed(&raw)
                .unwrap_or_else(|| panic!("agency gameplay variation seed is invalid: {raw:?}"))
        })
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
            world_seed: 4,
            anchor: Some(MaintainedAnchor::WarningMaintenance),
        },
    ]
}

pub(super) fn run_gameplay_agency_counterfactuals() {
    let registries = build_registries();
    let variation_root = replayable_agency_root();
    std::println!("AGENCY INPUT mode=gate organic=1 variation_root=0x{variation_root:016X}");
    let mut worlds = maintained_agency_worlds();
    worlds.extend(organic_agency_worlds(variation_root, 1));
    run_agency_probe(&registries, &worlds);
}

pub(super) fn run_exploratory_agency_counterfactuals() {
    let registries = build_registries();
    let variation_root = replayable_agency_root();
    std::println!("AGENCY INPUT mode=explore organic=3 variation_root=0x{variation_root:016X}");
    let organic = organic_agency_worlds(variation_root, 3);
    let mut worlds = maintained_agency_worlds();
    worlds.extend(organic);
    run_agency_probe(&registries, &worlds);
}

#[test]
fn gameplay_agency_counterfactuals() {
    run_gameplay_agency_counterfactuals();
}
