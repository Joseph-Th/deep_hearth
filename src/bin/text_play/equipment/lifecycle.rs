//! Player-facing equipment maintenance and additive upgrades.

use std::fmt::Write as _;

use deep_hearth::equipment::{
    EquipmentDefinitionId, EquipmentId, EquipmentMaintenanceResolutionError, EquipmentUpgradeError,
    PlayerEquipmentMaintenanceStartError, player_equipment_maintenance_options,
    player_equipment_upgrade_options, validate_start_player_equipment_maintenance,
    validate_upgrade_equipment,
};
use deep_hearth::{AppState, Registries};

use super::{carried, local_stationary, tool_name};
use crate::text_play::{activity, errors, view};

fn service_blocker(registries: &Registries, error: PlayerEquipmentMaintenanceStartError) -> String {
    match error {
        PlayerEquipmentMaintenanceStartError::Resolution(
            EquipmentMaintenanceResolutionError::InsufficientReplacementMaterial {
                commodity,
                available,
                required,
                ..
            },
        ) => view::missing_construction_material(registries, commodity, available, required),
        PlayerEquipmentMaintenanceStartError::Resolution(
            EquipmentMaintenanceResolutionError::ReplacementMaterialTooHot {
                commodity,
                temperature,
                maximum,
            },
        ) => view::unsafe_handling_material(
            registries,
            commodity,
            temperature,
            maximum,
            "equipment maintenance",
        ),
        other => errors::work_start_error(other),
    }
}

fn upgrade_blocker(registries: &Registries, error: EquipmentUpgradeError) -> String {
    match error {
        EquipmentUpgradeError::InsufficientMaterial {
            commodity,
            available,
            required,
            ..
        } => view::missing_construction_material(registries, commodity, available, required),
        EquipmentUpgradeError::UpgradeMaterialTooHot {
            commodity,
            temperature,
            maximum,
        } => view::unsafe_handling_material(
            registries,
            commodity,
            temperature,
            maximum,
            "equipment upgrades",
        ),
        other => other.to_string(),
    }
}

/// Use exactly the same carried numbering as inventory, and the same stationary
/// numbering as take/recover. No remote equipment can enter this catalog.
fn equipment_selector(state: &AppState, local: &[EquipmentId], id: EquipmentId) -> Option<String> {
    if let Some(index) = state
        .logistics()
        .carried_equipment()
        .position(|item| item == id)
    {
        return Some(format!("carried {}", index + 1));
    }
    local
        .iter()
        .position(|item| *item == id)
        .map(|index| format!("local {}", index + 1))
}

pub(super) fn catalog(
    registries: &Registries,
    state: &AppState,
    local: &[EquipmentId],
    output: &mut String,
) -> Result<(), String> {
    maintenance_catalog(registries, state, local, output)?;
    upgrade_catalog(registries, state, local, output)
}

fn maintenance_catalog(
    registries: &Registries,
    state: &AppState,
    local: &[EquipmentId],
    output: &mut String,
) -> Result<(), String> {
    output.push_str("Equipment maintenance (service carried N / service local N):\n");
    let options = player_equipment_maintenance_options(registries, state)
        .map_err(|error| error.to_string())?;
    if options.is_empty() {
        output.push_str("  (no idle equipment needs service)\n");
    }
    for option in options {
        let selector = equipment_selector(state, local, option.equipment())
            .expect("maintenance projection refers to observable equipment");
        let replacement = registries
            .materials()
            .commodity_name(option.replacement())
            .unwrap_or("unidentified material");
        let spent = registries
            .materials()
            .commodity_name(option.spent())
            .unwrap_or("unidentified material");
        let availability = match option.admission() {
            Ok(()) => "ready".to_owned(),
            Err(error) => format!("blocked: {}", service_blocker(registries, error.clone())),
        };
        let _ = writeln!(
            output,
            "  service {selector}: {} | condition {}% -> {}% | requires {} g {replacement}; returns {spent} | {} | {availability}",
            tool_name(registries, option.definition()),
            condition_label(option.condition_before()),
            condition_label(option.target_condition()),
            view::grams(option.required_replacement_mass()),
            activity::gathering_duration(option.duration().value()),
        );
    }
    Ok(())
}

