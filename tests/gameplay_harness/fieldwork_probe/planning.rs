//! Fieldwork planning split by material opportunity and extraction-tool choice.

#[path = "planning/materials.rs"]
mod materials;
#[path = "planning/tools.rs"]
mod tools;

pub(super) use materials::{
    equipment_component_requirements, fieldwork_raw_opportunity, multiplied_mass,
    project_sampling_hammer_upgrade_ticks,
};
#[cfg(test)]
pub(super) use tools::{
    FIELDWORK_TOOLS, FieldworkToolBlocker, choose_fieldwork_tool, estimate_fieldwork_tool,
};
pub(super) use tools::{
    FieldworkMiningLimits, FieldworkTool, FieldworkToolEstimate,
    choose_fieldwork_tool_with_market_phase, fieldwork_bulk_crossover, fieldwork_mining_limits,
};
