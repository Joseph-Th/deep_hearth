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

use super::super::manual_construction_planning::manual_construction_route_from_roots;
use super::super::manual_craft_batches::execute_manual_craft_batches;
use super::planning::{FieldworkTool, disclosed_raw_inputs, equipment_component_requirements};

fn craft_missing_component(
    registries: &Registries,
    state: &mut AppState,
    raw: StockpileId,
    parts: StockpileId,
    commodity: deep_hearth::material::CommodityKey,
    missing: deep_hearth::core::quantity::Mass,
    context: &'static str,
) -> u64 {
    let route = manual_construction_route_from_roots(
        registries,
        commodity,
        missing,
        &disclosed_raw_inputs(),
        context,
    )
    .unwrap_or_else(|| {
        panic!(
            "fieldwork {context} has no ordinary manual route from disclosed raw inputs to component {}",
            commodity.value()
        )
    });
    assert!(
        !route.steps.is_empty(),
        "fieldwork {context} assembly directly consumes disclosed raw commodity {}; add an explicit custody transfer instead of treating it as a shaped component",
        commodity.value()
    );
    let mut source = raw;
    let mut ticks = 0_u64;
    for step in route.steps {
        let duration = execute_manual_craft_batches(
            registries,
            state,
            step.process,
            source,
            parts,
            step.batches,
            context,
        );
        assert_eq!(
            duration.value(),
            step.duration_ticks,
            "fieldwork {context} live component craft diverged from pre-action topology projection"
        );
        ticks = ticks
            .checked_add(duration.value())
            .unwrap_or_else(|| panic!("fieldwork {context} component route duration overflowed"));
        source = parts;
    }
    assert_eq!(
        ticks, route.attention_ticks,
        "fieldwork {context} executed component route diverged from its projected attention"
    );
    ticks
}

pub(super) fn craft_equipment_components(
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
        let available = state
            .inventory()
            .get_stockpile(parts)
            .unwrap_or_else(|| panic!("fieldwork parts stockpile disappeared"))
            .get_mass(commodity);
        if available >= required {
            continue;
        }
        let missing = required
            .checked_sub(available)
            .unwrap_or_else(|| unreachable!("fieldwork checked existing component mass"));
        ticks = ticks
            .checked_add(craft_missing_component(
                registries, state, raw, parts, commodity, missing, context,
            ))
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
        &[tool.target],
        "fieldwork selected-tool components",
    );
    let pick = validate_assemble_equipment(registries, state, tool.target, parts)
        .unwrap_or_else(|error| panic!("fieldwork hard-pick assembly failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("fieldwork hard-pick assembly commit failed: {error}"));
    (pick, component_ticks)
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
    upgrade_fieldwork_tool(
        registries,
        state,
        raw,
        parts,
        hammer,
        EQUIPMENT_COPPER_REINFORCED_GEOLOGICAL_HAMMER,
        "fieldwork sampling-hammer reinforcement",
    )
}

pub(super) fn upgrade_fieldwork_tool(
    registries: &Registries,
    state: &mut AppState,
    raw: StockpileId,
    parts: StockpileId,
    equipment: EquipmentId,
    target: EquipmentDefinitionId,
    context: &'static str,
) -> u64 {
    let reinforcement_ticks =
        craft_upgrade_additions(registries, state, raw, parts, target, context);
    let upgraded = validate_upgrade_equipment(registries, state, equipment, target, parts)
        .unwrap_or_else(|error| panic!("fieldwork {context} upgrade validation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("fieldwork {context} upgrade commit failed: {error}"));
    assert_eq!(upgraded, equipment);
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
        let available = state
            .inventory()
            .get_stockpile(parts)
            .unwrap_or_else(|| panic!("fieldwork parts stockpile disappeared"))
            .get_mass(input.commodity());
        if available >= input.mass() {
            continue;
        }
        let missing = input
            .mass()
            .checked_sub(available)
            .unwrap_or_else(|| unreachable!("fieldwork checked existing upgrade component mass"));
        ticks = ticks
            .checked_add(craft_missing_component(
                registries,
                state,
                raw,
                parts,
                input.commodity(),
                missing,
                context,
            ))
            .unwrap_or_else(|| panic!("fieldwork reinforcement duration overflowed"));
    }
    ticks
}
