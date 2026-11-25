//! Player-carried manual-craft discovery and admission. No inventory fixtures or private recipe rules.

use std::fmt::Write as _;
use std::num::NonZeroU64;

use super::view;
use deep_hearth::capability::{CapabilityId, CapabilityValue};
use deep_hearth::crafting::{
    ManualCraftError, ManualCraftInputMode, PlayerManualCraftError, StartManualCraftError,
    player_manual_craft_options, validate_start_player_manual_craft_from_stack,
    validate_start_player_manual_craft_option,
};
use deep_hearth::equipment::{EquipmentId, resolve_available_equipment_provider};
use deep_hearth::logistics::project_player_inventory;
use deep_hearth::production::ProcessId;
use deep_hearth::registry::{ProcessEquipmentRole, ProcessExecutionFamily};
use deep_hearth::{AppState, Registries};

fn commodity(registries: &Registries, key: deep_hearth::material::CommodityKey) -> &str {
    registries
        .materials()
        .commodity_name(key)
        .unwrap_or("unidentified material")
}

#[derive(Clone, Copy)]
enum CraftTool {
    Carried(usize),
    Local(usize),
}

impl CraftTool {
    fn label(self) -> String {
        match self {
            Self::Carried(index) => format!("with {index}"),
            Self::Local(index) => format!("at {index}"),
        }
    }
}

/// Uses the same local equipment order as 'gear' and the carried order of 'inventory'.
fn craft_tools(
    registries: &Registries,
    state: &AppState,
    carried: &[EquipmentId],
    capability: CapabilityId,
) -> Vec<(CraftTool, EquipmentId, String)> {
    carried
        .iter()
        .copied()
        .enumerate()
        .map(|(i, id)| (CraftTool::Carried(i + 1), id))
        .chain(
            super::equipment::local_stationary(state)
                .into_iter()
                .enumerate()
                .map(|(i, id)| (CraftTool::Local(i + 1), id)),
        )
        .filter_map(|(choice, id)| {
            let provider = resolve_available_equipment_provider(registries, state, id).ok()?;
            match provider.get_capability(capability) {
                Some(CapabilityValue::MassFlow(rate)) if rate.milligrams_per_second() > 0 => {
                    let record = state.equipment().get_equipment(id)?;
                    let definition = registries.equipment().get_equipment(record.definition())?;
                    Some((choice, id, definition.name().to_owned()))
                }
                _ => None,
            }
        })
        .collect()
}

fn capability_name(registries: &Registries, capability: CapabilityId) -> &str {
    registries
        .capabilities()
        .get_capability(capability)
        .map_or("required shaping ability", |definition| definition.name())
}

fn tool_choices_label(choices: &[(CraftTool, EquipmentId, String)]) -> String {
    choices
        .iter()
        .map(|(choice, _, name)| format!("{} ({name})", choice.label()))
        .collect::<Vec<_>>()
        .join(", ")
}

fn craft_error(registries: &Registries, state: &AppState, error: PlayerManualCraftError) -> String {
    match &error {
        PlayerManualCraftError::Start(StartManualCraftError::Resolution(
            ManualCraftError::MissingEquipmentCapability {
                equipment,
                capability,
            },
        )) => {
            let name = state
                .equipment()
                .get_equipment(*equipment)
                .and_then(|record| registries.equipment().get_equipment(record.definition()))
                .map_or("Selected equipment", |definition| definition.name());
            format!(
                "{name} lacks {}. Choose compatible equipment from 'recipes' or omit the tool if optional.",
                capability_name(registries, *capability)
            )
        }
        PlayerManualCraftError::Start(StartManualCraftError::Resolution(
            ManualCraftError::EquipmentNotSupported { process, .. },
        )) => format!(
            "{} is a hand-only recipe; omit the tool selection.",
            registries
                .production()
                .get_process(*process)
                .map_or("This", |p| p.name())
        ),
        _ => super::errors::work_start_error(error),
    }
}

