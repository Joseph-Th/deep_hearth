//! Canonical primitive-to-mechanized progression probe for the gameplay experience harness.

use std::cmp::Reverse;
use std::collections::BTreeMap;

use super::environment::ROOM_TEMPERATURE;
use super::equipment_support::{nominal_equipment_mass_capability, pristine_equipment_capability};
use super::focused_runner::focused_probe_role_label;
use super::focused_seeds::{FocusedProbeCase, FocusedProbeRole};
use super::inventory_support::add_solid_stockpile;
use super::maintenance_timing::finish_active_equipment_maintenance;
use super::manual_craft_planning::{
    manual_craft_plan_for_available_output, manual_craft_topology_plan_for_output,
    project_manual_assembly_package,
};
use super::manual_craft_selection::select_manual_craft_request;
use super::manual_power_timing::finish_manual_power_work;
use super::material_selection::select_stockpile_mass;
use super::ore_fixture::copper_ore_composition;
use super::physical_time::format_physical_duration;
use super::primitive_workload::{STOCKPILE_WORK_ORDER_CYCLES, primitive_mining_cycle_mass};
use super::production_timing::finish_uninterrupted_production_job;
use super::seed::mix64;
use deep_hearth::capability::{CapabilityId, CapabilityValue};
use deep_hearth::content::gameplay_fixture::{
    GeologicalDepositSeed, seed_geological_deposit, seed_lot,
};
use deep_hearth::content::{
    ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE, ENERGY_STONE_FLYWHEEL_DRIVE,
    EQUIPMENT_COPPER_REINFORCED_HAND_CRANK, EQUIPMENT_COPPER_REINFORCED_PICK,
    EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER, EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
    EQUIPMENT_STONE_CRUSHER, EQUIPMENT_STONE_FLYWHEEL_PUMP_DRILL,
    EQUIPMENT_STONE_GEOLOGICAL_HAMMER, EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_STONE_PICK,
    EQUIPMENT_STONE_SEPARATOR, FORM_NATIVE_METAL, FORM_ORE, FORM_REINFORCEMENT, FORM_SCRAP,
    FORM_SCREEN_PLATE, MANUAL_POWER_HAND_CRANK, MATERIAL_COPPER, MINING_METHOD_HAND_PICK,
    PROCESS_COLD_WORK_COPPER_REINFORCEMENT, PROCESS_CRUSH_ORE, PROCESS_HAND_BREAK_ORE,
    PROCESS_HAND_SORT_NATIVE_COPPER, PROCESS_KNAP_STONE_TOOL, PROCESS_PIERCE_COPPER_SCREEN_PLATE,
    PROCESS_SEPARATE_NATIVE_COPPER, PROCESS_SHAPE_STONE_FLYWHEEL, PROCESS_SHAPE_WOOD_HANDLE,
    PROSPECTING_DETAILED_FIELD_SURVEY, PROSPECTING_FIELD_INSPECTION,
    PROSPECTING_REGIONAL_RECONNAISSANCE,
};
use deep_hearth::core::quantity::{Energy, Mass, MassFlow, Power, Pressure};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::crafting::{ManualCraftStartRequest, validate_start_manual_craft};
use deep_hearth::energy::{
    calculate_mass_specific_energy, calculate_mass_specific_energy_capacity,
    validate_assemble_energy_store, validate_upgrade_energy_store,
};
use deep_hearth::equipment::{
    EquipmentId, EquipmentMaintenanceRequest, resolve_equipment_maintenance,
    validate_assemble_equipment, validate_equipment_maintenance, validate_upgrade_equipment,
};
use deep_hearth::geology::{
    FieldProspectingRequest, GeologicalEvidenceConsistency, assess_geological_knowledge,
    validate_start_field_prospecting,
};
use deep_hearth::labor::{
    ManualPowerError, ManualPowerRequest, ProspectingMethodId, project_manual_power,
    project_prospecting_work, validate_start_manual_power,
};
use deep_hearth::maintenance::Condition;
use deep_hearth::material::{
    COMPOSITION_PARTS_PER_MILLION, CommodityKey, MaterialAssemblyProfile, MaterialComposition,
};
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::mining::{
    MiningStartError, MiningTargetRequest, MiningTargetResolution, MiningTargetResolutionError,
    resolve_mining_target, validate_claim_mining_output, validate_start_mining,
};
use deep_hearth::ore_processing::{
    ComminutionRequest, ComminutionResolutionError, ConstituentSeparationProcessDefinition,
    ConstituentSeparationRequest, assess_powered_ore_mass_envelope, project_manual_ore_duration,
    resolve_comminution_process, resolve_constituent_separation_process,
};
use deep_hearth::production::{
    ProcessOutputRoute, ProductionJobId, validate_start_process, validate_start_process_routed,
};
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;
use deep_hearth::spatial::{VoxelBounds, VoxelCoord};
use deep_hearth::survival::{assess_survival, initialize_player_survival};

