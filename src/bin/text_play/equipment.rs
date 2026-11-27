//! Actor-local assembly, pickup, recovery, and held-tool selection using canonical owners.

use std::fmt::Write as _;

use super::view;
use deep_hearth::equipment::{
    EquipmentDefinitionId, PlayerEquipmentAssemblyAvailability, player_equipment_assembly_options,
    validate_assemble_equipment, validate_player_disassemble_equipment,
};
use deep_hearth::logistics::{
    PlayerActiveSelection, project_player_inventory, validate_pickup_equipment,
    validate_set_active_selection,
};
use deep_hearth::{AppState, Registries};

fn carried(state: &AppState) -> Result<deep_hearth::inventory::StockpileId, String> {
    state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .ok_or("no admitted player".to_owned())
}

pub(super) fn local_stationary(state: &AppState) -> Vec<deep_hearth::equipment::EquipmentId> {
    state
        .available_local_equipment()
        .filter(|record| !state.logistics().is_equipment_carried(record.id()))
        .map(|record| record.id())
        .collect()
}

fn tool_name<'a>(
    registries: &'a Registries,
    definition: deep_hearth::equipment::EquipmentDefinitionId,
) -> &'a str {
    registries
        .equipment()
        .get_equipment(definition)
        .map_or("unidentified equipment", |record| record.name())
}

pub fn catalog(
    registries: &Registries,
    state: &AppState,
    show_all: bool,
) -> Result<String, String> {
    let options = player_equipment_assembly_options(registries, state, carried(state)?)
        .map_err(|error| error.to_string())?;
    let mut output = String::from("Assembly (assemble DEFINITION_ID):\n");
    let mut count = 0;
    for option in &options {
        if !option.has_usable_materials() && !show_all {
            continue;
        }
        count += 1;
        let _ = write!(
            output,
            "  {}. {} | {} g | ",
            option.definition().value(),
            option.name(),
            view::grams(option.embodied_mass())
        );
        match option.availability() {
            PlayerEquipmentAssemblyAvailability::Ready => output.push_str("ready"),
            PlayerEquipmentAssemblyAvailability::Blocked(error) => {
                let _ = write!(output, "blocked: {error}");
            }
            PlayerEquipmentAssemblyAvailability::MissingMaterial {
                commodity,
                available,
                required,
            } => {
                let name = registries
                    .materials()
                    .commodity_name(*commodity)
                    .unwrap_or("unidentified material");
                let _ = write!(
                    output,
                    "needs {} g {} (have {} g)",
                    view::grams(*required),
                    name,
                    view::grams(*available)
                );
            }
            PlayerEquipmentAssemblyAvailability::MaterialTooHot {
                commodity,
                temperature,
                maximum,
            } => {
                let name = registries
                    .materials()
                    .commodity_name(*commodity)
                    .unwrap_or("unidentified material");
                let _ = write!(
                    output,
                    "{} too hot: {} C, maximum {} C",
                    name,
                    view::celsius(*temperature),
                    view::celsius(*maximum)
                );
            }
        }
        output.push('\n');
        if show_all {
            for material in option.inputs() {
                let name = registries
                    .materials()
                    .commodity_name(material.commodity())
                    .unwrap_or("unidentified material");
                let _ = writeln!(
                    output,
                    "     Requires {} g {name}",
                    view::grams(material.mass())
                );
            }
        }
    }
    if count == 0 {
        output.push_str("  None available from your carried materials. Use 'gear all' to inspect requirements.\n");
    }
    output.push_str("Local stationary gear (take N, recover N):\n");
    let local = local_stationary(state);
    if local.is_empty() {
        output.push_str("  (none)\n");
    }
    for (index, equipment) in local.iter().enumerate() {
        let record = state
            .equipment()
            .get_equipment(*equipment)
            .ok_or("local equipment projection contains missing record")?;
        let _ = writeln!(
            output,
            "  {}. {} | {} g | condition {}%",
            index + 1,
            tool_name(registries, record.definition()),
            view::grams(record.embodied_mass()),
            record.condition().parts_per_million() / 10_000,
        );
        match validate_pickup_equipment(registries, state, *equipment) {
            Ok(_) => {
                let _ = writeln!(output, "     take {}: ready", index + 1);
            }
            Err(error) => {
                let _ = writeln!(output, "     take {}: blocked: {error}", index + 1);
            }
        }
        match validate_player_disassemble_equipment(registries, state, *equipment) {
            Ok(_) => {
                let _ = writeln!(output, "     recover {}: ready", index + 1);
            }
            Err(error) => {
                let _ = writeln!(output, "     recover {}: blocked: {error}", index + 1);
            }
        }
    }
    Ok(output)
}

pub fn assemble(
    registries: &Registries,
    state: &mut AppState,
    index: usize,
) -> Result<String, String> {
    let definition = u32::try_from(index)
        .ok()
        .filter(|id| *id != 0)
        .map(EquipmentDefinitionId::new)
        .ok_or("equipment definition is not in the assembly catalog")?;
    let name = tool_name(registries, definition);
    let id = validate_assemble_equipment(registries, state, definition, carried(state)?)
        .map_err(|error| error.to_string())?
        .commit(state)
        .map_err(|error| error.to_string())?;
    Ok(format!(
        "Assembled {name} at your current location (equipment {}). Inspect 'gear' for local use and pickup or recovery availability.\n",
        id.value()
    ))
}

pub fn take(registries: &Registries, state: &mut AppState, index: usize) -> Result<String, String> {
    let id = *local_stationary(state)
        .get(index.checked_sub(1).ok_or("indices begin at 1")?)
        .ok_or("no stationary equipment at that local index")?;
    let definition = state
        .equipment()
        .get_equipment(id)
        .ok_or("equipment disappeared")?
        .definition();
    validate_pickup_equipment(registries, state, id)
        .map_err(|error| error.to_string())?
        .commit(state)
        .map_err(|error| error.to_string())?;
    Ok(format!(
        "Picked up {}. Use 'equip N' from inventory to hold it.\n",
        tool_name(registries, definition)
    ))
}

pub fn equip(
    registries: &Registries,
    state: &mut AppState,
    index: usize,
) -> Result<String, String> {
    let inventory =
        project_player_inventory(registries, state).map_err(|error| error.to_string())?;
    let item = inventory
        .equipment()
        .get(index.checked_sub(1).ok_or("indices begin at 1")?)
        .ok_or("no carried equipment at that index")?;
    let id = item.equipment();
    let name = tool_name(registries, item.definition()).to_owned();
    let selection = PlayerActiveSelection::equipment(id);
    validate_set_active_selection(registries, state, Some(selection))
        .map_err(|error| error.to_string())?
        .commit(state)
        .map_err(|error| error.to_string())?;
    Ok(format!("Holding {name}.\n"))
}

pub fn recover(
    registries: &Registries,
    state: &mut AppState,
    index: usize,
) -> Result<String, String> {
    let id = *local_stationary(state)
        .get(index.checked_sub(1).ok_or("indices begin at 1")?)
        .ok_or("no stationary equipment at that local index")?;
    let definition = state
        .equipment()
        .get_equipment(id)
        .ok_or("equipment disappeared")?
        .definition();
    let result = validate_player_disassemble_equipment(registries, state, id)
        .map_err(|error| error.to_string())?
        .commit(state)
        .map_err(|error| error.to_string())?;
    Ok(format!(
        "Recovered {} into carried materials ({} surviving lots).\n",
        tool_name(registries, definition),
        result.recovered_lots().len()
    ))
}
