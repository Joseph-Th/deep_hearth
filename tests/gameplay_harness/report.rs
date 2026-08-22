//! Gameplay-harness report records and concise human-readable aggregate output.

use deep_hearth::core::quantity::{Energy, Mass, Volume};
use deep_hearth::maintenance::MaintenanceBand;
use deep_hearth::registry::Registries;

use super::scenario::ScenarioVariation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PowerPreference {
    PreserveReserve,
    FinishSooner,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EnergyRecoveryPreference {
    ProtectSurvival,
    SpendSurvivalReserve,
}

impl EnergyRecoveryPreference {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::ProtectSurvival => "protect-survival",
            Self::SpendSurvivalReserve => "spend-survival-reserve",
        }
    }
}

impl PowerPreference {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::PreserveReserve => "preserve-reserve",
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
    pub(super) energy_recovery_preference: EnergyRecoveryPreference,
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
                nominal_batch_mass: variation.ore.nominal_batch_mass,
                order_mass: variation.ore.order_mass,
                start_at_hydration_warning: variation.survival.start_at_hydration_warning,
                initial_condition_ppm: variation
                    .crusher
                    .initial_crusher_condition
                    .parts_per_million(),
                initial_maintenance_band,
                small_drive_batch_budget: variation.crusher.small_drive_batch_budget,
                small_drive_partial_batch_ppm: variation.crusher.small_drive_partial_batch_ppm,
                large_drive_batch_budget: variation.crusher.large_drive_batch_budget,
                large_drive_partial_batch_ppm: variation.crusher.large_drive_partial_batch_ppm,
                maintenance_replacement_units: variation.crusher.maintenance_replacement_units,
                delivery_mass: variation.delivery.mass,
                delivery_is_compact: variation.delivery.destination_is_compact,
                delivery_at_tick: 0,
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
                operations_before_delivery: 0,
                ore_frontier_visible: false,
                processed_mass: Mass::ZERO,
                target_mass: variation.ore.order_mass,
                operations_completed: 0,
                adaptive_batch_operations: 0,
                condition_adaptive_batch_operations: 0,
                energy_adaptive_batch_operations: 0,
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
    pub(super) elapsed_ticks: u64,
    pub(super) metabolic_energy_spent: Energy,
    pub(super) hydration_spent: Volume,
    pub(super) final_vitality_ppm: u32,
    pub(super) manually_generated_energy: Energy,
    pub(super) manual_power_ticks: u64,
    pub(super) manual_power_metabolic_energy: Energy,
    pub(super) manual_power_hydration: Volume,
    pub(super) final_hand_crank_condition_ppm: u32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ScenarioInputReport {
    pub(super) ore_copper_ppm: u32,
    pub(super) nominal_batch_mass: Mass,
    pub(super) order_mass: Mass,
    pub(super) start_at_hydration_warning: bool,
    pub(super) initial_condition_ppm: u32,
    pub(super) initial_maintenance_band: MaintenanceBand,
    pub(super) small_drive_batch_budget: u8,
    pub(super) small_drive_partial_batch_ppm: u32,
    pub(super) large_drive_batch_budget: u8,
    pub(super) large_drive_partial_batch_ppm: u32,
    pub(super) maintenance_replacement_units: u8,
    pub(super) delivery_mass: Mass,
    pub(super) delivery_is_compact: bool,
    pub(super) delivery_at_tick: u64,
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
    pub(super) small_drive_batches: u16,
    pub(super) large_drive_batches: u16,
    pub(super) large_drive_exhausted: bool,
    pub(super) policy_power_choices: u16,
    pub(super) single_source_power_choices: u16,
    pub(super) manual_recharges: u16,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ScenarioLimitReport {
    pub(super) energy_bottleneck_batches: u16,
    pub(super) throughput_bottleneck_batches: u16,
    pub(super) balanced_bottleneck_batches: u16,
    pub(super) maintenance_warning: bool,
    pub(super) maintenance_stop: bool,
    pub(super) energy_stop: bool,
    pub(super) manual_recovery_declined: bool,
    pub(super) manual_recovery_survival_limited: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ScenarioProgressReport {
    pub(super) delivery_applied: bool,
    pub(super) operations_before_delivery: u16,
    pub(super) ore_frontier_visible: bool,
    pub(super) processed_mass: Mass,
    pub(super) target_mass: Mass,
    pub(super) operations_completed: u16,
    pub(super) adaptive_batch_operations: u16,
    pub(super) condition_adaptive_batch_operations: u16,
    pub(super) energy_adaptive_batch_operations: u16,
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
    let structurally_installed_equipment = registries
        .equipment()
        .definitions()
        .filter(|definition| definition.requires_structural_support())
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
    let food_count = registries.survival().foods().count();
    let drink_count = registries.survival().drinks().count();
    std::println!(
        "CONTENT registry_schema={} equipment=[authored:{} runtime_assemblable:{} upgrade_routes:{} structural_installation_required:{}] energy=[authored:{} runtime_assemblable:{}] processes=[authored:{} manual:{} machine:{}] mining_methods={} survival=[foods:{} drinks:{}]",
        registries.schema_version().value(),
        equipment_count,
        runtime_assemblable_equipment,
        equipment_upgrade_routes,
        structurally_installed_equipment,
        energy_count,
        runtime_assemblable_energy,
        process_count,
        manual_process_count,
        machine_process_count,
        mining_method_count,
        food_count,
        drink_count,
    );

    let acquisition_declared_equipment = registries
        .equipment()
        .definitions()
        .filter(|definition| {
            definition.assembly_profile().is_some() || definition.upgrade_profile().is_some()
        })
        .map(|definition| definition.name())
        .collect::<Vec<_>>()
        .join(",");
    let no_acquisition_equipment = registries
        .equipment()
        .definitions()
        .filter(|definition| {
            definition.assembly_profile().is_none() && definition.upgrade_profile().is_none()
        })
        .map(|definition| definition.name())
        .collect::<Vec<_>>()
        .join(",");
    let assembly_declared_energy = registries
        .energy()
        .definitions()
        .filter(|definition| definition.assembly_profile().is_some())
        .map(|definition| definition.name())
        .collect::<Vec<_>>()
        .join(",");
    let no_assembly_energy = registries
        .energy()
        .definitions()
        .filter(|definition| definition.assembly_profile().is_none())
        .map(|definition| definition.name())
        .collect::<Vec<_>>()
        .join(",");
    std::println!(
        "CONTENT ACQUISITION declared-equipment=[{acquisition_declared_equipment}] declared-energy=[{assembly_declared_energy}] no-runtime-path-equipment=[{no_acquisition_equipment}] no-runtime-path-energy=[{no_assembly_energy}] evidence-note=declaration-is-not-end-to-end-reachability missing-bridge=[runtime-industrial-acquisition,industrial-power-generation,mixed-ore-concentration/smelting]"
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
            let installation = if definition.requires_structural_support() {
                "fixed"
            } else {
                "portable"
            };
            format!(
                "{}:{}:{}:{}",
                definition.id().value(),
                definition.name(),
                acquisition,
                installation,
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

pub(super) fn print_harness_summary(mode: &str, reports: &[ScenarioReport]) {
    assert!(
        !reports.is_empty(),
        "gameplay harness produced no scenario reports"
    );

    let processed_mass_mg: u128 = reports
        .iter()
        .map(|report| u128::from(report.progress.processed_mass.milligrams()))
        .sum();
    let target_mass_mg: u128 = reports
        .iter()
        .map(|report| u128::from(report.progress.target_mass.milligrams()))
        .sum();
    let completed_operations: u32 = reports
        .iter()
        .map(|report| u32::from(report.progress.operations_completed))
        .sum();
    let operations_before_delivery: u32 = reports
        .iter()
        .map(|report| u32::from(report.progress.operations_before_delivery))
        .sum();
    let controlled_deliveries = reports
        .iter()
        .filter(|report| report.progress.delivery_applied)
        .count();
    let adaptive_operations: u32 = reports
        .iter()
        .map(|report| u32::from(report.progress.adaptive_batch_operations))
        .sum();
    let condition_adaptive_operations: u32 = reports
        .iter()
        .map(|report| u32::from(report.progress.condition_adaptive_batch_operations))
        .sum();
    let energy_adaptive_operations: u32 = reports
        .iter()
        .map(|report| u32::from(report.progress.energy_adaptive_batch_operations))
        .sum();
    let completed_orders = reports
        .iter()
        .filter(|report| report.progress.processed_mass == report.progress.target_mass)
        .count();
    let productive_orders = reports
        .iter()
        .filter(|report| !report.progress.processed_mass.is_zero())
        .count();
    let partial_orders = reports
        .iter()
        .filter(|report| {
            !report.progress.processed_mass.is_zero()
                && report.progress.processed_mass < report.progress.target_mass
        })
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
        .map(|report| report.inputs.nominal_batch_mass.milligrams())
        .min()
        .unwrap_or_else(|| unreachable!("nonempty reports have a batch-mass minimum"));
    let batch_mass_max = reports
        .iter()
        .map(|report| report.inputs.nominal_batch_mass.milligrams())
        .max()
        .unwrap_or_else(|| unreachable!("nonempty reports have a batch-mass maximum"));
    let order_mass_min = reports
        .iter()
        .map(|report| report.inputs.order_mass.milligrams())
        .min()
        .unwrap_or_else(|| unreachable!("nonempty reports have an order-mass minimum"));
    let order_mass_max = reports
        .iter()
        .map(|report| report.inputs.order_mass.milligrams())
        .max()
        .unwrap_or_else(|| unreachable!("nonempty reports have an order-mass maximum"));
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
    let delivery_tick_min = reports
        .iter()
        .map(|report| report.inputs.delivery_at_tick)
        .min()
        .unwrap_or_else(|| unreachable!("nonempty reports have a delivery-tick minimum"));
    let delivery_tick_max = reports
        .iter()
        .map(|report| report.inputs.delivery_at_tick)
        .max()
        .unwrap_or_else(|| unreachable!("nonempty reports have a delivery-tick maximum"));
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
    let survival_warning_starts = reports
        .iter()
        .filter(|report| report.inputs.start_at_hydration_warning)
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
    let policy_power_choices: u32 = reports
        .iter()
        .map(|report| u32::from(report.choices.policy_power_choices))
        .sum();
    let single_source_power_choices: u32 = reports
        .iter()
        .map(|report| u32::from(report.choices.single_source_power_choices))
        .sum();
    let manual_recharges: u32 = reports
        .iter()
        .map(|report| u32::from(report.choices.manual_recharges))
        .sum();
    let manually_generated_energy: u128 = reports
        .iter()
        .map(|report| report.resources.manually_generated_energy.nanojoules())
        .sum();
    let manual_power_ticks: u128 = reports
        .iter()
        .map(|report| u128::from(report.resources.manual_power_ticks))
        .sum();
    let manual_metabolic_energy: u128 = reports
        .iter()
        .map(|report| report.resources.manual_power_metabolic_energy.nanojoules())
        .sum();
    let manual_hydration: u128 = reports
        .iter()
        .map(|report| u128::from(report.resources.manual_power_hydration.microliters()))
        .sum();
    let elapsed_ticks_min = reports
        .iter()
        .map(|report| report.resources.elapsed_ticks)
        .min()
        .unwrap_or_else(|| unreachable!("nonempty reports have an elapsed-tick minimum"));
    let elapsed_ticks_max = reports
        .iter()
        .map(|report| report.resources.elapsed_ticks)
        .max()
        .unwrap_or_else(|| unreachable!("nonempty reports have an elapsed-tick maximum"));

    for report in reports {
        let outcome = if report.structure.structural_stop {
            "structural-stop"
        } else if report.limits.maintenance_stop {
            "maintenance-stop"
        } else if report.limits.energy_stop {
            "energy-stop"
        } else if report.progress.processed_mass == report.progress.target_mass {
            "complete"
        } else {
            "partial"
        };
        let survival_start = if report.inputs.start_at_hydration_warning {
            "hydration-warning"
        } else {
            "full-reserve"
        };
        let event_progress = if report.progress.delivery_applied {
            format!(
                "scheduled:{}t reached:true operations-before:{}",
                report.inputs.delivery_at_tick, report.progress.operations_before_delivery
            )
        } else {
            format!(
                "scheduled:{}t reached:false operations-before:n/a",
                report.inputs.delivery_at_tick
            )
        };
        std::println!(
            "CAPABILITY EXPERIENCE world=0x{:016X} behavior=0x{:016X} start=[crusher:{:?} survival:{}] policy=[power:{} recovery:{} maintenance:{} structure:{}] initial=[small:{}+{}ppm nominal-batches large:{}+{}ppm nominal-batches maintenance:{}unit] order=[processed:{}/{}mg operations:{} adaptive:[total:{} condition:{} stored-work:{}] event:[{}]] power=[small:{} large:{} manual-recharges:{} generated:{}nJ manual-ticks:{}] maintenance={} relocation={} suspension={} stranded={} final=[crusher:{}ppm crank:{}ppm small:{}nJ large:{}nJ maintenance:{}mg ticks:{} survival-energy:-{}nJ hydration:-{}uL vitality:{}ppm] outcome={}",
            report.world_seed,
            report.behavior_seed,
            report.inputs.initial_maintenance_band,
            survival_start,
            report.policy.power_preference.label(),
            report.policy.energy_recovery_preference.label(),
            report.policy.maintenance_preference.label(),
            report.policy.structural_preference.label(),
            report.inputs.small_drive_batch_budget,
            report.inputs.small_drive_partial_batch_ppm,
            report.inputs.large_drive_batch_budget,
            report.inputs.large_drive_partial_batch_ppm,
            report.inputs.maintenance_replacement_units,
            report.progress.processed_mass.milligrams(),
            report.progress.target_mass.milligrams(),
            report.progress.operations_completed,
            report.progress.adaptive_batch_operations,
            report.progress.condition_adaptive_batch_operations,
            report.progress.energy_adaptive_batch_operations,
            event_progress,
            report.choices.small_drive_batches,
            report.choices.large_drive_batches,
            report.choices.manual_recharges,
            report.resources.manually_generated_energy.nanojoules(),
            report.resources.manual_power_ticks,
            report.maintenance.services,
            report.structure.support_relocation,
            report.structure.production_suspension,
            report.structure.stranded_work_in_process,
            report.resources.final_condition_ppm,
            report.resources.final_hand_crank_condition_ppm,
            report.resources.small_drive_remaining.nanojoules(),
            report.resources.large_drive_remaining.nanojoules(),
            report.resources.maintenance_stock_remaining.milligrams(),
            report.resources.elapsed_ticks,
            report.resources.metabolic_energy_spent.nanojoules(),
            report.resources.hydration_spent.microliters(),
            report.resources.final_vitality_ppm,
            outcome,
        );
    }

    std::println!(
        "SAMPLE ore=[grade:{ore_grade_min}..{ore_grade_max}ppm nominal-batch:{batch_mass_min}..{batch_mass_max}mg order:{order_mass_min}..{order_mass_max}mg] crusher-condition=[{initial_condition_min}..{initial_condition_max}ppm normal:{initial_normal} warning:{initial_warning} critical:{initial_critical}] survival-start=[hydration-warning:{survival_warning_starts} full-reserve:{}] resources=[small-drive:{}..{}+{}..{}ppm nominal-batches large-drive:{}..{}+{}..{}ppm maintenance-units:{}..{}] scheduled-event=[tick:{delivery_tick_min}..{delivery_tick_max}t mass:{delivery_mass_min}..{delivery_mass_max}mg compact:{compact_deliveries} reinforced:{} actor-visibility:hidden]",
        reports.len() - survival_warning_starts,
        reports
            .iter()
            .map(|report| report.inputs.small_drive_batch_budget)
            .min()
            .unwrap_or(0),
        reports
            .iter()
            .map(|report| report.inputs.small_drive_batch_budget)
            .max()
            .unwrap_or(0),
        reports
            .iter()
            .map(|report| report.inputs.small_drive_partial_batch_ppm)
            .min()
            .unwrap_or(0),
        reports
            .iter()
            .map(|report| report.inputs.small_drive_partial_batch_ppm)
            .max()
            .unwrap_or(0),
        reports
            .iter()
            .map(|report| report.inputs.large_drive_batch_budget)
            .min()
            .unwrap_or(0),
        reports
            .iter()
            .map(|report| report.inputs.large_drive_batch_budget)
            .max()
            .unwrap_or(0),
        reports
            .iter()
            .map(|report| report.inputs.large_drive_partial_batch_ppm)
            .min()
            .unwrap_or(0),
        reports
            .iter()
            .map(|report| report.inputs.large_drive_partial_batch_ppm)
            .max()
            .unwrap_or(0),
        reports
            .iter()
            .map(|report| report.inputs.maintenance_replacement_units)
            .min()
            .unwrap_or(0),
        reports
            .iter()
            .map(|report| report.inputs.maintenance_replacement_units)
            .max()
            .unwrap_or(0),
        reports.len() - compact_deliveries,
    );
    std::println!(
        "WORKSHOP CAPABILITY mode={mode} scenarios={} orders=[complete:{completed_orders} partial:{partial_orders} productive:{productive_orders}/{}] ore={processed_mass_mg}/{target_mass_mg}mg operations={completed_operations} adaptive=[total:{adaptive_operations} condition:{condition_adaptive_operations} stored-work:{energy_adaptive_operations}] events=[reached:{controlled_deliveries}/{} operations-before-reached:{operations_before_delivery}] stops=[structural:{} maintenance:{} energy:{} declined-manual:{} survival-limited-manual:{}] manual-recovery=[charges:{manual_recharges} generated:{manually_generated_energy}nJ ticks:{manual_power_ticks} metabolic:{manual_metabolic_energy}nJ hydration:{manual_hydration}uL] material=[mixed-ore-melt-rejected:{mixed_ore_melt_rejections}/{}]",
        reports.len(),
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
        reports
            .iter()
            .filter(|report| report.limits.manual_recovery_declined)
            .count(),
        reports
            .iter()
            .filter(|report| report.limits.manual_recovery_survival_limited)
            .count(),
        reports.len(),
    );
    let relocations = reports
        .iter()
        .filter(|report| report.structure.support_relocation)
        .count();
    let suspensions = reports
        .iter()
        .filter(|report| report.structure.production_suspension)
        .count();
    let stranded_work_in_process = reports
        .iter()
        .filter(|report| report.structure.stranded_work_in_process)
        .count();
    let structural_consequences = reports
        .iter()
        .filter(|report| report.structure.structural_consequence)
        .count();
    std::println!(
        "CAPABILITY SYSTEMS policy=[power:reserve:{} speed:{} recovery:protect:{} spend:{} maintenance:warning:{} critical:{} structure:margin:{} failure-only:{}] machine-work=[small-drive:{small_drive_batches} high-power:{large_drive_batches}] decisions=[power-policy:{policy_power_choices} single-source:{single_source_power_choices} manual-recharges:{manual_recharges} adaptive:[total:{adaptive_operations} condition:{condition_adaptive_operations} stored-work:{energy_adaptive_operations}]] survival=[elapsed:{elapsed_ticks_min}..{elapsed_ticks_max}t manual-metabolic:{manual_metabolic_energy}nJ manual-hydration:{manual_hydration}uL] recovery=[relocations:{} resumed-wip:{recovered_work_in_process} stranded-wip:{} maintenance-services:{maintenance_services}] pressure=[structural:{} maintenance-warning:{}] bottlenecks=[energy-delivery:{energy_bottleneck_batches} throughput:{throughput_bottleneck_batches} balanced:{balanced_bottleneck_batches}]",
        reports
            .iter()
            .filter(|report| report.policy.power_preference == PowerPreference::PreserveReserve)
            .count(),
        reports
            .iter()
            .filter(|report| report.policy.power_preference == PowerPreference::FinishSooner)
            .count(),
        reports
            .iter()
            .filter(|report| {
                report.policy.energy_recovery_preference
                    == EnergyRecoveryPreference::ProtectSurvival
            })
            .count(),
        reports
            .iter()
            .filter(|report| {
                report.policy.energy_recovery_preference
                    == EnergyRecoveryPreference::SpendSurvivalReserve
            })
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
        relocations,
        stranded_work_in_process,
        structural_consequences,
        reports
            .iter()
            .filter(|report| report.limits.maintenance_warning)
            .count(),
    );
    let mut observed = Vec::new();
    let mut unobserved = Vec::new();
    if structural_consequences > 0 {
        observed.push("structural-consequence");
    } else {
        unobserved.push("structural-consequence");
    }
    if relocations > 0 {
        observed.push("support-relocation");
    } else {
        unobserved.push("support-relocation");
    }
    if suspensions > 0 {
        observed.push("production-suspension");
    } else {
        unobserved.push("production-suspension");
    }
    if recovered_work_in_process > 0 {
        observed.push("wip-recovery");
    } else {
        unobserved.push("wip-recovery");
    }
    if stranded_work_in_process > 0 {
        observed.push("stranded-wip");
    } else {
        unobserved.push("stranded-wip");
    }
    if manual_recharges > 0 {
        observed.push("manual-energy-recovery");
    } else {
        unobserved.push("manual-energy-recovery");
    }
    if adaptive_operations > 0 {
        observed.push("adaptive-batching");
    } else {
        unobserved.push("adaptive-batching");
    }
    if controlled_deliveries > 0 {
        observed.push("controlled-supported-stockpile-delivery");
    } else {
        unobserved.push("controlled-supported-stockpile-delivery");
    }
    let observed = observed.join(",");
    let unobserved = unobserved.join(",");
    std::println!(
        "CAPABILITY SCOPE evidence=bootstrapped-industrial surface=[canonical-industrial-comminution,adaptive-batching,manual-energy-recovery,power-choice,wear,maintenance,structural-siting,controlled-supported-stockpile-delivery] observed=[{observed}] unobserved=[{unobserved}] outside-this-workshop-test=[playable-survival,playable-primitive-progression,industrial-ore-preparation,pure-copper-foundry] bootstrap=[industrial-workshop-equipment,industrial-energy-stores,constructed-bays,starting-workshop-matter,preauthorized-controlled-delivery] missing-bridge=[industrial-acquisition,industrial-power-generation,mixed-ore-concentration/smelting] actor-oracle=none fixture-guard=fail-if-injected-machine-becomes-runtime-acquirable capability-boundary=STATUS.md"
    );

    let stored_work_pressure = reports
        .iter()
        .filter(|report| {
            report.progress.energy_adaptive_batch_operations > 0
                || report.limits.energy_bottleneck_batches > 0
                || report.limits.energy_stop
        })
        .count();
    let body_power_pressure = reports
        .iter()
        .filter(|report| {
            report.choices.manual_recharges > 0
                || report.limits.manual_recovery_declined
                || report.limits.manual_recovery_survival_limited
        })
        .count();
    let wear_maintenance_pressure = reports
        .iter()
        .filter(|report| report.limits.maintenance_warning || report.maintenance.services > 0)
        .count();
    let structure_production_pressure = reports
        .iter()
        .filter(|report| {
            report.structure.structural_consequence
                || report.structure.support_relocation
                || report.structure.production_suspension
        })
        .count();
    let multi_system_adaptation = reports
        .iter()
        .filter(|report| {
            let dimensions = u8::from(
                report.progress.energy_adaptive_batch_operations > 0
                    || report.limits.energy_bottleneck_batches > 0
                    || report.limits.energy_stop,
            ) + u8::from(
                report.choices.manual_recharges > 0
                    || report.limits.manual_recovery_declined
                    || report.limits.manual_recovery_survival_limited,
            ) + u8::from(
                report.limits.maintenance_warning || report.maintenance.services > 0,
            ) + u8::from(
                report.structure.structural_consequence
                    || report.structure.support_relocation
                    || report.structure.production_suspension,
            );
            dimensions >= 2
        })
        .count();
    std::println!(
        "WORKSHOP EXPERIENCE REVIEW fantasy=operate+adapt-physical-infrastructure sample=pressure-rich+hidden-controlled-delivery reached-events:{controlled_deliveries}/{} loop=observe-pressure->choose-power/batch/service/site->run->recover dynamic-scenarios:{multi_system_adaptation}/{} interlocks=[stored-work+throughput:{stored_work_pressure} body+power:{body_power_pressure} wear+maintenance:{wear_maintenance_pressure} structure+production:{structure_production_pressure}] recovery=[suspensions:{suspensions} resumed:{recovered_work_in_process} stranded:{stranded_work_in_process}] agency=matched-policy-counterfactuals-in-AGENCY-SUMMARY dormant=[ore-grade:composition-only-until-concentration/smelting]",
        reports.len(),
        reports.len(),
    );
}
