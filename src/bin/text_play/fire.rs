//! Local campfire construction and finite fuel charging through the energy owner.

use std::fmt::Write as _;

use super::view;
use deep_hearth::content::ENERGY_STONE_RING_CAMPFIRE;
use deep_hearth::energy::{
    CombustionError, EnergyStoreAssemblyError, EnergyStoreDefinition, EnergyStoreId,
    validate_assemble_energy_store, validate_combust_stack_fuel,
};
use deep_hearth::logistics::{PlayerStockpileProjection, project_player_inventory};
use deep_hearth::{AppState, Registries};

pub fn energy_label(stored: deep_hearth::core::quantity::Energy) -> String {
    let nanojoules = stored.nanojoules();
    const NANOJOULES_PER_MEGAJOULE: u128 = 1_000_000_000_000_000;
    let fraction = format!("{:015}", nanojoules % NANOJOULES_PER_MEGAJOULE);
    let precision = fraction.trim_end_matches('0').len().max(3);
    format!(
        "{}.{} MJ",
        nanojoules / NANOJOULES_PER_MEGAJOULE,
        &fraction[..precision],
    )
}

pub(super) fn burn_runway_label(
    registries: &Registries,
    definition: deep_hearth::energy::EnergyStoreDefinitionId,
    stored: deep_hearth::core::quantity::Energy,
) -> String {
    match deep_hearth::energy::passive_dissipation_ticks_until_empty(registries, definition, stored)
    {
        Ok(Some(ticks)) if ticks.value() == 0 => "extinguished".to_owned(),
        Ok(Some(ticks)) => format!(
            "passive burn remaining ~{}",
            view::runway_label(Some(ticks.value()))
        ),
        _ => "burn duration not projected".to_owned(),
    }
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
        } => view::unsafe_handling_material(
            registries,
            commodity,
            temperature,
            maximum,
            "hand assembly",
        ),
        other => other.to_string(),
    }
}

pub(super) fn fuel_blocker(registries: &Registries, error: CombustionError) -> String {
    match error {
        CombustionError::InsufficientThermalCapacity {
            stored,
            released,
            capacity,
            ..
        } => format!(
            "fire has {} stored / {} capacity; this charge adds {}. Let it burn down before adding fuel",
            energy_label(stored),
            energy_label(capacity),
            energy_label(released),
        ),
        CombustionError::InsufficientFuel {
            available,
            required,
            ..
        } => format!(
            "selected fuel stack has {} g; one charge needs {} g",
            view::grams(available),
            view::grams(required),
        ),
        CombustionError::ImpureFuel { fuel } => format!(
            "selected {} stack contains impurities; this fire needs pure fuel",
            registries
                .materials()
                .commodity_name(fuel)
                .unwrap_or("unidentified material"),
        ),
        CombustionError::FuelTooHot {
            fuel,
            temperature,
            maximum,
        } => view::unsafe_handling_material(registries, fuel, temperature, maximum, "hand fueling"),
        other => other.to_string(),
    }
}

/// Both fire and workshop discovery use the same exact-stack combustion admission.
pub(super) fn append_fuel_choices(
    registries: &Registries,
    state: &AppState,
    id: EnergyStoreId,
    definition: &EnergyStoreDefinition,
    carried: &PlayerStockpileProjection,
    command_prefix: &str,
    output: &mut String,
) {
    for profile in definition.combustion_profiles() {
        let material = registries
            .materials()
            .commodity_name(profile.fuel())
            .unwrap_or("unidentified material");
        let _ = writeln!(
            output,
            "     Fuel: up to {} g {material} per charge",
            view::grams(profile.fuel_mass()),
        );
        let mut matching = false;
        for (index, stack) in carried
            .stacks()
            .iter()
            .enumerate()
            .filter(|(_, stack)| stack.commodity() == profile.fuel())
        {
            matching = true;
            let stack_index = index + 1;
            let choice = match validate_combust_stack_fuel(
                registries,
                state,
                id,
                carried.stockpile(),
                stack,
            ) {
                Ok(charge) => format!(
                    "{command_prefix} {stack_index} ({} g, {})",
                    view::grams(charge.consumed_mass()),
                    energy_label(charge.released_energy()),
                ),
                Err(error) => format!(
                    "stack {stack_index} blocked: {}",
                    fuel_blocker(registries, error),
                ),
            };
            let _ = writeln!(
                output,
                "       {choice} | {} C",
                view::celsius(stack.temperature())
            );
        }
        if !matching {
            output.push_str("       no matching carried stack\n");
        }
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
            "  {count}. {} | {} stored / {} capacity | {}",
            definition.name(),
            energy_label(record.stored()),
            energy_label(definition.capacity()),
            burn_runway_label(registries, record.definition(), record.stored()),
        );
        append_fuel_choices(
            registries,
            state,
            id,
            definition,
            inventory.carried(),
            &format!("feed-fire {count}"),
            &mut output,
        );
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
    definition.combustion_profile_for(commodity).ok_or_else(|| {
        format!(
            "stack {stack_index} contains {}, not campfire fuel; use 'fire' to see eligible carried stacks",
            registries
                .materials()
                .commodity_name(commodity)
                .unwrap_or("unidentified material")
        )
    })?;
    let material = registries
        .materials()
        .commodity_name(commodity)
        .unwrap_or("unidentified material")
        .to_owned();
    let charge = validate_combust_stack_fuel(registries, state, store, source, stack)
        .map_err(|error| fuel_blocker(registries, error))?;
    let mass = view::grams(charge.consumed_mass());
    charge.commit(state).map_err(|error| error.to_string())?;
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

#[cfg(test)]
#[path = "fire_tests.rs"]
mod tests;
