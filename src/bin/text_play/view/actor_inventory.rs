//! Player-facing carried inventory, quick-access labels, and co-located storage readouts.

use std::fmt::Write as _;

use deep_hearth::logistics::{
    PlayerActiveSelection, PlayerStockpileProjection, project_player_inventory,
};
use deep_hearth::{AppState, Registries};

use super::{celsius, grams, name};

/// Resolve both the hand label and whether its selected material is carried.
/// The status readout uses the same physical inventory projection for its food hint.
pub(super) fn hand_status(
    registries: &Registries,
    state: &AppState,
    selected: Option<PlayerActiveSelection>,
) -> (String, bool) {
    let projection = matches!(selected, Some(PlayerActiveSelection::Material(_)))
        .then(|| project_player_inventory(registries, state).ok())
        .flatten();
    hand_status_with_carried(
        registries,
        state,
        selected,
        projection.as_ref().map(|inventory| inventory.carried()),
    )
}

/// Reuse the exact carried projection when it is already available, avoiding
/// one complete inventory and local-container projection per hotbar slot.
pub(super) fn hand_status_with_carried(
    registries: &Registries,
    state: &AppState,
    selected: Option<PlayerActiveSelection>,
    carried: Option<&PlayerStockpileProjection>,
) -> (String, bool) {
    match selected {
        None => ("empty".to_owned(), false),
        Some(PlayerActiveSelection::Material(material)) => {
            let present = carried.is_some_and(|stockpile| {
                stockpile
                    .stacks()
                    .iter()
                    .any(|stack| stack.commodity() == material)
            });
            let label = if present {
                name(registries, material).to_owned()
            } else {
                format!("{} (none carried)", name(registries, material))
            };
            (label, present)
        }
        Some(PlayerActiveSelection::Equipment(equipment)) => (
            state
                .equipment()
                .get_equipment(equipment)
                .and_then(|record| registries.equipment().get_equipment(record.definition()))
                .map_or_else(
                    || format!("equipment #{}", equipment.value()),
                    |definition| definition.name().to_owned(),
                ),
            false,
        ),
    }
}

pub fn hand_label(
    registries: &Registries,
    state: &AppState,
    selected: Option<PlayerActiveSelection>,
) -> String {
    hand_status(registries, state, selected).0
}

pub fn inventory(registries: &Registries, state: &AppState) -> Result<String, String> {
    let projection =
        project_player_inventory(registries, state).map_err(|error| error.to_string())?;
    let mut output = String::new();
    let carried = projection.carried();
    let _ = writeln!(
        output,
        "Carried matter: {} g / {} g",
        grams(carried.stored()),
        grams(carried.capacity()),
    );
    if carried.stacks().is_empty() {
        output.push_str("  (empty)\n");
    }
    for (index, stack) in carried.stacks().iter().enumerate() {
        let _ = writeln!(
            output,
            "  {}. {} | {} g | {} C",
            index + 1,
            name(registries, stack.commodity()),
            grams(stack.mass()),
            celsius(stack.temperature()),
        );
    }
    for (index, equipment) in projection.equipment().iter().enumerate() {
        let _ = writeln!(
            output,
            "Carried equipment {}: {} | {} g | condition {}%",
            index + 1,
            registries
                .equipment()
                .get_equipment(equipment.definition())
                .map_or("unidentified equipment", |definition| definition.name()),
            grams(equipment.mass()),
            equipment.condition().parts_per_million() / 10_000,
        );
    }
    let _ = writeln!(
        output,
        "Active quick slot: {}",
        projection.active_quick_access_slot().index() + 1,
    );
    for (index, selection) in projection.quick_access_slots().iter().enumerate() {
        let _ = writeln!(
            output,
            "  slot {}: {}",
            index + 1,
            hand_status_with_carried(registries, state, *selection, Some(carried)).0
        );
    }
    for (index, local) in projection.local_containers().iter().enumerate() {
        let storage_name = local
            .storage_definition()
            .and_then(|definition| registries.storage().get(definition))
            .map_or("open ground stockpile", |definition| definition.name());
        let _ = writeln!(
            output,
            "Local storage {}: {} | {} g / {} g",
            index + 1,
            storage_name,
            grams(local.stored()),
            grams(local.capacity()),
        );
        for (stack_index, stack) in local.stacks().iter().enumerate() {
            let _ = writeln!(
                output,
                "    {}. {} | {} g | {} C",
                stack_index + 1,
                name(registries, stack.commodity()),
                grams(stack.mass()),
                celsius(stack.temperature()),
            );
        }
    }
    Ok(output)
}
