//! Player-carried manual-craft discovery and admission. No inventory fixtures or private recipe rules.

use std::fmt::Write as _;
use std::num::NonZeroU64;

use super::view;
use deep_hearth::crafting::{
    ManualCraftInputMode, player_manual_craft_options,
    validate_start_player_manual_craft_from_stack, validate_start_player_manual_craft_option,
};
use deep_hearth::logistics::project_player_inventory;
use deep_hearth::production::ProcessId;
use deep_hearth::registry::ProcessEquipmentRole;
use deep_hearth::{AppState, Registries};

fn commodity(registries: &Registries, key: deep_hearth::material::CommodityKey) -> &str {
    registries
        .materials()
        .commodity_name(key)
        .unwrap_or("unidentified material")
}

/// Only present carried-input eligibility; the production owner still checks tool and work costs.
pub fn catalog(
    registries: &Registries,
    state: &AppState,
    show_all: bool,
) -> Result<String, String> {
    let options = player_manual_craft_options(registries, state).map_err(|e| e.to_string())?;
    let inventory = project_player_inventory(registries, state).map_err(|e| e.to_string())?;
    let mut out = String::new();
    for option in options {
        let definition = registries
            .crafting()
            .get_manual(option.process())
            .ok_or("recipe catalog contains an unknown manual process")?;
        let (eligible, available) = match option.input_mode() {
            ManualCraftInputMode::Automatic(input) => (
                input.maximum_batches() > 0,
                format!(
                    "{} {} available",
                    input.maximum_batches(),
                    if input.maximum_batches() == 1 {
                        "batch"
                    } else {
                        "batches"
                    }
                ),
            ),
            ManualCraftInputMode::ExplicitStackChoice { input, batch_mass } => {
                let stacks = inventory
                    .carried()
                    .stacks()
                    .iter()
                    .filter(|stack| stack.commodity() == input && stack.mass() >= batch_mass)
                    .count();
                (
                    stacks > 0,
                    format!(
                        "{} candidate {}, choose 'from STACK'",
                        stacks,
                        if stacks == 1 { "stack" } else { "stacks" }
                    ),
                )
            }
        };
        if !eligible && !show_all {
            continue;
        }
        let process = registries
            .production()
            .get_process(option.process())
            .ok_or("recipe has no registered process description")?;
        let tool = match option.equipment_role() {
            ProcessEquipmentRole::None => "by hand",
            ProcessEquipmentRole::Optional => "tool optional",
            ProcessEquipmentRole::Required => "tool required",
        };
        let _ = writeln!(
            out,
            "{}. {} | {} g {} | {} | {}",
            option.process().value(),
            process.name(),
            view::grams(definition.input_mass()),
            commodity(registries, definition.input()),
            available,
            tool,
        );
        let mut products = String::new();
        for (i, product) in definition.outputs().iter().enumerate() {
            if i > 0 {
                products.push_str(", ");
            }
            let _ = write!(
                products,
                "{} g {}",
                view::grams(product.mass()),
                commodity(registries, product.commodity())
            );
        }
        let _ = writeln!(out, "    Produces: {products}");
    }
    if out.is_empty() {
        out.push_str("No recipes have enough carried input. Use 'recipes all' for the authored recipe book.\n");
    } else {
        out.push_str(
            "Use 'craft PROCESS_ID [batches] [from STACK] [with TOOL]' to attempt a recipe.\n",
        );
    }
    Ok(out)
}

