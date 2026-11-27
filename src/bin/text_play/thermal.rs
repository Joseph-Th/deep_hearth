//! Expose authored ceramic firing and cooling through exact-local production.

use std::fmt::Write as _;

use deep_hearth::content::{ENERGY_STONE_CERAMIC_COOLING_BED, ENERGY_STONE_PIT_KILN_HEARTH};
use deep_hearth::energy::{
    EnergyStoreDefinitionId, EnergyStoreId, validate_assemble_energy_store,
    validate_combust_stack_fuel,
};
use deep_hearth::inventory::MaterialStackProjection;
use deep_hearth::logistics::project_player_inventory;
use deep_hearth::production::ProcessId;
use deep_hearth::thermal::validate_start_player_thermal_batch_from_stack;
use deep_hearth::{AppState, Registries};

use super::{activity, command::parse_index, equipment, fire, quantity, view};

const STORE_DEFINITIONS: [EnergyStoreDefinitionId; 2] = [
    ENERGY_STONE_PIT_KILN_HEARTH,
    ENERGY_STONE_CERAMIC_COOLING_BED,
];

fn stores(state: &AppState) -> Vec<EnergyStoreId> {
    state
        .available_local_energy_stores()
        .filter(|record| STORE_DEFINITIONS.contains(&record.definition()))
        .map(|record| record.id())
        .collect()
}

pub(super) fn catalog(registries: &Registries, state: &AppState) -> Result<String, String> {
    let inventory = project_player_inventory(registries, state).map_err(|e| e.to_string())?;
    let source = inventory.carried().stockpile();
    let mut out = String::from(
        "Ceramic workshop: assemble the kiln using 'gear'; build a hearth and cooling bed.\n",
    );
    for id in STORE_DEFINITIONS {
        let definition = registries
            .energy()
            .get_store(id)
            .expect("authored ceramic store");
        let readiness = validate_assemble_energy_store(registries, state, id, source);
        let _ = writeln!(
            out,
            "  build-heat {}: {} | {}",
            id.value(),
            definition.name(),
            readiness.map_or_else(
                |e| fire::assembly_blocker(registries, e),
                |_| "ready".to_owned()
            )
        );
        for input in definition
            .assembly_profile()
            .expect("ceramic store assembly")
            .inputs()
        {
            let _ = writeln!(
                out,
                "     Requires {} g {}",
                view::grams(input.mass()),
                registries
                    .materials()
                    .commodity_name(input.commodity())
                    .unwrap_or("unknown material")
            );
        }
    }
    let local_stores = stores(state);
    out.push_str("Local heat stores (feed-heat STORE STACK):\n");
    for (i, id) in local_stores.iter().enumerate() {
        let store = state.energy().get_store(*id).expect("local energy store");
        let definition = registries
            .energy()
            .get_store(store.definition())
            .expect("authored store");
        let _ = writeln!(
            out,
            "  {}. {} | {} stored / {} capacity | {}",
            i + 1,
            definition.name(),
            fire::energy_label(store.stored()),
            fire::energy_label(definition.capacity()),
            if definition.combustion_profile().is_some() {
                fire::burn_runway_label(registries, store.definition(), store.stored())
            } else {
                "heat sink".to_owned()
            },
        );
        fire::append_fuel_choices(
            registries,
            state,
            *id,
            definition,
            inventory.carried(),
            &format!("feed-heat {}", i + 1),
            &mut out,
        );
    }
    if local_stores.is_empty() {
        out.push_str("  (none)\n");
    }
    out.push_str("Firing and cooling (heat PROCESS STACK STATION STORE [grams]; stations use 'gear' indices):\n");
    let firing = registries
        .thermal()
        .sensible_heating_definitions()
        .filter_map(|definition| {
            let conversion = definition.firing_conversion()?;
            Some((
                definition.process(),
                conversion.input(),
                conversion.minimum_temperature(),
                true,
            ))
        });
    let cooling = registries
        .thermal()
        .sensible_cooling_definitions()
        .map(|definition| {
            (
                definition.process(),
                definition.commodity(),
                definition.target_temperature(),
                false,
            )
        });
    for (process, input, target, heating) in firing.chain(cooling) {
        let name = registries
            .production()
            .get_process(process)
            .expect("authored thermal process")
            .name();
        let _ = writeln!(
            out,
            "  {}. {name} | {} C target",
            process.value(),
            view::celsius(target)
        );
        for (j, stack) in inventory
            .carried()
            .stacks()
            .iter()
            .enumerate()
            .filter(|(_, stack)| {
                stack.commodity() == input && (heating || stack.temperature() > target)
            })
        {
            let _ = writeln!(
                out,
                "     Stack {}: {} g | {} C",
                j + 1,
                view::grams(stack.mass()),
                view::celsius(stack.temperature())
            );
            describe_batch_choices(
                registries,
                state,
                process,
                stack,
                j + 1,
                &local_stores,
                &mut out,
            );
        }
    }
    Ok(out)
}

