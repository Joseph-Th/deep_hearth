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
fn gathering_suggestion_respects_unreserved_capacity_and_matches_action() {
    use deep_hearth::core::quantity::Mass;
    use deep_hearth::surface::player_surface_gathering_options;

    let mut session = super::super::Session::new(24318).unwrap();
    for command in ["e 20", "wait 10"] {
        session.command(command).unwrap();
    }
    let options = player_surface_gathering_options(&session.registries, &session.state).unwrap();
    let option = options[0];
    let carrying = session
        .state
        .logistics()
        .player()
        .unwrap()
        .carried_stockpile();
    let headroom = session
        .state
        .inventory()
        .get_stockpile(carrying)
        .unwrap()
        .available_capacity();
    let expected = option
        .source_mass()
        .min(option.maximum_batch_mass())
        .min(option.direct_batch_mass())
        .min(headroom);
    assert_eq!(super::default_gather_mass(&session.state, option), expected);
    assert_eq!(super::bounded_gather_mass(option, Mass::ZERO), Mass::ZERO);
    assert_eq!(
        super::bounded_gather_mass(option, Mass::from_milligrams(125)),
        Mass::from_milligrams(125),
    );
    let catalog = super::gather_options(&session.registries, &session.state, &options);
    assert!(
        catalog.contains(&format!("default {} g", super::grams(expected))),
        "{catalog}"
    );
    let (started, _) = session.command("gather 1").unwrap();
    assert!(
        started.contains(&format!("Gathering started: {} g", super::grams(expected))),
        "{started}"
    );
}

#[test]
fn inventory_hotbar_uses_one_carried_projection_and_reports_temperature_and_freshness() {
    use deep_hearth::logistics::{PlayerActiveSelection, project_player_inventory};

    let mut session = super::super::Session::new(24318).unwrap();
    for command in ["e 20", "wait 10", "gather 1 0.125", "wait work", "select 1"] {
        session.command(command).unwrap();
    }
    let projection = project_player_inventory(&session.registries, &session.state).unwrap();
    let stack = &projection.carried().stacks()[0];
    let selected = Some(PlayerActiveSelection::material(stack.commodity()));
    assert_eq!(
        super::actor_inventory::hand_status_with_carried(
            &session.registries,
            &session.state,
            selected,
            Some(projection.carried()),
        ),
        super::hand_status(&session.registries, &session.state, selected)
    );
    let inventory = super::inventory(&session.registries, &session.state).unwrap();
    assert!(inventory.contains("slot 1: berries"), "{inventory}");
    assert!(
        inventory.contains(&format!(
            "berries | 0.125 g | {} C",
            super::celsius(stack.temperature())
        )),
        "{inventory}"
    );
    let freshness = deep_hearth::survival::assess_food_stack_freshness(
        &session.registries,
        &session.state,
        stack,
    )
    .unwrap();
    let deep_hearth::survival::FoodFreshness::Fresh { remaining, .. } = freshness else {
        panic!("newly gathered wild berries must be fresh");
    };
    assert!(
        inventory.contains(&format!(
            " | fresh; estimated spoilage in {}",
            super::runway_label(Some(remaining.value()))
        )),
        "{inventory}"
    );
}

#[test]
fn loose_and_stored_food_share_the_same_spoilage_labels() {
    use deep_hearth::core::time::TickSpan;
    use deep_hearth::survival::FoodFreshness;

    assert_eq!(
        super::freshness_label(FoodFreshness::Fresh {
            age: TickSpan::ZERO,
            remaining: TickSpan::new(162),
        }),
        "fresh; estimated spoilage in 9m 44s"
    );
    assert_eq!(
        super::freshness_label(FoodFreshness::Spoiled {
            age: TickSpan::new(163),
        }),
        "spoiled"
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