/// Present carried-input eligibility and accessible equipment; exact work admission owns costs.
pub fn catalog(
    registries: &Registries,
    state: &AppState,
    show_all: bool,
) -> Result<String, String> {
    let options = player_manual_craft_options(registries, state).map_err(|e| e.to_string())?;
    let inventory = project_player_inventory(registries, state).map_err(|e| e.to_string())?;
    let carried = inventory
        .equipment()
        .iter()
        .map(|item| item.equipment())
        .collect::<Vec<_>>();
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
        if let Some(profile) = definition.equipment_profile() {
            let capability = profile.mass_flow_capability();
            let choices = craft_tools(registries, state, &carried, capability);
            let _ = writeln!(
                out,
                "    {}: {}",
                capability_name(registries, capability),
                if choices.is_empty() {
                    "no suitable local or carried equipment".to_owned()
                } else {
                    tool_choices_label(&choices)
                }
            );
        }
    }
    if out.is_empty() {
        out.push_str("No recipes have enough carried input. Use 'recipes all' for the authored recipe book.\n");
    } else {
        out.push_str(
            "Use 'craft PROCESS_ID [batches] [from STACK] [with TOOL | at STATION]' to attempt a recipe.\n",
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
    let mut tool_choice = None;
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
            "with" if tool_choice.replace(CraftTool::Carried(index)).is_none() => {}
            "at" if tool_choice.replace(CraftTool::Local(index)).is_none() => {}
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
    let carried = projection
        .equipment()
        .iter()
        .map(|item| item.equipment())
        .collect::<Vec<_>>();
    let definition = registries
        .crafting()
        .get_manual(process)
        .ok_or("recipe has no manual definition")?;
    let profile = definition.equipment_profile();
    let equipment = match tool_choice {
        Some(CraftTool::Carried(index)) => Some(
            projection
                .equipment()
                .get(index - 1)
                .ok_or(format!("carried equipment index {index} does not exist"))?
                .equipment(),
        ),
        Some(CraftTool::Local(index)) => Some(
            *super::equipment::local_stationary(state)
                .get(index - 1)
                .ok_or(format!("local gear index {index} does not exist"))?,
        ),
        None if option.equipment_role() == ProcessEquipmentRole::Required => {
            let capability = profile
                .expect("required equipment has a profile")
                .mass_flow_capability();
            let choices = craft_tools(registries, state, &carried, capability);
            match choices.as_slice() {
                [(_, id, _)] => Some(*id),
                [] => {
                    return Err(format!(
                        "This recipe needs {}. No suitable equipment is available here or carried.",
                        capability_name(registries, capability)
                    ));
                }
                _ => {
                    return Err(format!(
                        "Choose equipment: {}",
                        tool_choices_label(&choices)
                    ));
                }
            }
        }
        None => None,
    };
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
    .map_err(|error| craft_error(registries, state, error))?;
    let job = candidate.commit(state).map_err(|e| e.to_string())?;
    let process_name = registries
        .production()
        .get_process(process)
        .map_or("manual recipe", |p| p.name());
    Ok(format!("Started {process_name} (job {}).\n", job.value()))
}

/// The runtime admits manual production only with the player's exclusive labor.
/// A resumed final tick can complete without creating a durable work hold.
/// Classify the receipt by its authored execution family, not by whether the
/// actor happened to hold the job at the start of the boundary.
pub(super) fn manual_completion(
    registries: &Registries,
    state: &AppState,
    receipt: &deep_hearth::production::ProcessCompletion,
) -> Option<String> {
    let topology = registries.process_topology(receipt.process())?;
    let label = match topology.execution_family() {
        ProcessExecutionFamily::ManualCraft => "Craft completed",
        ProcessExecutionFamily::ManualComminution | ProcessExecutionFamily::ManualSeparation => {
            "Ore processing completed"
        }
        ProcessExecutionFamily::PoweredCraft
        | ProcessExecutionFamily::Comminution
        | ProcessExecutionFamily::Screening
        | ProcessExecutionFamily::ConstituentSeparation
        | ProcessExecutionFamily::Precipitation
        | ProcessExecutionFamily::SensibleHeating
        | ProcessExecutionFamily::SensibleCooling
        | ProcessExecutionFamily::Melting
        | ProcessExecutionFamily::Casting => return None,
    };
    let process = registries
        .production()
        .get_process(receipt.process())
        .expect("registered production topology has a process definition")
        .name();
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
    Some(output)
}