const MAX_STEADY_STATE_CRUSH_CYCLES: u64 = 24;
const PROGRESSION_REGIONAL_ZONE_COUNT: usize = 2;
pub(super) const SHALLOW_OPPORTUNITY_MIN_BATCHES: u64 = 6;
pub(super) const SHALLOW_OPPORTUNITY_MAX_BATCHES: u64 = 40;
pub(super) const MARGINAL_OPPORTUNITY_MIN_BATCHES: u64 = 48;
pub(super) const MARGINAL_OPPORTUNITY_MAX_BATCHES: u64 = 192;
pub(super) const DEEP_OPPORTUNITY_MIN_BATCHES: u64 = 384;
pub(super) const DEEP_OPPORTUNITY_MAX_BATCHES: u64 = 512;

fn project_manual_stockpile_breaking_attention(registries: &Registries, cycle_mass: Mass) -> u64 {
    let definition = registries
        .ore_processing()
        .get_manual_comminution(PROCESS_HAND_BREAK_ORE)
        .unwrap_or_else(|| panic!("primitive progression hand-breaking route disappeared"));
    let total_mass = cycle_mass
        .milligrams()
        .checked_mul(STOCKPILE_WORK_ORDER_CYCLES)
        .unwrap_or_else(|| panic!("primitive stockpile work-order mass overflowed"));
    let maximum = definition.max_batch_mass().milligrams();
    let mut remaining = total_mass;
    let mut ticks = 0_u64;
    while remaining > 0 {
        let batch = Mass::from_milligrams(remaining.min(maximum));
        let duration = project_manual_ore_duration(
            registries.core().physical_tick_duration(),
            definition.operating_profile(),
            batch,
        )
        .unwrap_or_else(|error| {
            panic!("primitive stockpile manual-breaking projection failed: {error}")
        });
        ticks = ticks
            .checked_add(duration.value())
            .unwrap_or_else(|| panic!("primitive stockpile manual attention overflowed"));
        remaining -= batch.milligrams();
    }
    ticks
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OreOpportunity {
    batch_budget: u64,
}

impl OreOpportunity {
    pub(super) const fn batch_budget(self) -> u64 {
        self.batch_budget
    }
}

fn autonomous_target_resolution_stop(error: MiningTargetResolutionError) -> AutonomousWorkStop {
    match error {
        MiningTargetResolutionError::EvidenceInsufficientToResolveTarget { .. } => {
            AutonomousWorkStop::TargetSupply
        }
        // Evidence that no longer resolves a live target (lost, incomparable, conflicting,
        // or ruling out material) means the known supply opportunity ended. It is an
        // observed supply stop, not a harness failure, so organic variation that exhausts
        // or invalidates its target reports `TargetSupply` like the reinvestment leg does.
        MiningTargetResolutionError::NoEvidence { .. }
        | MiningTargetResolutionError::SpatiallyIncomparableEvidence { .. }
        | MiningTargetResolutionError::ConflictingEvidence { .. }
        | MiningTargetResolutionError::EvidenceRulesOutMaterial { .. } => {
            AutonomousWorkStop::TargetSupply
        }
    }
}

pub(super) fn ore_opportunity(seed: u64, maintained_reinvestment_required: bool) -> OreOpportunity {
    if maintained_reinvestment_required {
        return OreOpportunity {
            batch_budget: DEEP_OPPORTUNITY_MAX_BATCHES,
        };
    }
    let opportunity_roll = mix64(seed ^ 0x4F50_504F_5254_554E);
    let magnitude_roll = opportunity_roll / 3;
    match opportunity_roll % 3 {
        0 => OreOpportunity {
            batch_budget: SHALLOW_OPPORTUNITY_MIN_BATCHES
                + magnitude_roll
                    % (SHALLOW_OPPORTUNITY_MAX_BATCHES - SHALLOW_OPPORTUNITY_MIN_BATCHES + 1),
        },
        1 => OreOpportunity {
            batch_budget: MARGINAL_OPPORTUNITY_MIN_BATCHES
                + magnitude_roll
                    % (MARGINAL_OPPORTUNITY_MAX_BATCHES - MARGINAL_OPPORTUNITY_MIN_BATCHES + 1),
        },
        2 => OreOpportunity {
            batch_budget: DEEP_OPPORTUNITY_MIN_BATCHES
                + magnitude_roll
                    % (DEEP_OPPORTUNITY_MAX_BATCHES - DEEP_OPPORTUNITY_MIN_BATCHES + 1),
        },
        _ => unreachable!("modulo-three opportunity class is bounded"),
    }
}

fn reinforced_pick_mining_batch_limit(registries: &Registries) -> Mass {
    let method = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("primitive progression mining method disappeared"));
    nominal_equipment_mass_capability(
        registries,
        EQUIPMENT_COPPER_REINFORCED_PICK,
        method.max_batch_mass_capability(),
    )
}

