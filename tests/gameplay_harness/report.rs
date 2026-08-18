//! Gameplay-harness report records and concise human-readable aggregate output.

use deep_hearth::core::quantity::{Energy, Mass};
use deep_hearth::maintenance::MaintenanceBand;
use deep_hearth::registry::Registries;

use super::scenario::ScenarioVariation;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MaintenancePreference {
    ServiceAtWarning,
    ServiceAtCritical,
}

impl MaintenancePreference {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::ServiceAtWarning => "service-warning",
            Self::ServiceAtCritical => "service-critical",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StructuralPreference {
    PreserveMargin,
    MoveOnlyForFailure,
}

impl StructuralPreference {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::PreserveMargin => "preserve-margin",
            Self::MoveOnlyForFailure => "failure-only",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ScenarioPolicyVariation {
    pub(super) power_preference: PowerPreference,
    pub(super) maintenance_preference: MaintenancePreference,
    pub(super) structural_preference: StructuralPreference,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ScenarioReport {
    pub(super) world_seed: u64,
    pub(super) behavior_seed: u64,
    pub(super) policy: ScenarioPolicyVariation,
    pub(super) inputs: ScenarioInputReport,
    pub(super) structure: ScenarioStructureReport,
    pub(super) choices: ScenarioChoiceReport,
    pub(super) maintenance: ScenarioMaintenanceReport,
    pub(super) limits: ScenarioLimitReport,
    pub(super) progress: ScenarioProgressReport,
    pub(super) resources: ScenarioResourceReport,
}

impl ScenarioReport {
    pub(super) fn new(
        variation: ScenarioVariation,
        initial_maintenance_band: MaintenanceBand,
    ) -> Self {
        Self {
            world_seed: variation.world_seed,
            behavior_seed: variation.behavior_seed,
            policy: variation.policy,
            inputs: ScenarioInputReport {
                ore_copper_ppm: variation.ore.ore_copper_ppm,
                batch_mass: variation.ore.batch_mass,
                initial_condition_ppm: variation
                    .crusher
                    .initial_crusher_condition
                    .parts_per_million(),
                initial_maintenance_band,
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
            resources: ScenarioResourceReport::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ScenarioResourceReport {
    pub(super) final_condition_ppm: u32,
    pub(super) small_drive_remaining: Energy,
    pub(super) large_drive_remaining: Energy,
    pub(super) maintenance_stock_remaining: Mass,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ScenarioInputReport {
    pub(super) ore_copper_ppm: u32,
    pub(super) batch_mass: Mass,
    pub(super) initial_condition_ppm: u32,
    pub(super) initial_maintenance_band: MaintenanceBand,
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
    pub(super) small_drive_batches: u8,
    pub(super) large_drive_batches: u8,
    pub(super) large_drive_exhausted: bool,
    pub(super) delivery_deadline_power_choice: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ScenarioLimitReport {
    pub(super) energy_bottleneck_batches: u8,
    pub(super) throughput_bottleneck_batches: u8,
    pub(super) balanced_bottleneck_batches: u8,
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

pub(super) fn print_content_summary(registries: &Registries, include_catalog: bool) {
    let equipment_count = registries.equipment().definitions().count();
    let runtime_assemblable_equipment = registries
        .equipment()
        .definitions()
        .filter(|definition| definition.assembly_profile().is_some())
        .count();
    let equipment_upgrade_routes = registries
        .equipment()
        .definitions()
        .filter(|definition| definition.upgrade_profile().is_some())
        .count();
    let energy_count = registries.energy().definitions().count();
    let runtime_assemblable_energy = registries
        .energy()
        .definitions()
        .filter(|definition| definition.assembly_profile().is_some())
        .count();
    let process_count = registries.production().definitions().count();
    let manual_process_count = registries.crafting().definitions().count();
    let machine_process_count = process_count.saturating_sub(manual_process_count);
    let mining_method_count = registries.mining().definitions().count();
    std::println!(
        "CONTENT registry_schema={} equipment=[authored:{} runtime_assemblable:{} upgrade_routes:{}] energy=[authored:{} runtime_assemblable:{}] processes=[authored:{} manual:{} machine:{}] mining_methods={}",
        registries.schema_version().value(),
        equipment_count,
        runtime_assemblable_equipment,
        equipment_upgrade_routes,
        energy_count,
        runtime_assemblable_energy,
        process_count,
        manual_process_count,
        machine_process_count,
        mining_method_count,
    );
    if !include_catalog {
        return;
    }

    let equipment = registries
        .equipment()
        .definitions()
        .map(|definition| {
            let acquisition = match (
                definition.assembly_profile().is_some(),
                definition.upgrade_profile().is_some(),
            ) {
                (true, true) => "assemble+upgrade",
                (true, false) => "assemble",
                (false, true) => "upgrade-only",
                (false, false) => "no-runtime-acquisition",
            };
            format!(
                "{}:{}:{}",
                definition.id().value(),
                definition.name(),
                acquisition
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let energy = registries
        .energy()
        .definitions()
        .map(|definition| {
            let acquisition = if definition.assembly_profile().is_some() {
                "assemble"
            } else {
                "no-runtime-acquisition"
            };
            format!(
                "{}:{}:{}",
                definition.id().value(),
                definition.name(),
                acquisition
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let processes = registries
        .production()
        .definitions()
        .map(|definition| format!("{}:{}", definition.id().value(), definition.name()))
        .collect::<Vec<_>>()
        .join(",");
    std::println!(
        "CONTENT CATALOG equipment=[{equipment}] energy=[{energy}] processes=[{processes}]"
    );
}

pub(super) fn print_harness_summary(
    mode: &str,
    reports: &[ScenarioReport],
    capability_probes_executed: bool,
) {
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
    let mixed_ore_melt_rejections = reports
        .iter()
        .filter(|report| report.progress.ore_frontier_visible)
        .count();
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
    let initial_condition_min = reports
        .iter()
        .map(|report| report.inputs.initial_condition_ppm)
        .min()
        .unwrap_or_else(|| unreachable!("nonempty reports have an initial-condition minimum"));
    let initial_condition_max = reports
        .iter()
        .map(|report| report.inputs.initial_condition_ppm)
        .max()
        .unwrap_or_else(|| unreachable!("nonempty reports have an initial-condition maximum"));
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
    let initial_normal = reports
        .iter()
        .filter(|report| report.inputs.initial_maintenance_band == MaintenanceBand::Normal)
        .count();
    let initial_warning = reports
        .iter()
        .filter(|report| report.inputs.initial_maintenance_band == MaintenanceBand::Warning)
        .count();
    let initial_critical = reports
        .iter()
        .filter(|report| report.inputs.initial_maintenance_band == MaintenanceBand::Critical)
        .count();
    let small_drive_batches: u32 = reports
        .iter()
        .map(|report| u32::from(report.choices.small_drive_batches))
        .sum();
    let large_drive_batches: u32 = reports
        .iter()
        .map(|report| u32::from(report.choices.large_drive_batches))
        .sum();
    let energy_bottleneck_batches: u32 = reports
        .iter()
        .map(|report| u32::from(report.limits.energy_bottleneck_batches))
        .sum();
    let throughput_bottleneck_batches: u32 = reports
        .iter()
        .map(|report| u32::from(report.limits.throughput_bottleneck_batches))
        .sum();
    let balanced_bottleneck_batches: u32 = reports
        .iter()
        .map(|report| u32::from(report.limits.balanced_bottleneck_batches))
        .sum();

    for report in reports {
        let outcome = if report.structure.structural_stop {
            "structural-stop"
        } else if report.limits.maintenance_stop {
            "maintenance-stop"
        } else if report.limits.energy_stop {
            "energy-stop"
        } else if report.progress.completed_batches == report.progress.target_batches {
            "complete"
        } else {
            "partial"
        };
        std::println!(
            "EXPERIENCE world=0x{:016X} behavior=0x{:016X} start={:?} policy=[power:{} maintenance:{} structure:{}] batches={}/{} drive_batches=[small:{} large:{}] delivery_before={} maintenance={} relocation={} suspension={} stranded={} final=[condition:{}ppm small:{}nJ large:{}nJ maintenance:{}mg] outcome={}",
            report.world_seed,
            report.behavior_seed,
            report.inputs.initial_maintenance_band,
            report.policy.power_preference.label(),
            report.policy.maintenance_preference.label(),
            report.policy.structural_preference.label(),
            report.progress.completed_batches,
            report.progress.target_batches,
            report.choices.small_drive_batches,
            report.choices.large_drive_batches,
            report.progress.batches_before_delivery,
            report.maintenance.services,
            report.structure.support_relocation,
            report.structure.production_suspension,
            report.structure.stranded_work_in_process,
            report.resources.final_condition_ppm,
            report.resources.small_drive_remaining.nanojoules(),
            report.resources.large_drive_remaining.nanojoules(),
            report.resources.maintenance_stock_remaining.milligrams(),
            outcome,
        );
    }

    std::println!(
        "SAMPLE ore=[grade:{ore_grade_min}..{ore_grade_max}ppm batch:{batch_mass_min}..{batch_mass_max}mg] crusher_condition=[{initial_condition_min}..{initial_condition_max}ppm normal:{initial_normal} warning:{initial_warning} critical:{initial_critical}] delivery=[mass:{delivery_mass_min}..{delivery_mass_max}mg compact:{compact_deliveries} reinforced:{}]",
        reports.len() - compact_deliveries,
    );
    let capability_probe_status = if capability_probes_executed {
        "ore-prep+foundry:pass"
    } else {
        "not-run"
    };
    std::println!(
        "HARNESS PASS mode={mode} scenarios={} orders={completed_orders}/{} batches={completed_batches}/{target_batches} pre_delivery={batches_before_delivery} stops=[structural:{} maintenance:{} energy:{}] material=[mixed_ore_melt_rejected:{mixed_ore_melt_rejections}/{} capability_probes:{capability_probe_status}]",
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
        reports.len(),
    );
    std::println!(
        "SYSTEMS policy=[power:reserve:{} condition:{} speed:{} maintenance:warning:{} critical:{} structure:margin:{} failure_only:{}] work=[small_drive_batches:{small_drive_batches} high_power_batches:{large_drive_batches}] control=[compact_siting:{} delivery_deadline_power:{}] recovery=[relocations:{} resumed_wip:{recovered_work_in_process} stranded_wip:{} maintenance_services:{maintenance_services}] pressure=[structural:{} maintenance_warning:{}] bottlenecks=[energy_delivery_batches:{energy_bottleneck_batches} throughput_batches:{throughput_bottleneck_batches} balanced_batches:{balanced_bottleneck_batches}]",
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
            .filter(|report| {
                report.policy.maintenance_preference == MaintenancePreference::ServiceAtWarning
            })
            .count(),
        reports
            .iter()
            .filter(|report| {
                report.policy.maintenance_preference == MaintenancePreference::ServiceAtCritical
            })
            .count(),
        reports
            .iter()
            .filter(|report| {
                report.policy.structural_preference == StructuralPreference::PreserveMargin
            })
            .count(),
        reports
            .iter()
            .filter(|report| {
                report.policy.structural_preference == StructuralPreference::MoveOnlyForFailure
            })
            .count(),
        reports
            .iter()
            .filter(|report| report.choices.chose_compact_support)
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
    );
    if capability_probes_executed {
        std::println!(
            "SCOPE exercised=[survival-costed primitive crafting,mining,native-copper cold-working,in-place equipment reinforcement,material-backed primitive infrastructure construction,manual power,autonomous-machine+player-work overlap,canonical comminution,power choice,wear,maintenance-to-scrap,structural siting,supported-stockpile delivery,failure recovery,matched-world policy counterfactuals,ore-preparation-capability,pure-copper-foundry-capability] bootstrap=[raw starting matter,finite geological deposit,industrial workshop equipment,scenario stored energy,constructed bays,pure-copper probe input] deferred=[world resource generation/prospecting acquisition path,industrial construction authorization,mixed-ore concentration/smelting bridge,worn-equipment salvage,scrap recovery]"
        );
    } else {
        std::println!(
            "SCOPE exercised=[canonical comminution,power choice,wear,maintenance,structural siting,supported-stockpile delivery,failure recovery] separate_targets_not_run=[primitive-progression,ore-preparation-capability,pure-copper-foundry-capability] bootstrap=[industrial workshop equipment,scenario stored energy,constructed bays,starting workshop matter] deferred=[world resource generation/prospecting acquisition path,industrial construction authorization,mixed-ore concentration/smelting bridge,worn-equipment salvage]"
        );
    }
}
