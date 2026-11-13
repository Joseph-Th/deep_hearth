//! Actor-local physical storage placement and stack moves over canonical inventory custody.

use std::fmt::Write as _;

use super::view;
use deep_hearth::inventory::{StorageDefinitionId, validate_place_player_storage};
use deep_hearth::logistics::{project_player_inventory, validate_quick_move_player_stack};
use deep_hearth::{AppState, Registries};

fn local_storage_name<'a>(
    registries: &'a Registries,
    definition: Option<StorageDefinitionId>,
) -> &'a str {
    definition
        .and_then(|definition| registries.storage().get(definition))
        .map_or("open ground stockpile", |definition| definition.name())
}

/// A catalogue read never creates a stockpile or consumes construction material.
pub fn catalog(registries: &Registries, state: &AppState, all: bool) -> Result<String, String> {
    let projection =
        project_player_inventory(registries, state).map_err(|error| error.to_string())?;
    let mut output = String::from("Buildable storage (place-store DEFINITION_ID):\n");
    let mut ready_count = 0;
    for definition in registries.storage().definitions() {
        let readiness = validate_place_player_storage(registries, state, definition.id());
        if readiness.is_err() && !all {
            continue;
        }
        ready_count += usize::from(readiness.is_ok());
        let _ = writeln!(
            output,
            "  {}. {} | {} g capacity | {}",
            definition.id().value(),
            definition.name(),
            view::grams(definition.maximum_stockpile_capacity()),
            match readiness {
                Ok(_) => "ready".to_owned(),
                Err(error) => format!("blocked: {error}"),
            },
        );
        if all {
            for input in definition.assembly_profile().inputs() {
                let name = registries
                    .materials()
                    .commodity_name(input.commodity())
                    .unwrap_or("unidentified material");
                let _ = writeln!(
                    output,
                    "     Requires {} g {name}",
                    view::grams(input.mass()),
                );
            }
        }
    }
    if ready_count == 0 && !all {
        output.push_str("  None currently placeable. Use 'storage all' to inspect blockers.\n");
    }
    output.push_str("Local containers (stash CONTAINER STACK / withdraw CONTAINER STACK):\n");
    if projection.local_containers().is_empty() {
        output.push_str("  (none)\n");
    }
    for (index, local) in projection.local_containers().iter().enumerate() {
        let _ = writeln!(
            output,
            "  {}. {} | {} g / {} g",
            index + 1,
            local_storage_name(registries, local.storage_definition()),
            view::grams(local.stored()),
            view::grams(local.capacity()),
        );
    }
    Ok(output)
}

pub fn place(registries: &Registries, state: &mut AppState, number: u32) -> Result<String, String> {
    if number == 0 {
        return Err("storage definition IDs start at 1".to_owned());
    }
    let definition = StorageDefinitionId::new(number);
    let name = registries
        .storage()
        .get(definition)
        .ok_or("unknown storage definition")?
        .name();
    let stockpile = validate_place_player_storage(registries, state, definition)
        .map_err(|error| error.to_string())?
        .commit(state)
        .map_err(|error| error.to_string())?;
    Ok(format!(
        "Placed {name} at your current location (stockpile {}). Use 'storage' to list local containers.\n",
        stockpile.value(),
    ))
}

pub fn move_stack(
    registries: &Registries,
    state: &mut AppState,
    container_index: usize,
    stack_index: usize,
    take_from_container: bool,
) -> Result<String, String> {
    let projection =
        project_player_inventory(registries, state).map_err(|error| error.to_string())?;
    let container = projection
        .local_containers()
        .get(
            container_index
                .checked_sub(1)
                .ok_or("container indices begin at 1")?,
        )
        .ok_or("no local container at that index")?;
    let (source, destination) = if take_from_container {
        (container.stacks(), projection.carried().stockpile())
    } else {
        (projection.carried().stacks(), container.stockpile())
    };
    let stack = source
        .get(
            stack_index
                .checked_sub(1)
                .ok_or("stack indices begin at 1")?,
        )
        .ok_or("no material stack at that index")?;
    let commodity = registries
        .materials()
        .commodity_name(stack.commodity())
        .unwrap_or("unidentified material")
        .to_owned();
    let moved = validate_quick_move_player_stack(registries, state, stack, destination)
        .map_err(|error| error.to_string())?
        .commit(state)
        .map_err(|error| error.to_string())?;
    Ok(format!(
        "{} {} g {commodity} {} {}.\n",
        if take_from_container {
            "Withdrew"
        } else {
            "Stashed"
        },
        view::grams(moved.mass()),
        if take_from_container { "from" } else { "in" },
        local_storage_name(registries, container.storage_definition()),
    ))
}
