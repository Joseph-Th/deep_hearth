//! Replayable ordinary-play woodworking investment episode for the cold-agent report.

use std::num::NonZeroU64;

use deep_hearth::content::gameplay_fixture::seed_lot;
use deep_hearth::content::{
    EQUIPMENT_STONE_WOODWORKING_ADZE, EQUIPMENT_TIMBER_FRAME_SAW_BENCH, FORM_BOARD, FORM_CHIP,
    FORM_LOG, FORM_LUMP, FORM_NATIVE_METAL, MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD,
    PROCESS_COLD_WORK_COPPER_SAW_BLADE, PROCESS_KNAP_STONE_TOOL, PROCESS_SAW_WOOD_BOARDS,
    PROCESS_SHAPE_WOOD_BOARDS,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::crafting::{project_manual_craft_equipment, resolve_manual_craft};
use deep_hearth::equipment::{
    EquipmentId, EquipmentMaintenanceRequest, resolve_equipment_maintenance,
    validate_assemble_equipment, validate_equipment_maintenance,
};
use deep_hearth::inventory::StockpileId;
use deep_hearth::maintenance::{Condition, MaintenanceBand};
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::production::ProcessResolution;
use deep_hearth::registry::Registries;
use deep_hearth::survival::initialize_player_survival;

use super::environment::ROOM_TEMPERATURE;
use super::focused_runner::focused_probe_role_label;
use super::focused_seeds::{FocusedProbeCase, FocusedProbeRole};
use super::inventory_support::add_solid_stockpile;
use super::maintenance_timing::finish_active_equipment_maintenance;
use super::manual_craft_execution::{execute_manual_craft, execute_manual_craft_batches};
use super::manual_craft_planning::manual_craft_plan_for_available_output;
use super::manual_craft_selection::select_manual_craft_request;
use super::physical_time::format_physical_duration;
use super::seed::mix64;

#[path = "woodworking_probe/policy.rs"]
mod policy;

use policy::{
    WoodworkingInvestmentPreference, WoodworkingInvestmentReason, WoodworkingTimberBalance,
    woodworking_investment_decision, woodworking_timber_balance,
};

#[path = "woodworking_probe/evaluation.rs"]
mod evaluation;
#[path = "woodworking_probe/execution.rs"]
mod execution;

pub(super) fn run_woodworking_probe(registries: &Registries, case: FocusedProbeCase) {
    evaluation::run_woodworking_probe(registries, case);
}
