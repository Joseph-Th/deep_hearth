//! Regression tests for player-local text projection formatting.

#[test]
fn nonzero_vitality_and_reserves_never_display_as_exhausted() {
    use super::{reserve_label, vitality_label};
    assert_eq!(vitality_label(0), "0");
    assert_eq!(vitality_label(1), "<0.01");
    assert_eq!(vitality_label(9_100), "0.91");
    assert_eq!(vitality_label(10_000), "1");
    assert_eq!(vitality_label(1_000_000), "100");
    assert_eq!(reserve_label(0, false), "<1");
    assert_eq!(reserve_label(0, true), "0");
    assert_eq!(reserve_label(78, false), "78");
}

#[test]
fn rugged_natural_start_exposes_local_footing_without_inventing_a_route() {
    use deep_hearth::logistics::project_player_standing_space;
    use deep_hearth::world::HabitatKind;

    let mut session = super::super::Session::new_natural_in_habitat(
        7_405_532_088_482_206_663,
        Some(HabitatKind::DampGrove),
    )
    .unwrap();
    let initial = session.player_position().unwrap();
    let readout = super::look(&session.registries, &session.state, None);
    assert!(
        readout.contains("Nearby footing (not a guaranteed route): N ")
            && readout.contains(" | E ")
            && readout.contains(" | S ")
            && readout.contains(" | W "),
        "the hillside must describe all four immediate choices: {readout}"
    );
    assert_eq!(
        super::adjacent_footing(&session.registries, &session.state, 0, 1),
        "level",
        "a generated level exit should be recognizable to the player"
    );
    assert!(project_player_standing_space(
        &session.registries,
        &session.state,
        deep_hearth::spatial::VoxelCoord::new(initial.x(), initial.y(), initial.z() + 1),
    ));
    let (movement, _) = session.command("s 240").unwrap();
    assert_eq!(
        session.player_position().unwrap().z(),
        initial.z() + 1,
        "{movement}"
    );
}

#[test]
fn cold_forecast_warns_when_weather_can_change_before_or_at_the_estimated_warning() {
    use deep_hearth::core::time::TickSpan;

    let until_front = Some(TickSpan::new(20));
    assert_eq!(
        super::weather_forecast_notice(Some(TickSpan::new(21)), until_front),
        "; weather may change in 1m 12s (reassess then)"
    );
    assert_eq!(
        super::weather_forecast_notice(Some(TickSpan::new(20)), until_front),
        "; weather may change in 1m 12s (reassess then)",
        "a new weather front changes the cooling rate on its first tick"
    );
    assert_eq!(
        super::weather_forecast_notice(None, until_front),
        "; weather may change in 1m 12s (reassess then)"
    );
    assert_eq!(
        super::weather_forecast_notice(Some(TickSpan::new(19)), until_front),
        "",
        "a warning before the front does not need a conditional-weather caveat"
    );
    assert_eq!(
        super::weather_forecast_notice(Some(TickSpan::new(21)), None),
        ""
    );
}

#[test]
fn cold_forecast_in_play_uses_the_survival_owners_weather_horizon() {
    use deep_hearth::survival::assess_player_unheated_comfort_runway;

    let session = super::super::Session::new(24318).unwrap();
    let forecast =
        assess_player_unheated_comfort_runway(&session.registries, &session.state).unwrap();
    let rendered = super::look(&session.registries, &session.state, None);
    assert!(
        rendered.contains("At rest without local heat, cold warning in"),
        "{rendered}"
    );
    let notice = super::weather_forecast_notice(forecast.current(), forecast.weather_changes_in());
    assert!(
        rendered.contains(&format!("cover: {:?}{notice}", forecast.overhead_cover())),
        "the visible forecast must use the authoritative weather horizon: {rendered}"
    );
}

#[test]
fn repeated_patrol_impressions_are_collapsed_without_erasing_distinct_routes() {
    let grouped = super::group_track_descriptions([
        "scout footprints at (0,-3,8), heading 1 east".to_owned(),
        "scout hurried footprints at (0,-3,9), heading 1 north".to_owned(),
        "scout footprints at (0,-3,8), heading 1 east".to_owned(),
        "scout footprints at (1,-3,8), heading 1 west".to_owned(),
        "scout footprints at (0,-3,8), heading 1 east".to_owned(),
    ]);
    assert_eq!(
        grouped,
        vec![
            ("scout footprints at (0,-3,8), heading 1 east".to_owned(), 3),
            (
                "scout hurried footprints at (0,-3,9), heading 1 north".to_owned(),
                1
            ),
            ("scout footprints at (1,-3,8), heading 1 west".to_owned(), 1),
        ]
    );
}

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
fn natural_spring_readout_reports_finite_resting_supply_without_claiming_regeneration() {
    use deep_hearth::core::quantity::Volume;

    let rate = Volume::from_microliters(125);
    let remaining = Volume::from_microliters(1_250_000);
    let report = super::spring_supply_label(remaining, rate);
    assert!(report.contains("Natural spring supply:"), "{report}");
    assert!(
        report.contains("1250 mL") && report.contains("10h 0m"),
        "{report}"
    );
    assert!(
        report.contains("equivalent to") && report.contains("finite water"),
        "{report}"
    );
    let dry = super::spring_supply_label(Volume::ZERO, rate);
    assert!(dry.contains("Dry natural spring basin here"), "{dry}");
    assert!(dry.contains("Seek another source"), "{dry}");
    let no_drain = super::spring_supply_label(remaining, Volume::ZERO);
    assert!(
        no_drain.contains("1250 mL of finite surface water"),
        "{no_drain}"
    );
}

