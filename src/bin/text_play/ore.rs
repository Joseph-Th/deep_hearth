//! Canonical hand ore-breaking and sorting over selected carried physical cohorts.

use std::fmt::Write as _;

use super::view;

use deep_hearth::core::quantity::Mass;
use deep_hearth::logistics::project_player_inventory;
use deep_hearth::ore_processing::{
    resolve_manual_comminution_from_stack, resolve_manual_constituent_separation_from_stack,
    validate_start_manual_comminution, validate_start_manual_constituent_separation,
};
use deep_hearth::production::ProcessId;
use deep_hearth::{AppState, Registries};

fn process_name(registries: &Registries, process: ProcessId) -> &str {
    registries
        .production()
        .get_process(process)
        .map_or("unidentified process", |definition| definition.name())
}

pub fn catalog(registries: &Registries, state: &AppState) -> Result<String, String> {
    let inventory =
        project_player_inventory(registries, state).map_err(|error| error.to_string())?;
    let stacks = inventory.carried().stacks();
    let mut output = String::from("Hand ore processing (dress PROCESS STACK GRAMS):\n");
    for definition in registries.ore_processing().manual_comminution_definitions() {
        let _ = writeln!(
            output,
            "  {}. {} | input {} | max {} g per batch",
            definition.process().value(),
            process_name(registries, definition.process()),
            registries
                .materials()
                .get_form(definition.input_form())
                .map_or("unidentified form", |form| form.name()),
            view::grams(definition.max_batch_mass()),
        );
        for (index, stack) in stacks.iter().enumerate() {
            if stack.commodity().form() == definition.input_form() {
                let _ = writeln!(
                    output,
                    "     Candidate stack {}: {} | {} g",
                    index + 1,
                    registries
                        .materials()
                        .commodity_name(stack.commodity())
                        .unwrap_or("unidentified material"),
                    view::grams(stack.mass()),
                );
            }
        }
    }
    for definition in registries
        .ore_processing()
        .manual_constituent_separation_definitions()
    {
        let _ = writeln!(
            output,
            "  {}. {} | input {} | max {} g per batch | {}% target recovery",
            definition.process().value(),
            process_name(registries, definition.process()),
            registries
                .materials()
                .get_form(definition.input_form())
                .map_or("unidentified form", |form| form.name()),
            view::grams(definition.operating_profile().max_batch_mass()),
            definition.target_recovery_ppm() / 10_000,
        );
        for (index, stack) in stacks.iter().enumerate() {
            if stack.commodity().form() == definition.input_form() {
                let _ = writeln!(
                    output,
                    "     Candidate stack {}: {} | {} g",
                    index + 1,
                    registries
                        .materials()
                        .commodity_name(stack.commodity())
                        .unwrap_or("unidentified material"),
                    view::grams(stack.mass()),
                );
            }
        }
    }
    output.push_str("Candidate stacks may still fail current composition, particle-size, capacity, or work admission. Outputs enter carried inventory.\n");
    Ok(output)
}

pub fn start(
    registries: &Registries,
    state: &mut AppState,
    args: &[&str],
) -> Result<String, String> {
    let [process, stack_index, grams] = args else {
        return Err("usage: dress PROCESS STACK GRAMS".to_owned());
    };
    let process = process
        .parse::<u32>()
        .map_err(|_| "process must be a positive ID")?;
    if process == 0 {
        return Err("process IDs begin at 1".to_owned());
    }
    let process = ProcessId::new(process);
    let stack_index = stack_index
        .parse::<usize>()
        .map_err(|_| "stack must be a numbered carried material")?;
    let grams = grams
        .parse::<u64>()
        .map_err(|_| "processing quantity must be whole grams")?;
    let milligrams = grams
        .checked_mul(1_000)
        .ok_or("processing quantity is too large")?;
    if milligrams == 0 {
        return Err("processing quantity must be positive".to_owned());
    }
    let inventory =
        project_player_inventory(registries, state).map_err(|error| error.to_string())?;
    let source = inventory.carried().stockpile();
    let stack = inventory
        .carried()
        .stacks()
        .get(
            stack_index
                .checked_sub(1)
                .ok_or("stack indices begin at 1")?,
        )
        .ok_or("no carried material at that stack index")?;
    let mass = Mass::from_milligrams(milligrams);
    let (job, duration, product) = if registries
        .ore_processing()
        .get_manual_comminution(process)
        .is_some()
    {
        let resolved =
            resolve_manual_comminution_from_stack(registries, state, process, stack, mass)
                .map_err(|error| error.to_string())?;
        let duration = resolved.duration().value();
        let validated =
            validate_start_manual_comminution(registries, state, &resolved, source, source)
                .map_err(|error| error.to_string())?;
        let job = validated.commit(state).map_err(|error| error.to_string())?;
        (job, duration, "crushed material")
    } else if registries
        .ore_processing()
        .get_manual_constituent_separation(process)
        .is_some()
    {
        let resolved = resolve_manual_constituent_separation_from_stack(
            registries, state, process, stack, mass,
        )
        .map_err(|error| error.to_string())?;
        let duration = resolved.duration().value();
        let validated = validate_start_manual_constituent_separation(
            registries, state, &resolved, source, source, source,
        )
        .map_err(|error| error.to_string())?;
        let job = validated.commit(state).map_err(|error| error.to_string())?;
        (job, duration, "recovered constituent and residue")
    } else {
        return Err("process is not an authored direct-labor ore operation; see 'ore'".to_owned());
    };
    Ok(format!(
        "Started {} (job {}) on {} g from stack {}; {} in {} slow ticks. Use 'wait work' to complete.\n",
        process_name(registries, process),
        job.value(),
        grams,
        stack_index,
        product,
        duration,
    ))
}
