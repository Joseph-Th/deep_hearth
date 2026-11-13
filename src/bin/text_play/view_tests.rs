//! Regression tests for player-local text projection formatting.

#[test]
fn signed_foot_elevation_is_correct_below_zero() {
    assert_eq!(super::meters_from_nanometers(2_225_000_000), "2.225");
    assert_eq!(super::meters_from_nanometers(-500_000_000), "-0.500");
    assert_eq!(super::meters_from_nanometers(-1_500_000_000), "-1.500");
    assert!(super::meters_from_nanometers(i64::MIN).starts_with('-'));
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
    let initial = super::LocalCues {
        resources: "stone".into(),
        water: "spring".into(),
        kobolds: "watchman".into(),
        footfalls: String::new(),
        tracks: String::new(),
        scents: String::new(),
        weather: "breeze".into(),
    };
    let weather_shift = super::LocalCues {
        weather: "calm".into(),
        ..initial.clone()
    };
    assert_eq!(
        initial.changed(&weather_shift),
        Some(("The wind or weather has shifted.", false)),
    );
    let creature_moved = super::LocalCues {
        kobolds: "watchman moved".into(),
        ..initial.clone()
    };
    assert_eq!(
        initial.changed(&creature_moved),
        Some(("A nearby kobold moved or changed behavior.", true)),
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
