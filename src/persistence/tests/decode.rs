//! Strict duplicate persistent map/set decoding contracts.

use super::*;

#[test]
fn duplicate_persistent_map_and_set_entries_are_rejected_during_decode() {
    let (registries, production_state) =
        make_started_test_heating_state(WorldSeed::new(0xD001_0001));
    let production_value = serde_json::to_value(SaveEnvelope::new(&registries, &production_state))
        .unwrap_or_else(|error| panic!("duplicate-job fixture serialization failed: {error}"));
    let production_json = serde_json::to_string(&production_value)
        .unwrap_or_else(|error| panic!("duplicate-job JSON serialization failed: {error}"));
    let duplicate_jobs = duplicate_first_object_entry(
        &production_json,
        "jobs",
        &production_value["state"]["systems"]["production"]["jobs"],
    );
    assert!(serde_json::from_str::<LoadedSaveEnvelope>(&duplicate_jobs).is_err());

    let registries = build_registries();
    let mut inventory_state = AppState::new(WorldSeed::new(0xD001_0002));
    let stockpile = add_solid_stockpile_for_test(&mut inventory_state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("duplicate-inventory stockpile failed: {error}"));
    deposit_bulk_for_test(
        &registries,
        &mut inventory_state,
        stockpile,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(10),
    )
    .unwrap_or_else(|error| panic!("duplicate-inventory material failed: {error}"));
    let inventory_value = serde_json::to_value(SaveEnvelope::new(&registries, &inventory_state))
        .unwrap_or_else(|error| panic!("duplicate-inventory fixture failed: {error}"));
    let inventory_json = serde_json::to_string(&inventory_value)
        .unwrap_or_else(|error| panic!("duplicate-inventory JSON failed: {error}"));
    let duplicate_contents = duplicate_first_object_entry(
        &inventory_json,
        "contents",
        &inventory_value["state"]["systems"]["inventory"]["stockpiles"]
            [stockpile.value().to_string()]["contents"],
    );
    assert!(serde_json::from_str::<LoadedSaveEnvelope>(&duplicate_contents).is_err());

    let mut structure_state = AppState::new(WorldSeed::new(0xD001_0003));
    let foundation = make_test_structural_element(&registries, &mut structure_state, 0, 0, true);
    let upper = make_test_structural_element(&registries, &mut structure_state, 0, 1, false);
    link_test_structural_support(&registries, &mut structure_state, upper, foundation);
    let structure_value = serde_json::to_value(SaveEnvelope::new(&registries, &structure_state))
        .unwrap_or_else(|error| panic!("duplicate-structure fixture failed: {error}"));
    let structure_json = serde_json::to_string(&structure_value)
        .unwrap_or_else(|error| panic!("duplicate-structure JSON failed: {error}"));
    let duplicate_loads = duplicate_first_object_entry(
        &structure_json,
        "loads",
        &structure_value["state"]["systems"]["structures"]["elements"]
            [foundation.value().to_string()]["loads"],
    );
    assert!(serde_json::from_str::<LoadedSaveEnvelope>(&duplicate_loads).is_err());

    let supports = &structure_value["state"]["systems"]["structures"]["supports_by_element"];
    let mut duplicate_supports = supports.clone();
    let support_sets = duplicate_supports
        .as_object_mut()
        .unwrap_or_else(|| panic!("support-set fixture is not an object"));
    let support_set = support_sets
        .values_mut()
        .find(|value| value.as_array().is_some_and(|values| !values.is_empty()))
        .unwrap_or_else(|| panic!("support-set fixture has no linked support"));
    let duplicate = support_set
        .as_array()
        .and_then(|values| values.first())
        .cloned()
        .unwrap_or_else(|| panic!("support-set fixture lost its linked support"));
    support_set
        .as_array_mut()
        .unwrap_or_else(|| panic!("support-set fixture changed shape"))
        .push(duplicate);
    let duplicate_support_set = replace_serialized_field(
        &structure_json,
        "supports_by_element",
        supports,
        &duplicate_supports,
    );
    assert!(serde_json::from_str::<LoadedSaveEnvelope>(&duplicate_support_set).is_err());
}
