//! Contract tests for workshop powered-crushing option planning.

use super::*;

#[test]
fn condition_lifetime_is_an_adaptive_crush_constraint() {
    let registries = build_registries();
    let variation = ScenarioVariation::from_seeds(&registries, 0x531C_3826_D24C_C39E, 1, None);
    let desired = variation.ore.nominal_batch_mass;
    let (mut state, ids, _delivery_authorization) = setup_workshop(&registries, variation);
    let _ = validate_mount_equipment(&registries, &state, ids.crusher, ids.compact_support)
        .unwrap_or_else(|error| panic!("condition-lifetime fixture mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("condition-lifetime fixture mount commit failed: {error}"));

    assert!(matches!(
        resolve_crush_option(&registries, &state, ids, desired, "small", ids.small_drive,),
        Err(CrushConstraint::ConditionLifetime)
    ));

    let adaptive = largest_resolvable_crush_batch(&registries, &state, ids, desired)
        .unwrap_or_else(|| panic!("condition-limited crusher should admit a smaller batch"));
    assert!(adaptive.mass < desired);
    assert!(adaptive.options.has_viable_option());
    assert!(adaptive.desired_constraints.condition_lifetime);
}
