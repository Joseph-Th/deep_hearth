//! Canonical field-tool fabrication and upgrade execution.

use deep_hearth::content::{
    EQUIPMENT_COPPER_REINFORCED_GEOLOGICAL_HAMMER, EQUIPMENT_STONE_GEOLOGICAL_HAMMER,
};
use deep_hearth::core::state::AppState;
use deep_hearth::equipment::{
    EquipmentDefinitionId, EquipmentId, validate_assemble_equipment, validate_upgrade_equipment,
};
use deep_hearth::inventory::StockpileId;
use deep_hearth::registry::Registries;

use super::super::manual_craft_execution::execute_manual_craft_batches;
use super::super::manual_craft_planning::manual_craft_plan_for_available_output;
use super::planning::{FieldworkTool, equipment_component_requirements};

fn craft_equipment_components(
    registries: &Registries,
    state: &mut AppState,
    raw: StockpileId,
    parts: StockpileId,
    equipment_definitions: &[EquipmentDefinitionId],
    context: &'static str,
) -> u64 {
    let mut ticks = 0_u64;
    for (commodity, required) in equipment_component_requirements(registries, equipment_definitions)
    {
        let (craft, batches, source) = manual_craft_plan_for_available_output(
            registries,
            state,
            &[raw],
            commodity,
            required,
            context,
        );
        let duration = execute_manual_craft_batches(
            registries,
            state,
            craft.process(),
            source,
            parts,
            batches,
            context,
        );
        ticks = ticks
            .checked_add(duration.value())
            .unwrap_or_else(|| panic!("fieldwork tool preparation duration overflowed"));
    }
    ticks
}

pub(super) fn assemble_fieldwork_tool(
    registries: &Registries,
    state: &mut AppState,
    raw: StockpileId,
    parts: StockpileId,
    tool: FieldworkTool,
) -> (EquipmentId, u64) {
    let component_ticks = craft_equipment_components(
        registries,
        state,
        raw,
        parts,
        &[tool.base],
        "fieldwork selected-tool components",
    );
    let pick = validate_assemble_equipment(registries, state, tool.base, parts)
        .unwrap_or_else(|error| panic!("fieldwork hard-pick assembly failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("fieldwork hard-pick assembly commit failed: {error}"));
    if tool.target == tool.base {
        return (pick, component_ticks);
    }
    let reinforcement_ticks = craft_upgrade_additions(
        registries,
        state,
        raw,
        parts,
        tool.target,
        "fieldwork selected-tool reinforcement",
    );
    let upgraded = validate_upgrade_equipment(registries, state, pick, tool.target, parts)
        .unwrap_or_else(|error| panic!("fieldwork hard-pick upgrade failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("fieldwork hard-pick upgrade commit failed: {error}"));
    assert_eq!(upgraded, pick);
    (
        pick,
        component_ticks
            .checked_add(reinforcement_ticks)
            .unwrap_or_else(|| panic!("fieldwork hard-pick adaptation duration overflowed")),
    )
}

pub(super) fn assemble_sampling_hammer(
    registries: &Registries,
    state: &mut AppState,
    raw: StockpileId,
    parts: StockpileId,
) -> (EquipmentId, u64) {
    let setup_ticks = craft_equipment_components(
        registries,
        state,
        raw,
        parts,
        &[EQUIPMENT_STONE_GEOLOGICAL_HAMMER],
        "fieldwork sampling-hammer components",
    );
    let hammer =
        validate_assemble_equipment(registries, state, EQUIPMENT_STONE_GEOLOGICAL_HAMMER, parts)
            .unwrap_or_else(|error| panic!("fieldwork sampling-hammer assembly failed: {error}"))
            .commit(state)
            .unwrap_or_else(|error| {
                panic!("fieldwork sampling-hammer assembly commit failed: {error}")
            });
    (hammer, setup_ticks)
}

pub(super) fn upgrade_sampling_hammer(
    registries: &Registries,
    state: &mut AppState,
    raw: StockpileId,
    parts: StockpileId,
    hammer: EquipmentId,
) -> u64 {
    let reinforcement_ticks = craft_upgrade_additions(
        registries,
        state,
        raw,
        parts,
        EQUIPMENT_COPPER_REINFORCED_GEOLOGICAL_HAMMER,
        "fieldwork sampling-hammer reinforcement",
    );
    let upgraded = validate_upgrade_equipment(
        registries,
        state,
        hammer,
        EQUIPMENT_COPPER_REINFORCED_GEOLOGICAL_HAMMER,
        parts,
    )
    .unwrap_or_else(|error| panic!("fieldwork sampling-hammer upgrade failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("fieldwork sampling-hammer upgrade commit failed: {error}"));
    assert_eq!(upgraded, hammer);
    reinforcement_ticks
}

fn craft_upgrade_additions(
    registries: &Registries,
    state: &mut AppState,
    raw: StockpileId,
    parts: StockpileId,
    target: EquipmentDefinitionId,
    context: &'static str,
) -> u64 {
    let upgrade = registries
        .equipment()
        .get_equipment(target)
        .and_then(|definition| definition.upgrade_profile())
        .unwrap_or_else(|| {
            panic!(
                "fieldwork reinforced equipment {} lost its authored upgrade",
                target.value()
            )
        });
    let mut ticks = 0_u64;
    for input in upgrade.additions().inputs() {
        let (craft, batches, source) = manual_craft_plan_for_available_output(
            registries,
            state,
            &[raw],
            input.commodity(),
            input.mass(),
            context,
        );
        let duration = execute_manual_craft_batches(
            registries,
            state,
            craft.process(),
            source,
            parts,
            batches,
            context,
        );
        ticks = ticks
            .checked_add(duration.value())
            .unwrap_or_else(|| panic!("fieldwork reinforcement duration overflowed"));
    }
    ticks
}
