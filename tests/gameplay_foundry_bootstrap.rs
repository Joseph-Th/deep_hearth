//! Focused first-foundry bootstrap target for the fast edit/test loop.

#[macro_use]
#[path = "gameplay_harness/output.rs"]
mod output;

#[path = "gameplay_harness/environment.rs"]
mod environment;
#[path = "gameplay_harness/equipment_support.rs"]
mod equipment_support;
#[path = "gameplay_harness/exact_local_runtime.rs"]
mod exact_local_runtime;
#[path = "gameplay_harness/first_foundry_probe.rs"]
mod first_foundry_probe;
#[path = "gameplay_harness/focused_case.rs"]
mod focused_case;
#[path = "gameplay_harness/focused_runner.rs"]
mod focused_runner;
#[path = "gameplay_harness/focused_seeds.rs"]
mod focused_seeds;
#[path = "gameplay_harness/focused_witnesses.rs"]
mod focused_witnesses;
#[path = "gameplay_harness/inherited_condition.rs"]
mod inherited_condition;
#[path = "gameplay_harness/inventory_support.rs"]
mod inventory_support;
#[path = "gameplay_harness/manual_craft_execution.rs"]
mod manual_craft_execution;
#[path = "gameplay_harness/manual_power_timing.rs"]
mod manual_power_timing;
#[path = "gameplay_harness/material_selection.rs"]
mod material_selection;
#[path = "gameplay_harness/ore_fixture.rs"]
mod ore_fixture;
#[path = "gameplay_harness/physical_time.rs"]
mod physical_time;
#[path = "gameplay_harness/production_timing.rs"]
mod production_timing;
#[path = "gameplay_harness/seed.rs"]
mod seed;
#[path = "gameplay_harness/seed_input.rs"]
mod seed_input;
#[path = "gameplay_harness/tick_observation.rs"]
mod tick_observation;
#[path = "gameplay_harness/workshop_craft_planning.rs"]
mod workshop_craft_planning;
#[path = "gameplay_harness/world_admission.rs"]
mod world_admission;

#[test]
fn gameplay_foundry_bootstrap_probe() {
    focused_runner::run_focused_probe(
        "foundry-bootstrap",
        first_foundry_probe::run_first_foundry_probe,
    );
}

#[test]
fn manual_recovery_planning_respects_runtime_sort_batch_rounding() {
    let registries = deep_hearth::content::build_registries();
    let sorting = registries
        .ore_processing()
        .get_manual_constituent_separation(deep_hearth::content::PROCESS_HAND_SORT_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("manual native-copper sorting definition disappeared"));
    let target = deep_hearth::core::quantity::Mass::from_milligrams(110_297);
    let copper_ppm = 588_771;
    let single_group = sorting
        .minimum_homogeneous_feed_mass_for_target_recovery(target, copper_ppm)
        .unwrap_or_else(|| panic!("single-group recovery projection disappeared"));
    let runtime_batched = sorting
        .minimum_batched_homogeneous_feed_mass_for_target_recovery(target, copper_ppm)
        .unwrap_or_else(|| panic!("batch-aware recovery projection disappeared"));

    assert!(
        runtime_batched > single_group,
        "multi-batch planning must not reuse the optimistic one-group recovery bound"
    );
    let recovered = sorting
        .project_batched_homogeneous_target_recovery(runtime_batched, copper_ppm)
        .unwrap_or_else(|| panic!("batch-aware recovery projection overflowed"));
    assert!(
        recovered >= target,
        "planned feed must recover the requested copper"
    );
    let previous_feed = runtime_batched
        .checked_sub(deep_hearth::core::quantity::Mass::from_milligrams(1))
        .unwrap_or_else(|| panic!("nonzero batch-aware feed lost its predecessor"));
    let previous_recovered = sorting
        .project_batched_homogeneous_target_recovery(previous_feed, copper_ppm)
        .unwrap_or_else(|| panic!("predecessor recovery projection overflowed"));
    assert!(
        previous_recovered < target,
        "batch-aware planner must return the smallest sufficient feed"
    );
}