fn condition_label(condition: deep_hearth::maintenance::Condition) -> String {
    let ppm = condition.parts_per_million();
    format!("{}.{:02}", ppm / 10_000, ppm % 10_000 / 100)
}

fn upgrade_catalog(
    registries: &Registries,
    state: &AppState,
    local: &[EquipmentId],
    output: &mut String,
) -> Result<(), String> {
    output.push_str("Equipment upgrades (upgrade LOCAL_N DEFINITION_ID):\n");
    let options = player_equipment_upgrade_options(registries, state, carried(state)?)
        .map_err(|error| error.to_string())?;
    if options.is_empty() {
        output.push_str("  (no idle local equipment upgrades)\n");
    }
    for option in options {
        let index = local
            .iter()
            .position(|id| *id == option.equipment())
            .expect("upgrade projection refers to stationary local equipment")
            + 1;
        let availability = match option.admission() {
            Ok(()) => "ready".to_owned(),
            Err(error) => format!("blocked: {}", upgrade_blocker(registries, error.clone())),
        };
        let _ = writeln!(
            output,
            "  upgrade {index} {}: {} -> {} | {} g | {availability}",
            option.target_definition().value(),
            tool_name(registries, option.base_definition()),
            option.target_name(),
            view::grams(option.target_mass()),
        );
        for input in option.additions() {
            let name = registries
                .materials()
                .commodity_name(input.commodity())
                .unwrap_or("unidentified material");
            let _ = writeln!(
                output,
                "     Requires {} g {name}",
                view::grams(input.mass())
            );
        }
    }
    Ok(())
}

pub(in crate::text_play) fn service(
    registries: &Registries,
    state: &mut AppState,
    custody: &str,
    index: usize,
) -> Result<String, String> {
    let index = index.checked_sub(1).ok_or("indices begin at 1")?;
    let equipment = match custody {
        "carried" => state.logistics().carried_equipment().nth(index),
        "local" => local_stationary(state).get(index).copied(),
        _ => return Err("service requires carried or local equipment".to_owned()),
    }
    .ok_or("no equipment at that index")?;
    let validated = validate_start_player_equipment_maintenance(registries, state, equipment)
        .map_err(|error| service_blocker(registries, error))?;
    let name = tool_name(
        registries,
        state
            .equipment()
            .get_equipment(equipment)
            .expect("validated equipment exists")
            .definition(),
    );
    let outcome = validated.commit(state).map_err(|error| error.to_string())?;
    Ok(format!(
        "Started maintenance of {name} using {} g of replacement material. Scheduled completion in {}. Use 'wait finish'.\n",
        view::grams(outcome.material_mass()),
        activity::gathering_duration(outcome.completes_at().value() - state.tick().value()),
    ))
}

pub(in crate::text_play) fn upgrade(
    registries: &Registries,
    state: &mut AppState,
    index: usize,
    target: u32,
) -> Result<String, String> {
    let equipment = *local_stationary(state)
        .get(index.checked_sub(1).ok_or("indices begin at 1")?)
        .ok_or("no stationary equipment at that local index")?;
    if target == 0 {
        return Err("equipment definition IDs start at 1".to_owned());
    }
    let target = EquipmentDefinitionId::new(target);
    let validated =
        validate_upgrade_equipment(registries, state, equipment, target, carried(state)?)
            .map_err(|error| upgrade_blocker(registries, error))?;
    let before = tool_name(
        registries,
        state
            .equipment()
            .get_equipment(equipment)
            .expect("validated equipment exists")
            .definition(),
    );
    let after = tool_name(registries, target);
    validated.commit(state).map_err(|error| error.to_string())?;
    Ok(format!(
        "Upgraded {before} to {after} at your current location.\n"
    ))
}
