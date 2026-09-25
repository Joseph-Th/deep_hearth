//! Gameplay-harness report records and concise human-readable aggregate output.

use deep_hearth::core::quantity::{Energy, Mass, Volume};
use deep_hearth::maintenance::MaintenanceBand;

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
            Self::ServiceAtWarning => "service-warning-demand-aware",
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
                gangue_clay_share_ppm: variation.ore.gangue_clay_share_ppm,
                nominal_batch_mass: variation.ore.nominal_batch_mass,
                order_mass: variation.ore.order_mass,
                start_at_hydration_warning_boundary: variation
                    .survival
                    .start_at_hydration_warning_boundary,
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
                warning_deferrals: 0,
                critical_services: 0,
                service_ticks: 0,
                replacement_spent: Mass::ZERO,
                supply_exhausted: false,
                labor_unavailable: false,
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
    /// Tick when the actor stopped issuing workshop decisions for this episode.
    pub(super) episode_end_tick: u64,
    /// Tick of the evaluator's final observation frame.
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ScenarioInputReport {
    pub(super) ore_copper_ppm: u32,
    pub(super) gangue_clay_share_ppm: u32,
    pub(super) nominal_batch_mass: Mass,
    pub(super) order_mass: Mass,
    pub(super) start_at_hydration_warning_boundary: bool,
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
    pub(super) warning_deferrals: u16,
    pub(super) critical_services: u8,
    pub(super) service_ticks: u64,
    pub(super) replacement_spent: Mass,
    pub(super) supply_exhausted: bool,
    pub(super) labor_unavailable: bool,
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

#[cfg(not(test))]
#[path = "report/content_output.rs"]
mod content_output;
#[cfg(not(test))]
#[path = "report/workshop_output.rs"]
mod workshop_output;

#[cfg(not(test))]
pub(super) use content_output::print_content_summary;
#[cfg(not(test))]
pub(super) use workshop_output::print_harness_summary;
