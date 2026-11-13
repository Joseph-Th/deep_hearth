//! Actor-local assembly, pickup, recovery, and held-tool selection using canonical owners.

use std::fmt::Write as _;

use deep_hearth::equipment::{
    PlayerEquipmentAssemblyAvailability, player_equipment_assembly_options,
    validate_assemble_player_equipment_option, validate_player_disassemble_equipment,
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

fn local_stationary(state: &AppState) -> Vec<deep_hearth::equipment::EquipmentId> {
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
        if !option.is_ready() && !show_all {
            continue;
        }
        count += 1;
        let _ = write!(
            output,
            "  {}. {} | {} g | ",
            option.definition().value(),
            option.name(),
            option.embodied_mass().milligrams() / 1_000
        );
        match option.availability() {
            PlayerEquipmentAssemblyAvailability::Ready => {
                output.push_str("ready");
            }
            PlayerEquipmentAssemblyAvailability::MissingMaterial {
                commodity,
                available,
                required,
            } => {
                let name = registries
                    .materials()
                    .commodity_name(commodity)
                    .unwrap_or("unidentified material");
                let _ = write!(
                    output,
                    "needs {} g {} (have {} g)",
                    required.milligrams() / 1_000,
                    name,
                    available.milligrams() / 1_000
                );
            }
            PlayerEquipmentAssemblyAvailability::MaterialTooHot {
                commodity,
                temperature,
                maximum,
            } => {
                let name = registries
                    .materials()
                    .commodity_name(commodity)
                    .unwrap_or("unidentified material");
                let _ = write!(
                    output,
                    "{} too hot: {} C, maximum {} C",
                    name,
                    (i64::from(temperature.millikelvin()) - 273_150) / 1_000,
                    (i64::from(maximum.millikelvin()) - 273_150) / 1_000
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
                    material.mass().milligrams() / 1_000
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
            record.embodied_mass().milligrams() / 1_000,
            record.condition().parts_per_million() / 10_000,
        );
    }
    Ok(output)
}

pub fn assemble(
    registries: &Registries,
    state: &mut AppState,
    index: usize,
) -> Result<String, String> {
    let candidate = player_equipment_assembly_options(registries, state, carried(state)?)
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|option| option.definition().value() as usize == index)
        .ok_or("equipment definition is not in the assembly catalog".to_owned())?;
    let name = candidate.name().to_owned();
    let id = validate_assemble_player_equipment_option(registries, state, &candidate)
        .map_err(|error| error.to_string())?
        .commit(state)
        .map_err(|error| error.to_string())?;
    Ok(format!(
        "Assembled {name} at your current location (equipment {}). Use 'take N' from gear.\n",
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