pub(super) fn varied_four_way_order(seed: u64) -> [usize; 4] {
    let mut order = [0, 1, 2, 3];
    let mut random = seed;
    for upper in (1..order.len()).rev() {
        random = mix64(random ^ upper as u64);
        let selected = usize::try_from(random % (upper as u64 + 1))
            .unwrap_or_else(|_| unreachable!("four-way shuffle index fits usize"));
        order.swap(upper, selected);
    }
    order
}

fn progression_regional_bounds(zone: usize) -> VoxelBounds {
    assert!(zone < PROGRESSION_REGIONAL_ZONE_COUNT);
    let x = i64::try_from(zone)
        .unwrap_or_else(|_| unreachable!("bounded progression regional zone fits i64"))
        .checked_mul(4)
        .unwrap_or_else(|| unreachable!("bounded regional clue coordinate cannot overflow"));
    VoxelBounds::new(VoxelCoord::new(x, -4, 0), VoxelCoord::new(x + 3, -3, 1))
        .unwrap_or_else(|error| panic!("primitive progression regional bounds failed: {error}"))
}

fn regional_zone_for_clue(region: VoxelBounds, zones: &[VoxelBounds]) -> usize {
    let mut matches = zones
        .iter()
        .enumerate()
        .filter(|(_, zone)| zone.has_intersection(region))
        .map(|(index, _)| index);
    let zone = matches
        .next()
        .unwrap_or_else(|| panic!("primitive progression clue lies outside regional search zones"));
    assert!(
        matches.next().is_none(),
        "primitive progression clue overlaps multiple regional search zones"
    );
    zone
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum AutonomousWorkStop {
    #[default]
    MachineCompleted,
    FeedBufferReady,
    FeedBufferCapacity,
    TargetSupply,
    ToolCondition,
}

impl AutonomousWorkStop {
    const fn label(self) -> &'static str {
        match self {
            Self::MachineCompleted => "machine-completed",
            Self::FeedBufferReady => "policy-feed-buffer-ready",
            Self::FeedBufferCapacity => "feed-buffer-capacity",
            Self::TargetSupply => "target-supply",
            Self::ToolCondition => "tool-condition",
        }
    }
}