fn describe_batch_choices(
    registries: &Registries,
    state: &AppState,
    process: ProcessId,
    stack: &MaterialStackProjection,
    stack_index: usize,
    local_stores: &[EnergyStoreId],
    out: &mut String,
) {
    let topology = registries
        .process_topology(process)
        .expect("authored thermal topology");
    let stations = equipment::local_stationary(state);
    let mut found = false;
    for (k, station) in stations.iter().enumerate() {
        let record = state
            .equipment()
            .get_equipment(*station)
            .expect("local station");
        if !topology.nominal_providers().contains(&record.definition()) {
            continue;
        }
        for (l, energy) in local_stores.iter().enumerate() {
            let store = state
                .energy()
                .get_store(*energy)
                .expect("local energy store");
            if !topology
                .compatible_energy_stores()
                .contains(&store.definition())
            {
                continue;
            }
            found = true;
            let candidate = validate_start_player_thermal_batch_from_stack(
                registries,
                state,
                process,
                stack,
                stack.mass(),
                *station,
                *energy,
            );
            let _ = writeln!(
                out,
                "     heat {} {} {} {} | {}",
                process.value(),
                stack_index,
                k + 1,
                l + 1,
                candidate.map_or_else(super::errors::work_start_error, |_| "ready".to_owned())
            );
        }
    }
    if !found {
        out.push_str("     Needs compatible local gear and heat store.\n");
    }
}

pub(super) fn build(
    registries: &Registries,
    state: &mut AppState,
    id: &str,
) -> Result<String, String> {
    let id = id
        .parse::<u32>()
        .map_err(|_| "use a build-heat definition ID from thermal")?;
    let definition = STORE_DEFINITIONS
        .into_iter()
        .find(|value| value.value() == id)
        .ok_or("use a build-heat definition ID from thermal")?;
    let inventory = project_player_inventory(registries, state).map_err(|e| e.to_string())?;
    let store = validate_assemble_energy_store(
        registries,
        state,
        definition,
        inventory.carried().stockpile(),
    )
    .map_err(|e| fire::assembly_blocker(registries, e))?
    .commit(state)
    .map_err(|e| e.to_string())?;
    Ok(format!(
        "Built {} (store {}).\n",
        registries
            .energy()
            .get_store(definition)
            .expect("authored store")
            .name(),
        store.value()
    ))
}

pub(super) fn fuel(
    registries: &Registries,
    state: &mut AppState,
    args: &[&str],
) -> Result<String, String> {
    let local = stores(state);
    let store = local[parse_index(args[0], local.len())? - 1];
    let inventory = project_player_inventory(registries, state).map_err(|e| e.to_string())?;
    let stacks = inventory.carried().stacks();
    let stack = &stacks[parse_index(args[1], stacks.len())? - 1];
    let charge = validate_combust_stack_fuel(
        registries,
        state,
        store,
        inventory.carried().stockpile(),
        stack,
    )
    .map_err(|e| fire::fuel_blocker(registries, e))?;
    let report = format!(
        "Charged heat store with {} g fuel, releasing {}.\n",
        view::grams(charge.consumed_mass()),
        fire::energy_label(charge.released_energy())
    );
    charge.commit(state).map_err(|e| e.to_string())?;
    Ok(report)
}

pub(super) fn start(
    registries: &Registries,
    state: &mut AppState,
    args: &[&str],
) -> Result<String, String> {
    let process = ProcessId::new(
        args[0]
            .parse::<u32>()
            .ok()
            .filter(|id| *id != 0)
            .ok_or("use a process ID from thermal")?,
    );
    let inventory = project_player_inventory(registries, state).map_err(|e| e.to_string())?;
    let stacks = inventory.carried().stacks();
    let stack = &stacks[parse_index(args[1], stacks.len())? - 1];
    let stations = equipment::local_stationary(state);
    let station = stations[parse_index(args[2], stations.len())? - 1];
    let local = stores(state);
    let store = local[parse_index(args[3], local.len())? - 1];
    let mass = args
        .get(4)
        .map(|value| quantity::parse_decimal_grams(value, "thermal batch mass"))
        .transpose()?
        .unwrap_or(stack.mass());
    let job = validate_start_player_thermal_batch_from_stack(
        registries, state, process, stack, mass, station, store,
    )
    .map_err(super::errors::work_start_error)?
    .commit(state)
    .map_err(|e| e.to_string())?;
    let record = state
        .production()
        .get_job(job)
        .expect("started thermal batch");
    let duration = record.completes_at().value() - state.tick().value();
    let name = registries
        .production()
        .get_process(process)
        .expect("authored process")
        .name();
    Ok(format!(
        "Started {name} (job {}). Runs unattended; scheduled completion in {}. Advance time with wait; local completion will be reported.\n",
        job.value(),
        activity::gathering_duration(duration)
    ))
}
