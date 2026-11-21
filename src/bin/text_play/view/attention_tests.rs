//! Regression cases for visible resident-watch changes.

use super::*;

fn resident(x: i64, defending: bool) -> ResidentAttentionRecord {
    ResidentAttentionRecord {
        role: KoboldRole::TempleWarden,
        position: VoxelCoord::new(x, 0, 0),
        defending,
    }
}

#[test]
fn newly_near_resident_cannot_be_hidden_by_departure_of_another_of_the_same_role() {
    let player = VoxelCoord::new(0, 0, 0);
    let before = [resident(-1, false), resident(2, false)];
    let after = [resident(-2, false), resident(1, false)];
    assert_eq!(
        resident_attention(&before, &after, player, player),
        Some("A resident is now visible near your immediate position.")
    );
}

#[test]
fn newly_defensive_resident_cannot_be_hidden_by_another_lowering_guard() {
    let player = VoxelCoord::new(0, 0, 0);
    for (left, right) in [(-1, 1), (-1, 0)] {
        let before = [resident(left, true), resident(right, false)];
        let after = [resident(left, false), resident(right, true)];
        assert_eq!(
            resident_attention(&before, &after, player, player),
            Some("A nearby resident is visibly defensive.")
        );
    }
}

#[test]
fn ordinary_one_cell_patrol_does_not_interrupt_but_a_second_resident_does() {
    let player = VoxelCoord::new(0, 0, 0);
    for defending in [false, true] {
        assert_eq!(
            resident_attention(
                &[resident(-1, defending)],
                &[resident(0, defending)],
                player,
                player,
            ),
            None
        );
    }
    assert_eq!(
        resident_attention(
            &[resident(0, false)],
            &[resident(0, false), resident(0, false)],
            player,
            player,
        ),
        Some("A resident is now visible near your immediate position.")
    );
}

#[test]
fn daylight_watch_tolerates_passive_footprints_but_not_a_new_close_resident() {
    let mut session = super::super::super::Session::new(1170684742).unwrap();
    session.command("s 20").unwrap();
    session.command("wait 10").unwrap();
    let player = session.player_position().unwrap();
    let initial = super::local_cues(&session.registries, &session.state);
    assert_eq!(
        super::local_resident_sightings(&session.registries, &session.state),
        initial.kobolds,
        "a daylight watch and ordinary observation must share actual resident sightlines"
    );
    assert!(
        !initial.kobolds.is_empty(),
        "the seeded clearing must contain a visible resident"
    );

    // Watches and scheduled work ignore passive patrol traces, scents, and weather.
    let mut passive = initial.clone();
    passive.footfalls.clear();
    passive.tracks.clear();
    passive.scents.clear();
    passive.weather = None;
    assert!(initial.changed(&passive).is_some());
    assert_eq!(initial.watch_attention(&passive, player), None);
    assert_eq!(initial.work_attention(&passive, player), None);

    // A newly close resident is still a real reason to stop watching.
    let mut unseen = initial.clone();
    unseen.kobolds.clear();
    let resident_position = initial.kobolds[0].position();
    assert_eq!(
        unseen.watch_attention(&initial, resident_position),
        Some("A resident is now visible near your immediate position.")
    );
    assert_eq!(
        super::resident_watch_attention(
            &[],
            &initial.kobolds,
            resident_position,
            resident_position,
        ),
        unseen.watch_attention(&initial, resident_position),
        "daylight watches must use the same attention policy without surveying unrelated cues"
    );
    assert_eq!(
        unseen.work_attention(&initial, resident_position),
        Some("A resident is now visible near your immediate position.")
    );
}

#[test]
fn watch_attention_counts_same_role_residents_instead_of_treating_roles_as_identities() {
    let mut session = super::super::super::Session::new(1170684742).unwrap();
    session.command("s 20").unwrap();
    session.command("wait 10").unwrap();
    let initial = super::local_cues(&session.registries, &session.state);
    let resident = *initial
        .kobolds
        .iter()
        .find(|resident| !resident.is_defending())
        .expect("the seeded clearing contains a non-defending visible resident");
    let position = resident.position();
    let mut before = initial.clone();
    before.kobolds = vec![resident];
    let mut after = before.clone();
    // These are two observation entries of the same caste. Their shared role
    // cannot stand in for a unique resident when evaluating a changed count.
    after.kobolds.push(resident);
    assert_eq!(
        before.watch_attention(&after, position),
        Some("A resident is now visible near your immediate position.")
    );
    assert_eq!(
        before.work_attention(&after, position),
        Some("A resident is now visible near your immediate position.")
    );
    assert_eq!(after.watch_attention(&after, position), None);
}