pub fn start(
    registries: &Registries,
    state: &mut AppState,
    args: &[&str],
) -> Result<String, String> {
    let Some(first) = args.first() else {
        return Err("craft requires a process ID from recipes".to_owned());
    };
    let id: u32 = first
        .parse()
        .map_err(|_| "craft requires a positive process ID")?;
    if id == 0 {
        return Err("craft requires a positive process ID".to_owned());
    }
    let process = ProcessId::new(id);
    let mut next = 1;
    let batches = if args
        .get(next)
        .is_some_and(|s| s.chars().all(|c| c.is_ascii_digit()))
    {
        let number = args[next]
            .parse::<u64>()
            .map_err(|_| "batch count is too large")?;
        next += 1;
        NonZeroU64::new(number).ok_or("batch count must be positive")?
    } else {
        NonZeroU64::new(1).unwrap_or_else(|| unreachable!("one is nonzero"))
    };
    let mut stack_index = None;
    let mut tool_index = None;
    while next < args.len() {
        let key = args[next];
        let index = args
            .get(next + 1)
            .ok_or_else(|| format!("{key} requires an inventory index"))?
            .parse::<usize>()
            .map_err(|_| format!("{key} requires a positive inventory index"))?;
        if index == 0 {
            return Err("inventory indices begin at 1".to_owned());
        }
        match key {
            "from" if stack_index.replace(index).is_none() => {}
            "with" if tool_index.replace(index).is_none() => {}
            _ => return Err(format!("unknown or repeated craft option: {key}")),
        }
        next += 2;
    }
    let options = player_manual_craft_options(registries, state).map_err(|e| e.to_string())?;
    let option = options
        .into_iter()
        .find(|entry| entry.process() == process)
        .ok_or(format!(
            "process {id} is not in the player's manual recipe catalog"
        ))?;
    let projection = project_player_inventory(registries, state).map_err(|e| e.to_string())?;
    let equipment = match tool_index {
        Some(index) => Some(
            projection
                .equipment()
                .get(index - 1)
                .ok_or(format!("carried equipment index {index} does not exist"))?
                .equipment(),
        ),
        None => None,
    };
    if option.equipment_role() == ProcessEquipmentRole::Required && equipment.is_none() {
        return Err(
            "this recipe requires a carried tool; use 'with TOOL' from inventory".to_owned(),
        );
    }
    let candidate = match (option.input_mode(), stack_index) {
        (ManualCraftInputMode::Automatic(_), None) => {
            validate_start_player_manual_craft_option(registries, state, option, batches, equipment)
        }
        (ManualCraftInputMode::ExplicitStackChoice { .. }, Some(index)) => {
            let stack = projection
                .carried()
                .stacks()
                .get(index - 1)
                .ok_or(format!("carried stack index {index} does not exist"))?;
            validate_start_player_manual_craft_from_stack(
                registries, state, process, batches, stack, equipment,
            )
        }
        (ManualCraftInputMode::ExplicitStackChoice { .. }, None) => {
            return Err(
                "this recipe requires a freshness-specific carried stack; use 'from STACK'"
                    .to_owned(),
            );
        }
        (ManualCraftInputMode::Automatic(_), Some(_)) => {
            return Err(
                "this recipe chooses its input lots automatically; omit 'from STACK'".to_owned(),
            );
        }
    }
    .map_err(|e| e.to_string())?;
    let job = candidate.commit(state).map_err(|e| e.to_string())?;
    let process_name = registries
        .production()
        .get_process(process)
        .map_or("manual recipe", |p| p.name());
    Ok(format!("Started {process_name} (job {}).\n", job.value()))
}

/// Display only a completion of the player's own known job, never unrelated production.
pub fn completion(
    registries: &Registries,
    state: &AppState,
    receipt: &deep_hearth::production::ProcessCompletion,
) -> String {
    let process = registries
        .production()
        .get_process(receipt.process())
        .map_or("manual recipe", |p| p.name());
    let label = if registries
        .crafting()
        .get_manual(receipt.process())
        .is_some()
    {
        "Craft completed"
    } else if registries
        .ore_processing()
        .get_manual_comminution(receipt.process())
        .is_some()
        || registries
            .ore_processing()
            .get_manual_constituent_separation(receipt.process())
            .is_some()
    {
        "Ore processing completed"
    } else {
        "Work completed"
    };
    let mut output = format!("{label}: {process}.\n");
    let carried = state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile());
    for landing in receipt.landings() {
        let destination = if Some(landing.destination()) == carried {
            "carried".to_owned()
        } else {
            format!("stockpile {}", landing.destination().value())
        };
        for parcel in landing.parcels() {
            let item = parcel.output();
            let _ = writeln!(
                output,
                "  Received {} g {} ({destination}).",
                super::view::grams(item.mass()),
                commodity(registries, item.commodity()),
            );
        }
    }
    output
}
