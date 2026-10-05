//! Fieldwork planning split by material opportunity and extraction-tool choice.

#[path = "planning/materials.rs"]
mod materials;
#[path = "planning/tools.rs"]
mod tools;

pub(super) use materials::{
    disclosed_raw_inputs, equipment_component_requirements, fieldwork_raw_opportunity,
    multiplied_mass, project_sampling_hammer_upgrade_ticks,
    raw_opportunity_for_equipment_components,
};
pub(super) use tools::{
    FIELDWORK_ORDER_MAX_BATCHES, FIELDWORK_TOOLS, FieldworkMiningLimits, FieldworkTool,
    FieldworkToolBlocker, FieldworkToolEstimate, assert_fieldwork_tool_market_current,
    choose_fieldwork_tool_quiet, choose_fieldwork_tool_with_market_phase, estimate_fieldwork_tool,
    estimate_fieldwork_upgrade_preparation, fieldwork_bulk_crossover,
    fieldwork_bulk_crossover_blocker, fieldwork_mining_limits,
};