#[test]
fn shelter_preview_uses_the_same_standing_clearance_and_grounding_as_player_motion() {
    use deep_hearth::logistics::project_player_standing_space;
    use deep_hearth::world::BlockStateId;

    let session = super::super::Session::new(24318).unwrap();
    let root = session
        .state
        .world()
        .trees()
        .next()
        .expect("generated opening has a living tree")
        .root();
    assert_eq!(
        session.state.world().block_at(root),
        Some(BlockStateId::AIR)
    );
    assert!(session.state.world().tree_trunk_at(root).is_some());
    assert!(
        !project_player_standing_space(&session.registries, &session.state, root),
        "air terrain cannot turn an embodied living trunk into open shelter space"
    );
    let player = session.player_position().unwrap();
    assert!(project_player_standing_space(
        &session.registries,
        &session.state,
        player
    ));
    let unsupported = deep_hearth::spatial::VoxelCoord::new(player.x(), player.y() + 3, player.z());
    assert!(
        !project_player_standing_space(&session.registries, &session.state, unsupported),
        "empty air above an otherwise valid standing spot has no standing support"
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
    assert_eq!(option.suggested_mass_at(&session.state), expected);
    let catalog = super::gather_options(&session.registries, &session.state, &options);
    assert!(
        catalog.contains(&format!("default {} g", super::grams(expected))),
        "{catalog}"
    );
    let limit = option.maximum_mass_at(&session.state);
    assert!(
        catalog.contains(&format!("batch limit {} g", super::grams(limit))),
        "{catalog}"
    );
    let excessive = deep_hearth::core::quantity::Mass::from_milligrams(limit.milligrams() + 1);
    let before_tick = session.state.control_tick();
    let rejected = session
        .command(&format!("gather 1 {}", super::grams(excessive)))
        .unwrap_err();
    assert!(rejected.contains(&format!("at most {} g", super::grams(limit))));
    assert_eq!(session.state.control_tick(), before_tick);
    assert!(session.state.player_work().active().is_none());
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
fn built_fire_is_reported_from_its_real_warmth_radius_without_remote_fuel_access() {
    let mut session = super::super::Session::new(24318).unwrap();
    // Follow the real player route: collect a stone ring and timber fuel.
    for command in [
        "e 20",
        "wait 10",
        "e 20",
        "wait 10",
        "e 20",
        "wait 10",
        "gather 1 1000",
        "wait work",
        "build-fire",
        "w 20",
        "wait 10",
        "s 20",
        "wait 10",
        "gather 1 1000",
        "wait work",
        "n 20",
        "wait 10",
        "e 20",
        "wait 10",
        "feed-fire 1 1",
    ] {
        session
            .command(command)
            .unwrap_or_else(|error| panic!("{command} failed: {error}"));
    }
    let stored = session
        .state
        .available_local_energy_stores()
        .next()
        .expect("the constructed fire must remain at the player's position")
        .stored();
    assert_eq!(stored, deep_hearth::content::CAMPFIRE_FUEL_THERMAL_ENERGY);
    let charge = super::super::fire::energy_label(stored);
    let local = super::status(&session.registries, &session.state);
    let projected = deep_hearth::energy::passive_dissipation_ticks_until_empty(
        &session.registries,
        deep_hearth::content::ENERGY_STONE_RING_CAMPFIRE,
        stored,
    )
    .unwrap()
    .unwrap();
    assert!(
        local.contains(&format!(
            "passive burn remaining ~{}",
            super::runway_label(Some(projected.value()))
        )),
        "{local}"
    );
    assert!(
        local.contains(&format!(
            "Local warmth: stone-ring campfire | {charge} stored"
        )),
        "{local}"
    );
    for command in ["w 20", "wait 10"] {
        session
            .command(command)
            .unwrap_or_else(|error| panic!("{command} failed: {error}"));
    }
    let adjacent = super::status(&session.registries, &session.state);
    assert!(
        adjacent.contains(&format!(
            "Adjacent warmth: stone-ring campfire at (11,2,8) | {charge} stored"
        )),
        "{adjacent}"
    );
    assert!(!adjacent.contains("Local warmth:"), "{adjacent}");
    let remote_fire = super::super::fire::catalog(&session.registries, &session.state).unwrap();
    assert!(
        remote_fire.contains("(none at your current location)"),
        "{remote_fire}"
    );
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
fn survival_runways_use_elapsed_time_instead_of_internal_tick_counts() {
    assert_eq!(super::runway_label(Some(0)), "now");
    assert_eq!(super::runway_label(Some(1)), "4s");
    assert_eq!(super::runway_label(Some(162)), "9m 44s");
    assert_eq!(super::runway_label(Some(1_000)), "1h 0m");
    assert_eq!(super::runway_label(None), "not projected");
}