fn autonomous_mining_stop(error: MiningStartError) -> AutonomousWorkStop {
    match error {
        MiningStartError::DestinationCapacityExceeded { .. } => {
            AutonomousWorkStop::FeedBufferCapacity
        }
        MiningStartError::TargetNoLongerResolved => AutonomousWorkStop::TargetSupply,
        // A seam that resists the owned tool is a tooling limit whether the player learned
        // that limit through prior sampling or by directly trying a visible target.
        MiningStartError::ExcavationHardnessEvidenceExceedsCapability { .. }
        | MiningStartError::TargetResistsEquipment { .. } => AutonomousWorkStop::ToolCondition,
        MiningStartError::ConditionDuration(_) | MiningStartError::ZeroThroughput => {
            AutonomousWorkStop::ToolCondition
        }
        unexpected @ MiningStartError::UnknownMethod { .. }
        | unexpected @ MiningStartError::PlayerOutsideDeposit { .. }
        | unexpected @ MiningStartError::ZeroMass
        | unexpected @ MiningStartError::Equipment(_)
        | unexpected @ MiningStartError::EquipmentAccess(_)
        | unexpected @ MiningStartError::EquipmentMounted { .. }
        | unexpected @ MiningStartError::EquipmentBusyProduction { .. }
        | unexpected @ MiningStartError::EquipmentBusyMining { .. }
        | unexpected @ MiningStartError::EquipmentBusyManualPower { .. }
        | unexpected @ MiningStartError::MissingCapability { .. }
        | unexpected @ MiningStartError::CapabilityKindMismatch { .. }
        | unexpected @ MiningStartError::BatchTooLarge { .. }
        | unexpected @ MiningStartError::Duration(_)
        | unexpected @ MiningStartError::CompletionTickOverflow
        | unexpected @ MiningStartError::InvalidOutput(_)
        | unexpected @ MiningStartError::UnknownDestination { .. }
        | unexpected @ MiningStartError::DestinationAccess(_)
        | unexpected @ MiningStartError::DestinationBusyStorageDismantling { .. }
        | unexpected @ MiningStartError::DestinationStorage(_)
        | unexpected @ MiningStartError::DestinationMassOverflow { .. }
        | unexpected @ MiningStartError::MaterialLotIdExhausted
        | unexpected @ MiningStartError::InventoryRevisionExhausted
        | unexpected @ MiningStartError::StructureRevisionExhausted
        | unexpected @ MiningStartError::DestinationSupport(_)
        | unexpected @ MiningStartError::MiningIdExhausted
        | unexpected @ MiningStartError::MiningRevisionExhausted
        | unexpected @ MiningStartError::GeologyRevisionExhausted
        | unexpected @ MiningStartError::EquipmentRevisionExhausted
        | unexpected @ MiningStartError::Work(_) => panic!(
            "primitive progression autonomous-window mining hit unexpected blocker: {unexpected}"
        ),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MiningAttemptOutcome {
    ticks: u64,
    output: Mass,
}

fn progression_clue_bounds(slot: usize) -> VoxelBounds {
    let x = i64::try_from(slot)
        .unwrap_or_else(|_| unreachable!("four-way clue slot fits i64"))
        .checked_mul(2)
        .unwrap_or_else(|| unreachable!("bounded clue coordinate cannot overflow"));
    VoxelBounds::new(VoxelCoord::new(x, -4, 0), VoxelCoord::new(x + 1, -3, 1))
        .unwrap_or_else(|error| panic!("primitive progression clue bounds failed: {error}"))
}

/// Verifies only the runtime dependencies this episode actually intends to use.
///
/// The broader cold-agent catalog is discovered dynamically by the exploratory report. This
/// probe therefore does not freeze the whole playable catalog to an exact ID list just to protect its
/// own scenario. New routes may coexist without making this established primitive episode stale.
fn assert_progression_authored_dependencies(registries: &Registries) {
    for equipment in [
        EQUIPMENT_STONE_PICK,
        EQUIPMENT_STONE_GEOLOGICAL_HAMMER,
        EQUIPMENT_STONE_HAND_CRANK,
        EQUIPMENT_COPPER_REINFORCED_PICK,
        EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
        EQUIPMENT_STONE_CRUSHER,
        EQUIPMENT_STONE_SEPARATOR,
    ] {
        let definition = registries
            .equipment()
            .get_equipment(equipment)
            .unwrap_or_else(|| {
                panic!(
                    "primitive progression equipment {} disappeared",
                    equipment.value()
                )
            });
        assert!(
            definition.has_authored_acquisition_edge(),
            "primitive progression equipment {} lost its direct authored acquisition edge",
            equipment.value()
        );
        assert!(
            definition
                .maintenance_profile()
                .is_some_and(|profile| profile.is_component_replacement()),
            "primitive progression equipment {} lost its embodied-component service route",
            equipment.value()
        );
    }
    assert!(
        registries
            .ore_processing()
            .get_manual_comminution(PROCESS_HAND_BREAK_ORE)
            .is_some(),
        "primitive progression manual ore-breaking route disappeared"
    );
    assert!(
        registries
            .ore_processing()
            .get_manual_constituent_separation(PROCESS_HAND_SORT_NATIVE_COPPER)
            .is_some(),
        "primitive progression manual native-copper sorting route disappeared"
    );
    assert!(
        registries
            .energy()
            .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
            .is_some_and(|definition| definition.has_authored_assembly_edge()),
        "primitive progression flywheel lost its direct authored assembly edge"
    );
    for process in [
        PROCESS_KNAP_STONE_TOOL,
        PROCESS_SHAPE_WOOD_HANDLE,
        PROCESS_SHAPE_STONE_FLYWHEEL,
        PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
    ] {
        assert!(
            registries.crafting().get_manual(process).is_some(),
            "primitive progression manual process {} disappeared",
            process.value()
        );
    }
    for method in [
        PROSPECTING_REGIONAL_RECONNAISSANCE,
        PROSPECTING_FIELD_INSPECTION,
        PROSPECTING_DETAILED_FIELD_SURVEY,
    ] {
        assert!(
            registries.labor().get_prospecting(method).is_some(),
            "primitive progression prospecting method {} disappeared",
            method.value()
        );
    }
    assert!(
        registries
            .mining()
            .get_method(MINING_METHOD_HAND_PICK)
            .is_some(),
        "primitive progression hand-mining method disappeared"
    );
}

fn duration(start: u64, end: u64) -> u64 {
    end.checked_sub(start)
        .unwrap_or_else(|| panic!("primitive progression work duration underflowed"))
}

#[path = "progression_probe/preparation.rs"]
mod preparation;
use preparation::*;

#[path = "progression_probe/fieldwork.rs"]
mod fieldwork;
use fieldwork::*;

#[path = "progression_probe/mechanization.rs"]
mod mechanization;
use mechanization::*;

#[path = "progression_probe/steady_state.rs"]
mod steady_state;
pub(super) use steady_state::PrimitiveSteadyStop;
use steady_state::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PrimitivePriority {
    PickFirst,
    CrankFirst,
}

impl PrimitivePriority {
    const fn label(self) -> &'static str {
        match self {
            Self::PickFirst => "pick-first",
            Self::CrankFirst => "crank-first",
        }
    }
}

fn observed_primitive_priority(
    hard_clue: ObservedCopperClue,
    bulk_sample: ObservedMaterialSample,
) -> PrimitivePriority {
    // Spend scarce copper on access only when acquired evidence already guarantees that the blocked
    // seam beats the exact grade of owned bulk ore. Otherwise prefer mechanizing known feed first.
    // This intentionally uses the conservative lower evidence bound, never hidden seam truth.
    if hard_clue.lower_ppm > bulk_sample.copper_ppm {
        PrimitivePriority::PickFirst
    } else {
        PrimitivePriority::CrankFirst
    }
}

#[derive(Clone)]
struct PrimitiveProgressionExperience {
    natural_priority: PrimitivePriority,
    prospecting_ticks: u64,
    regional_recon_ticks: u64,
    regional_upper_bounds_ppm: [u32; PROGRESSION_REGIONAL_ZONE_COUNT],
    surface_prospecting_ticks: u64,
    hardness_sampling_ticks: u64,
    detailed_survey_ticks: u64,
    surface_clue_count: u8,
    surface_resolved_clue_count: u8,
    information_refinement_required: bool,
    refinement_triggered_by_direct_shortage: bool,
    refined_coarse_lower_ppm: u32,
    refined_coarse_upper_ppm: u32,
    refined_detailed_lower_ppm: u32,
    refined_detailed_upper_ppm: u32,
    refined_sample_copper_ppm: u32,
    refined_sample_is_ore: bool,
    stone_mineable_clue_count: u8,
    hardness_blocked_clue_count: u8,
    direct_copper_evidence_lower_ppm: u32,
    direct_copper_evidence_upper_ppm: u32,
    bulk_ore_evidence_lower_ppm: u32,
    bulk_ore_evidence_upper_ppm: u32,
    hard_ore_evidence_lower_ppm: u32,
    hard_ore_evidence_upper_ppm: u32,
    bulk_sample_copper_ppm: u32,
    processing_decision_at: u64,
    manual_bridge_ready_at: u64,
    manual_bridge_feed_mass: Mass,
    manual_bridge_attention_ticks: u64,
    preaction_manual_processing_attention_ticks: u64,
    preaction_mechanized_attention_upper_ticks: u64,
    preaction_machine_assembly_ticks: u64,
    preaction_machine_initial_charge_ticks: u64,
    preaction_machine_repeated_charge_ticks: u64,
    manual_stockpile_breaking_ticks: u64,
    mechanized_stockpile_player_ticks: u64,
    manual_bridge_recovery_ppm: u32,
    manual_bootstrap_pick_ready_ticks: u64,
    manual_bootstrap_hard_sample_ticks: u64,
    manual_bootstrap_second_ready_ticks: u64,
    manual_bootstrap_machine_ready_ticks: u64,
    manual_bootstrap_selected_hard_feed: bool,
    manual_bridge_metabolic_cost_nj: u128,
    manual_bridge_hydration_cost_ul: u64,
    selected_processing_feed_copper_ppm: u32,
    selected_processing_feed_is_hard: bool,
    processing_feed_selected_from_bulk: bool,
    post_convergence_mining_target_is_hard: bool,
    refined_clue_sample_mass: Mass,
    refined_clue_mining_ticks: u64,
    primary_batch_mass: Mass,
    first_upgrade_at: u64,
    second_upgrade_at: u64,
    pick_upgraded_at: Option<u64>,
    hard_seam_accessed_at: Option<u64>,
    machine_started_at: u64,
    automation_preparation_ticks: u64,
    separator_preparation_ticks: u64,
    processing_line_preparation_ticks: u64,
    processing_line_preparation_metabolic_cost_nj: u128,
    processing_line_preparation_hydration_cost_ul: u64,
    overlap_setup_equivalent_cycles: Option<u64>,
    steady_state_cycles: u64,
    steady_state_stop: PrimitiveSteadyStop,
    final_crusher_condition_ppm: u32,
    initial_full_charge_ticks: u64,
    first_processed_output_at: u64,
    elapsed_ticks: u64,
    soft_ore_mining_ticks: u64,
    reinforced_mining_ticks: Option<u64>,
    charge_ticks: u64,
    machine_work_ticks: u64,
    reserve_batch_mass: Mass,
    reserve_machine_work_ticks: u64,
    overlap_ticks: u64,
    machine_useful_overlap_ticks: u64,
    reserve_useful_overlap_ticks: u64,
    machine_player_free_ticks: u64,
    primary_autonomous_stop: AutonomousWorkStop,
    reserve_autonomous_stop: AutonomousWorkStop,
    primary_mining_jobs: u64,
    reserve_mining_jobs: u64,
    steady_mining_jobs: u64,
    steady_feed_buffer_ready_cycles: u64,
    steady_feed_buffer_capacity_cycles: u64,
    separation_feed_mass: Mass,
    recovered_copper_mass: Mass,
    separation_required_energy: Energy,
    flywheel_loss_before_reserve: Energy,
    reserve_recharge_ticks: u64,
    separation_ticks: u64,
    separation_completed_at: u64,
    processed_output_enabled_second_upgrade: bool,
    hard_ore_mined: Mass,
    hard_ore_before_convergence: Mass,
    total_ore_mined: Mass,
    direct_second_upgrade_blocked: bool,
    initial_crank_reinforced: bool,
    crank_reinforced: bool,
    maintenance_material_preparation_ticks: u64,
    maintenance_preparation_overlap_ticks: u64,
    component_service_ticks: u64,
    component_service_mass: Mass,
    component_service_condition_before_ppm: u32,
    component_service_preserved_reinforcement: bool,
    final_pick_condition_ppm: u32,
    metabolic_energy_spent_nj: u128,
    hydration_spent_ul: u64,
    reinvestment: PrimitiveReinvestmentOutcome,
    stockpiling_reinvestment: PrimitiveReinvestmentOutcome,
    stockpiling_delay_ticks: u64,
    selected_end: PrimitiveSelectedEnd,
}

/// Actual primary-state endpoint, never the throughput/service coverage clone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PrimitiveSelectedEnd {
    pub(crate) decision_at: u64,
    pub(crate) completed_at: u64,
    pub(crate) crushed_mass: Mass,
    pub(crate) native_copper: Mass,
    pub(crate) pick_condition_ppm: u32,
    pub(crate) crusher_reinforced: bool,
    pub(crate) separator_reinforced: bool,
    pub(crate) drive_reinforced: bool,
    pub(crate) metabolic_energy_spent_nj: u128,
    pub(crate) hydration_spent_ul: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PrimitiveReinvestmentOutcome {
    Completed(Box<PrimitiveReinvestmentExperience>),
    TargetSupplyLimited,
    StorageCapacityLimited {
        available: Mass,
        required_above: Mass,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PrimitiveReinvestmentExperience {
    pub(crate) elapsed_ticks: u64,
    pub(crate) stockpile_demand_executed: bool,
    pub(crate) stockpile_before_demand: Mass,
    pub(crate) stockpile_after_demand: Mass,
    pub(crate) stockpile_demand_feed: Mass,
    pub(crate) stockpile_demand_copper: Mass,
    pub(crate) stockpile_demand_energy: Energy,
    pub(crate) stockpile_demand_charge_ticks: u64,
    pub(crate) stockpile_demand_separation_ticks: u64,
    pub(crate) sizing_plate_continuation_ticks: u64,
    invested_copper_mass: Mass,
    base_crush_ticks: u64,
    reinforced_crush_ticks: u64,
    crusher_time_reduction_ppm: u32,
    base_separator_ticks: u64,
    reinforced_separator_ticks: u64,
    base_separator_processing_rate: MassFlow,
    reinforced_separator_processing_rate: MassFlow,
    separator_processing_rate_gain_ppm: u32,
    base_separator_target_mass: Mass,
    reinforced_separator_target_mass: Mass,
    base_separator_batch_capacity: Mass,
    upgraded_separator_batch_capacity: Mass,
    base_drive_capacity: Energy,
    upgraded_drive_capacity: Energy,
    expanded_batch_mass: Mass,
    expanded_batch_energy: Energy,
    expanded_charge_ticks: u64,
    expanded_crush_ticks: u64,
    expanded_separator_energy: Energy,
    expanded_separator_ticks: u64,
    expanded_separator_target_mass: Mass,
    survival_energy_spent_nj: u128,
    survival_hydration_spent_ul: u64,
}

#[path = "progression_probe/reinvestment.rs"]
mod reinvestment;
use reinvestment::{evaluate_mature_reinvestment, run_mature_reinvestment};

#[path = "progression_probe/episode.rs"]
mod episode;
use episode::run_primitive_progression_case;

#[path = "progression_probe/manual_processing.rs"]
pub(super) mod manual_processing;
use manual_processing::{
    OwnedOreManualBridgePlan, evaluate_manual_processing_fallback,
    evaluate_owned_ore_manual_bridge, project_owned_ore_manual_bridge, run_owned_ore_manual_bridge,
};

#[path = "progression_probe/review.rs"]
pub(super) mod review;

pub(super) use review::run_primitive_progression_probe;
