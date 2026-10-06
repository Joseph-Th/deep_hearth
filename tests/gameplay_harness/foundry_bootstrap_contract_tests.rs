//! Cheap first-foundry planning contracts kept off the lived probe target.

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
