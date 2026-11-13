//! Regression tests for player-local text projection formatting.

#[test]
fn condition_report_matches_survival_owner_reserve_percentages() {
    use deep_hearth::survival::assess_survival;

    let session = super::super::Session::new(24318).unwrap();
    let assessment = assess_survival(&session.registries, &session.state).unwrap();
    let (food_percent, water_percent) = assessment.reserve_percentages(&session.registries);
    let condition = super::status(&session.registries, &session.state);
    assert!(
        condition.contains(&format!(
            "food reserve {}%, hydration {}%",
            food_percent, water_percent
        )),
        "{condition}"
    );
}

#[test]
fn hand_status_uses_one_inventory_projection_for_label_and_carried_presence() {
    use deep_hearth::content::{FORM_FOOD, MATERIAL_BERRIES};
    use deep_hearth::logistics::PlayerActiveSelection;
    use deep_hearth::material::CommodityKey;

    let session = super::super::Session::new(24318).unwrap();
    let selected = Some(PlayerActiveSelection::material(CommodityKey::new(
        MATERIAL_BERRIES,
        FORM_FOOD,
    )));
    let (label, carried) = super::hand_status(&session.registries, &session.state, selected);
    assert!(!carried);
    assert!(label.ends_with("(none carried)"), "{label}");
    assert_eq!(
        super::hand_label(&session.registries, &session.state, selected),
        label
    );
    assert_eq!(
        super::hand_status(&session.registries, &session.state, None),
        ("empty".to_owned(), false)
    );
}

#[test]
fn signed_foot_elevation_is_correct_below_zero() {
    assert_eq!(super::meters_from_nanometers(2_225_000_000), "2.225");
    assert_eq!(super::meters_from_nanometers(-500_000_000), "-0.500");
    assert_eq!(super::meters_from_nanometers(-1_500_000_000), "-1.500");
    assert!(super::meters_from_nanometers(i64::MIN).starts_with('-'));
}

#[test]
fn celsius_safety_labels_retain_sign_and_millidegrees_at_the_limit() {
    use deep_hearth::content::{FORM_LOG, MATERIAL_WOOD};
    use deep_hearth::core::quantity::Temperature;
    use deep_hearth::material::CommodityKey;

    for (millikelvin, expected) in [
        (0, "-273.15"),
        (272_151, "-0.999"),
        (273_149, "-0.001"),
        (273_150, "0"),
        (273_151, "0.001"),
        (293_150, "20"),
        (333_150, "60"),
        (333_151, "60.001"),
    ] {
        assert_eq!(
            super::celsius(Temperature::from_millikelvin(millikelvin)),
            expected
        );
    }

    let session = super::super::Session::new(24318).unwrap();
    let unsafe_message = super::unsafe_construction_material(
        &session.registries,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Temperature::from_millikelvin(333_151),
        Temperature::from_millikelvin(333_150),
        "construction",
    );
    assert_eq!(
        unsafe_message,
        "timber log is too hot for construction (60.001 C; maximum 60 C)"
    );
}

#[test]
fn storage_catalog_names_missing_construction_material_instead_of_private_identifiers() {
    let session = super::super::Session::new(24318).unwrap();
    let text = super::super::storage::catalog(&session.registries, &session.state, true).unwrap();
    assert!(
        text.contains("rough timber field box | 10000 g capacity | blocked: need 1600 g rough timber field box body (0 g carried)"),
        "{text}"
    );
    assert!(!text.contains("commodity 65558"), "{text}");
    assert!(!text.contains("stockpile 1 has 0 mg"), "{text}");
}

#[test]
fn generated_opening_describes_real_weather_and_scent_without_debug_types() {
    let session = super::super::Session::new(24318).unwrap();
    let text = super::look(&session.registries, &session.state, None);
    assert!(
        text.contains("Weather: Spring Morning, 13 C, breeze toward east"),
        "{text}"
    );
    assert!(
        text.contains("Scent: distinct sweet fruit (no exact location)"),
        "{text}"
    );
    assert!(
        text.contains("Scent: distinct wild nuts (no exact location)"),
        "{text}"
    );
    assert!(!text.contains("Some("), "{text}");
    assert!(!text.contains("SweetFruit"), "{text}");
}

#[test]
fn campfire_without_stone_reports_a_real_material_shortfall() {
    let session = super::super::Session::new(24318).unwrap();
    let text = super::super::fire::catalog(&session.registries, &session.state).unwrap();
    assert!(
        text.contains("blocked: need 1000 g stone (0 g carried)"),
        "{text}"
    );
    assert!(!text.contains("commodity 589826"), "{text}");
}

#[test]
fn exact_gram_labels_retain_milligrams_without_trailing_zeros() {
    use deep_hearth::core::quantity::Mass;

    for (milligrams, expected) in [
        (0, "0"),
        (32_500, "32.5"),
        (17_500, "17.5"),
        (1, "0.001"),
        (12_040, "12.04"),
        (1_000_000, "1000"),
    ] {
        assert_eq!(super::grams(Mass::from_milligrams(milligrams)), expected);
    }
}

#[test]
fn observable_changes_are_named_and_weather_is_not_a_work_interruption() {
    let session = super::super::Session::new(24318).unwrap();
    let initial = super::local_cues(&session.registries, &session.state);
    assert!(initial.weather.is_some());
    assert!(!initial.scents.is_empty());
    let weather_shift = super::LocalCues {
        weather: None,
        ..initial.clone()
    };
    assert_eq!(
        initial.changed(&weather_shift),
        Some(("The wind or weather has shifted.", false)),
    );
    let scent_shift = super::LocalCues {
        scents: Vec::new(),
        ..initial.clone()
    };
    assert_eq!(
        initial.changed(&scent_shift),
        Some(("Scents in the area have shifted.", true)),
    );
    assert_eq!(initial.changed(&initial), None);
}

#[test]
fn survival_runways_use_elapsed_time_instead_of_internal_tick_counts() {
    assert_eq!(super::runway_label(Some(0)), "now");
    assert_eq!(super::runway_label(Some(1)), "4s");
    assert_eq!(super::runway_label(Some(162)), "9m 44s");
    assert_eq!(super::runway_label(Some(1_000)), "1h 0m");
    assert_eq!(super::runway_label(None), "not projected");
}