#[test]
fn rolling_resident_watch_notices_a_close_approach_after_a_departure() {
    let mut session = super::super::super::Session::new(1170684742).unwrap();
    session.command("s 20").unwrap();
    session.command("wait 10").unwrap();
    let resident = super::local_resident_sightings(&session.registries, &session.state)
        .into_iter()
        .find(|resident| !resident.is_defending())
        .expect("the seeded clearing contains a non-defending visible resident");
    let position = resident.position();
    let mut watch = super::ResidentWatch {
        position,
        residents: vec![resident],
    };
    assert_eq!(
        watch.observe(super::ResidentWatch {
            position,
            residents: Vec::new(),
        }),
        None
    );
    assert_eq!(
        watch.observe(super::ResidentWatch {
            position,
            residents: vec![resident],
        }),
        Some("A resident is now visible near your immediate position.")
    );
}

#[test]
fn watch_attention_includes_residents_on_an_adjacent_vertical_ledge() {
    let mut session = super::super::super::Session::new(1170684742).unwrap();
    session.command("s 20").unwrap();
    session.command("wait 10").unwrap();
    let initial = super::local_cues(&session.registries, &session.state);
    let resident = *initial
        .kobolds
        .iter()
        .find(|resident| !resident.is_defending())
        .expect("the seeded clearing contains a non-defending visible resident");
    let mut unseen = initial.clone();
    unseen.kobolds.clear();
    let position = resident.position();
    let above = deep_hearth::spatial::VoxelCoord::new(position.x(), position.y() + 1, position.z());
    let below = deep_hearth::spatial::VoxelCoord::new(position.x(), position.y() - 1, position.z());
    let two_above =
        deep_hearth::spatial::VoxelCoord::new(position.x(), position.y() + 2, position.z());
    for player in [above, below] {
        assert_eq!(
            unseen.watch_attention(&initial, player),
            Some("A resident is now visible near your immediate position.")
        );
    }
    assert_eq!(unseen.watch_attention(&initial, two_above), None);
}

#[test]
fn work_wait_still_notices_changes_to_physical_food_and_water() {
    let mut session = super::super::super::Session::new(24318).unwrap();
    session.command("e 20").unwrap();
    session.command("wait 10").unwrap();
    let player = session.player_position().unwrap();
    let initial = super::local_cues(&session.registries, &session.state);
    assert!(!initial.resources.is_empty());
    let mut changed = initial.clone();
    changed.resources.clear();
    assert_eq!(
        initial.work_attention(&changed, player),
        Some("Nearby loose resources have changed.")
    );
    changed = initial.clone();
    changed.water.clear();
    assert!(!initial.water.is_empty());
    assert_eq!(initial.work_attention(&changed, player), None);
    assert_eq!(
        initial.changed(&changed),
        Some("Nearby water sightings have changed.")
    );
    assert_eq!(
        changed.work_attention(&initial, player),
        Some("New nearby water is visible.")
    );

    // Follow the real opening branch to a drinkable source.
    for command in ["s 20", "wait 10"] {
        session.command(command).unwrap();
    }
    let player = session.player_position().unwrap();
    let at_water = super::local_cues(&session.registries, &session.state);
    assert!(!at_water.local_water.is_empty());
    let catalog = super::super::drink_options(&session.registries, &session.state);
    assert!(super::super::look(&session.registries, &session.state, None).contains(&catalog));
    let mut lost_supply = at_water.clone();
    lost_supply.local_water.clear();
    assert_eq!(
        at_water.work_attention(&lost_supply, player),
        Some("Drinkable water at your position has changed.")
    );
}

#[test]
fn observable_changes_are_named_and_weather_is_not_a_work_interruption() {
    let session = super::super::super::Session::new(24318).unwrap();
    let initial = super::local_cues(&session.registries, &session.state);
    assert!(initial.weather.is_some());
    assert!(!initial.scents.is_empty());
    let weather_shift = super::LocalCues {
        weather: None,
        ..initial.clone()
    };
    assert_eq!(
        initial.changed(&weather_shift),
        Some("The wind or weather has shifted."),
    );
    let scent_shift = super::LocalCues {
        scents: Vec::new(),
        ..initial.clone()
    };
    assert_eq!(
        initial.changed(&scent_shift),
        Some("Scents in the area have shifted."),
    );
    assert_eq!(initial.changed(&initial), None);
}

#[test]
fn temperature_only_weather_front_is_observable_even_when_wind_is_unchanged() {
    use deep_hearth::core::quantity::Temperature;

    let session = super::super::super::Session::new(24318).unwrap();
    let initial = super::local_cues(&session.registries, &session.state);
    let old_weather = initial.weather.expect("generated world has local weather");
    let mut colder = initial.clone();
    colder.weather = Some(super::LocalWeatherCue {
        ambient_temperature: Temperature::from_millikelvin(
            old_weather.ambient_temperature.millikelvin() - 1_000,
        ),
        ..old_weather
    });
    assert_ne!(colder.weather, initial.weather);
    assert_eq!(
        initial.changed(&colder),
        Some("The wind or weather has shifted."),
        "a changed ambient temperature affects cold exposure and must be visible to the player"
    );
}
