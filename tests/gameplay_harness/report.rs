//! Gameplay-harness report records and concise human-readable aggregate output.

use deep_hearth::core::quantity::Mass;

use super::ScenarioVariation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PowerPreference {
    PreserveReserve,
    ProtectCondition,
    FinishSooner,
}

impl PowerPreference {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::PreserveReserve => "preserve-reserve",
            Self::ProtectCondition => "protect-condition",
            Self::FinishSooner => "finish-sooner",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ScenarioPolicyVariation {
    pub(super) power_preference: PowerPreference,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ScenarioReport {
    pub(super) seed: u64,
    pub(super) policy: ScenarioPolicyVariation,
    pub(super) inputs: ScenarioInputReport,
    pub(super) structure: ScenarioStructureReport,
    pub(super) choices: ScenarioChoiceReport,
    pub(super) maintenance: ScenarioMaintenanceReport,
    pub(super) limits: ScenarioLimitReport,
    pub(super) progress: ScenarioProgressReport,
}

impl ScenarioReport {
    pub(super) fn new(variation: ScenarioVariation) -> Self {
        Self {
            seed: variation.seed,
            policy: variation.policy,
            inputs: ScenarioInputReport {
                ore_copper_ppm: variation.ore.ore_copper_ppm,
                batch_mass: variation.ore.batch_mass,
                delivery_mass: variation.delivery.mass,
                delivery_is_compact: variation.delivery.destination_is_compact,
            },
            structure: ScenarioStructureReport::default(),
            choices: ScenarioChoiceReport::default(),
            maintenance: ScenarioMaintenanceReport {
                services: 0,
                replacement_spent: Mass::ZERO,
                supply_exhausted: false,
            },
            limits: ScenarioLimitReport::default(),
            progress: ScenarioProgressReport {
                delivery_applied: false,
                batches_before_delivery: 0,
                ore_frontier_visible: false,
                completed_batches: 0,
                target_batches: variation.ore.planned_batches,
            },
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ScenarioInputReport {
    pub(super) ore_copper_ppm: u32,
    pub(super) batch_mass: Mass,
    pub(super) delivery_mass: Mass,
    pub(super) delivery_is_compact: bool,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ScenarioMaintenanceReport {
    pub(super) services: u8,
    pub(super) replacement_spent: Mass,
    pub(super) supply_exhausted: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ScenarioStructureReport {
    pub(super) structural_consequence: bool,
    pub(super) structural_damage_debt: bool,
    pub(super) support_failure_blocked_production: bool,
    pub(super) support_relocation: bool,
    pub(super) structural_stop: bool,
    pub(super) production_suspension: bool,
    pub(super) stranded_work_in_process: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ScenarioChoiceReport {
    pub(super) chose_compact_support: bool,
    pub(super) delivery_plan_changed_siting: bool,
    pub(super) used_small_drive: bool,
    pub(super) used_large_drive: bool,
    pub(super) large_drive_exhausted: bool,
    pub(super) delivery_deadline_power_choice: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ScenarioLimitReport {
    pub(super) energy_bottleneck: bool,
    pub(super) throughput_bottleneck: bool,
    pub(super) maintenance_warning: bool,
    pub(super) maintenance_stop: bool,
    pub(super) energy_stop: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ScenarioProgressReport {
    pub(super) delivery_applied: bool,
    pub(super) batches_before_delivery: u8,
    pub(super) ore_frontier_visible: bool,
    pub(super) completed_batches: u8,
    pub(super) target_batches: u8,
}

pub(super) fn print_harness_summary(mode: &str, reports: &[ScenarioReport]) {
    assert!(
        !reports.is_empty(),
        "gameplay harness produced no scenario reports"
    );

    let completed_batches: u32 = reports
        .iter()
        .map(|report| u32::from(report.progress.completed_batches))
        .sum();
    let target_batches: u32 = reports
        .iter()
        .map(|report| u32::from(report.progress.target_batches))
        .sum();
    let batches_before_delivery: u32 = reports
        .iter()
        .map(|report| u32::from(report.progress.batches_before_delivery))
        .sum();
    let completed_orders = reports
        .iter()
        .filter(|report| report.progress.completed_batches == report.progress.target_batches)
        .count();
    let recovered_work_in_process = reports
        .iter()
        .filter(|report| {
            report.structure.production_suspension && !report.structure.stranded_work_in_process
        })
        .count();
    let maintenance_services: u32 = reports
        .iter()
        .map(|report| u32::from(report.maintenance.services))
        .sum();
    let ore_grade_min = reports
        .iter()
        .map(|report| report.inputs.ore_copper_ppm)
        .min()
        .unwrap_or_else(|| unreachable!("nonempty reports have an ore-grade minimum"));
    let ore_grade_max = reports
        .iter()
        .map(|report| report.inputs.ore_copper_ppm)
        .max()
        .unwrap_or_else(|| unreachable!("nonempty reports have an ore-grade maximum"));
    let batch_mass_min = reports
        .iter()
        .map(|report| report.inputs.batch_mass.milligrams())
        .min()
        .unwrap_or_else(|| unreachable!("nonempty reports have a batch-mass minimum"));
    let batch_mass_max = reports
        .iter()
        .map(|report| report.inputs.batch_mass.milligrams())
        .max()
        .unwrap_or_else(|| unreachable!("nonempty reports have a batch-mass maximum"));
    let delivery_mass_min = reports
        .iter()
        .map(|report| report.inputs.delivery_mass.milligrams())
        .min()
        .unwrap_or_else(|| unreachable!("nonempty reports have a delivery-mass minimum"));
    let delivery_mass_max = reports
        .iter()
        .map(|report| report.inputs.delivery_mass.milligrams())
        .max()
        .unwrap_or_else(|| unreachable!("nonempty reports have a delivery-mass maximum"));
    let compact_deliveries = reports
        .iter()
        .filter(|report| report.inputs.delivery_is_compact)
        .count();

    std::println!(
        "SAMPLE ore=[grade:{ore_grade_min}..{ore_grade_max}ppm batch:{batch_mass_min}..{batch_mass_max}mg] delivery=[mass:{delivery_mass_min}..{delivery_mass_max}mg compact:{compact_deliveries} reinforced:{}]",
        reports.len() - compact_deliveries,
    );
    std::println!(
        "HARNESS PASS mode={mode} scenarios={} orders={completed_orders}/{} batches={completed_batches}/{target_batches} pre_delivery={batches_before_delivery} stops=[structural:{} maintenance:{} energy:{}] material=[ore_prep:pass foundry:pass mixed_ore_bridge:blocked]",
        reports.len(),
        reports.len(),
        reports
            .iter()
            .filter(|report| report.structure.structural_stop)
            .count(),
        reports
            .iter()
            .filter(|report| report.limits.maintenance_stop)
            .count(),
        reports
            .iter()
            .filter(|report| report.limits.energy_stop)
            .count(),
    );
    std::println!(
        "SYSTEMS policy=[reserve:{} condition:{} speed:{}] control=[delivery_siting:{} delivery_deadline_power:{}] recovery=[relocations:{} resumed_wip:{recovered_work_in_process} stranded_wip:{} maintenance_services:{maintenance_services}] pressure=[structural:{} maintenance_warning:{}] bottlenecks=[energy_delivery:{} throughput:{}]",
        reports
            .iter()
            .filter(|report| report.policy.power_preference == PowerPreference::PreserveReserve)
            .count(),
        reports
            .iter()
            .filter(|report| report.policy.power_preference == PowerPreference::ProtectCondition)
            .count(),
        reports
            .iter()
            .filter(|report| report.policy.power_preference == PowerPreference::FinishSooner)
            .count(),
        reports
            .iter()
            .filter(|report| report.choices.delivery_plan_changed_siting)
            .count(),
        reports
            .iter()
            .filter(|report| report.choices.delivery_deadline_power_choice)
            .count(),
        reports
            .iter()
            .filter(|report| report.structure.support_relocation)
            .count(),
        reports
            .iter()
            .filter(|report| report.structure.stranded_work_in_process)
            .count(),
        reports
            .iter()
            .filter(|report| report.structure.structural_consequence)
            .count(),
        reports
            .iter()
            .filter(|report| report.limits.maintenance_warning)
            .count(),
        reports
            .iter()
            .filter(|report| report.limits.energy_bottleneck)
            .count(),
        reports
            .iter()
            .filter(|report| report.limits.throughput_bottleneck)
            .count(),
    );
    std::println!(
        "SCOPE exercised=[canonical comminution,power choice,wear,maintenance,structural siting,supported-stockpile delivery,failure recovery] bootstrap=[starting matter,stored energy,equipment,constructed bays] deferred=[resource acquisition,energy generation,construction authorization,concentration/smelting bridge]"
    );
}
