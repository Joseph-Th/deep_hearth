//! Local campfire construction and finite fuel charging through the energy owner.

use std::fmt::Write as _;

use super::view;
use deep_hearth::content::ENERGY_STONE_RING_CAMPFIRE;
use deep_hearth::energy::{
    EnergyStoreAssemblyError, EnergyStoreId, validate_assemble_energy_store,
    validate_combust_selected_fuel,
};
use deep_hearth::logistics::project_player_inventory;
use deep_hearth::{AppState, Registries};

pub fn energy_label(stored: deep_hearth::core::quantity::Energy) -> String {
    let nanojoules = stored.nanojoules();
    format!(
        "{}.{:03} MJ",
        nanojoules / 1_000_000_000_000_000,
        (nanojoules % 1_000_000_000_000_000) / 1_000_000_000_000,
    )
}

fn campfires(state: &AppState, registries: &Registries) -> Vec<EnergyStoreId> {
    state
        .available_local_energy_stores()
        .filter(|store| {
            registries
                .energy()
                .get_store(store.definition())
                .is_some_and(|definition| {
                    definition.combustion_profile().is_some() && definition.is_local_warmth_source()
                })
        })
        .map(|store| store.id())
        .collect()
}

fn carried(state: &AppState) -> Result<deep_hearth::inventory::StockpileId, String> {
    state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .ok_or("no admitted player".to_owned())
}

pub(super) fn assembly_blocker(registries: &Registries, error: EnergyStoreAssemblyError) -> String {
    match error {
        EnergyStoreAssemblyError::InsufficientMaterial {
            commodity,
            available,
            required,
            ..
        } => view::missing_construction_material(registries, commodity, available, required),
        EnergyStoreAssemblyError::AssemblyMaterialTooHot {
            commodity,
            temperature,
            maximum,
        } => view::unsafe_construction_material(
            registries,
            commodity,
            temperature,
            maximum,
            "hand assembly",
        ),
        other => other.to_string(),
    }
}

pub fn catalog(registries: &Registries, state: &AppState) -> Result<String, String> {
    let source = carried(state)?;
    let inventory =
        project_player_inventory(registries, state).map_err(|error| error.to_string())?;
    let definition = registries
        .energy()
        .get_store(ENERGY_STONE_RING_CAMPFIRE)
        .ok_or("built-in campfire has no definition")?;
    let mut output = String::new();
    let placement =
        validate_assemble_energy_store(registries, state, ENERGY_STONE_RING_CAMPFIRE, source);
    let _ = writeln!(
        output,
        "Campfire construction (build-fire): {}",
        match placement {
            Ok(_) => "ready".to_owned(),
            Err(error) => format!("blocked: {}", assembly_blocker(registries, error)),
        },
    );
    for input in definition
        .assembly_profile()
        .ok_or("campfire has no physical assembly")?
        .inputs()
    {
        let material = registries
            .materials()
            .commodity_name(input.commodity())
            .unwrap_or("unidentified material");
        let _ = writeln!(
            output,
            "  Requires {} g {material}",
            view::grams(input.mass()),
        );
    }
    output.push_str("Local fires (feed-fire FIRE STACK):\n");
    let mut count = 0;
    for id in campfires(state, registries) {
        count += 1;
        let record = state
            .energy()
            .get_store(id)
            .ok_or("local fire disappeared")?;
        let definition = registries
            .energy()
            .get_store(record.definition())
            .ok_or("local fire has no authored definition")?;
        let _ = writeln!(
            output,
            "  {count}. {} | {} stored",
            definition.name(),
            energy_label(record.stored()),
        );
        for profile in definition.combustion_profiles() {
            let material = registries
                .materials()
                .commodity_name(profile.fuel())
                .unwrap_or("unidentified material");
            let eligible = inventory
                .carried()
                .stacks()
                .iter()
                .enumerate()
                .find(|(_, stack)| {
                    stack.commodity() == profile.fuel() && stack.mass() >= profile.fuel_mass()
                })
                .map(|(index, _)| index + 1);
            let _ = writeln!(
                output,
                "     Fuel: {} g {material} per charge | {}",
                view::grams(profile.fuel_mass()),
                eligible.map_or_else(
                    || "no sufficient carried stack".to_owned(),
                    |stack| format!("feed-fire {count} {stack}")
                ),
            );
        }
    }
    if count == 0 {
        output.push_str("  (none at your current location)\n");
    }
    Ok(output)
}

pub fn build(registries: &Registries, state: &mut AppState) -> Result<String, String> {
    let source = carried(state)?;
    let id = validate_assemble_energy_store(registries, state, ENERGY_STONE_RING_CAMPFIRE, source)
        .map_err(|error| assembly_blocker(registries, error))?
        .commit(state)
        .map_err(|error| error.to_string())?;
    Ok(format!(
        "Built a stone-ring campfire (store {}). Charge it with 'feed-fire FIRE STACK'.\n",
        id.value(),
    ))
}

pub fn fuel(
    registries: &Registries,
    state: &mut AppState,
    fire_index: usize,
    stack_index: usize,
) -> Result<String, String> {
    let store = *campfires(state, registries)
        .get(fire_index.checked_sub(1).ok_or("fire indices start at 1")?)
        .ok_or("no local campfire at that index")?;
    let carried = project_player_inventory(registries, state).map_err(|error| error.to_string())?;
    let source = carried.carried().stockpile();
    let stack = carried
        .carried()
        .stacks()
        .get(
            stack_index
                .checked_sub(1)
                .ok_or("stack indices start at 1")?,
        )
        .ok_or("no carried material at that stack index")?;
    let commodity = stack.commodity();
    let definition = state
        .energy()
        .get_store(store)
        .and_then(|record| registries.energy().get_store(record.definition()))
        .ok_or("local campfire has no definition")?;
    let fuel = definition.combustion_profile_for(commodity).ok_or_else(|| {
        format!(
            "stack {stack_index} contains {}, not campfire fuel; use 'fire' to see eligible carried stacks",
            registries
                .materials()
                .commodity_name(commodity)
                .unwrap_or("unidentified material")
        )
    })?;
    let mass = view::grams(fuel.fuel_mass());
    let material = registries
        .materials()
        .commodity_name(commodity)
        .unwrap_or("unidentified material")
        .to_owned();
    validate_combust_selected_fuel(registries, state, store, source, commodity)
        .map_err(|error| error.to_string())?
        .commit(state)
        .map_err(|error| error.to_string())?;
    let energy = state
        .energy()
        .get_store(store)
        .ok_or("campfire disappeared after fueling")?
        .stored();
    Ok(format!(
        "Burned {mass} g {material} in the campfire; thermal store now has {}.\n",
        energy_label(energy),
    ))
}
