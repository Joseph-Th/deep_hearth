//! Read-only drink planning contracts.

use super::*;
use crate::survival::{DrinkHydrationProjectionError, project_minimum_drink_to_hydration_target};

#[test]
fn minimum_drink_projection_prices_its_own_consumption_time() {
    let registries = build_registries();
    let physiology = registries.survival().physiology();
    let drink = registries
        .survival()
        .get_drink(FLUID_WATER)
        .copied()
        .unwrap_or_else(|| panic!("water drink definition disappeared"));
    let current = physiology.thirsty_below();
    let target = current
        .checked_add(Volume::from_microliters(100_000))
        .unwrap_or_else(|| panic!("drink projection target overflowed"));

    let projection = project_minimum_drink_to_hydration_target(physiology, drink, current, target)
        .unwrap_or_else(|error| panic!("minimum drink projection failed: {error}"))
        .unwrap_or_else(|| panic!("drink projection unexpectedly needed no drink"));

    assert_eq!(projection.volume(), Volume::from_microliters(100_375));
    assert_eq!(projection.duration(), TickSpan::new(3));
    assert_eq!(
        projection.hydration_offered(),
        Volume::from_microliters(100_375)
    );
    assert_eq!(projection.hydration_after(), target);
}

#[test]
fn minimum_drink_projection_respects_authored_serving_floor() {
    let registries = build_registries();
    let physiology = registries.survival().physiology();
    let drink = registries
        .survival()
        .get_drink(FLUID_WATER)
        .copied()
        .unwrap_or_else(|| panic!("water drink definition disappeared"));
    let current = physiology.thirsty_below();
    let target = current
        .checked_add(Volume::from_microliters(1))
        .unwrap_or_else(|| panic!("minimum-serving target overflowed"));

    let projection = project_minimum_drink_to_hydration_target(physiology, drink, current, target)
        .unwrap_or_else(|error| panic!("minimum-serving drink projection failed: {error}"))
        .unwrap_or_else(|| panic!("minimum-serving projection unexpectedly needed no drink"));

    assert_eq!(
        projection.volume(),
        physiology.direct_consumption().minimum_drink_volume()
    );
    assert!(projection.hydration_after() >= target);
}

#[test]
fn minimum_drink_projection_matches_canonical_execution() {
    let registries = build_registries();
    let physiology = registries.survival().physiology();
    let drink = registries
        .survival()
        .get_drink(FLUID_WATER)
        .copied()
        .unwrap_or_else(|| panic!("water drink definition disappeared"));
    let current = physiology.thirsty_below();
    let target = current
        .checked_add(Volume::from_microliters(100_000))
        .unwrap_or_else(|| panic!("drink execution target overflowed"));
    let projection = project_minimum_drink_to_hydration_target(physiology, drink, current, target)
        .unwrap_or_else(|error| panic!("minimum drink projection failed: {error}"))
        .unwrap_or_else(|| panic!("drink execution unexpectedly needed no drink"));

    let mut state = AppState::new(WorldSeed::new(0x5A70_0030));
    let store = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        projection.volume(),
        FLUID_WATER,
        projection.volume(),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("drink projection water fixture failed: {error}"));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("drink projection survival setup failed: {error}"));
    let player = state
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("drink projection player disappeared"));
    let expected_revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        expected_revision,
        expected_revision + 1,
        player_record(
            player.metabolic_energy(),
            current,
            player.vitality(),
            player.nutrition(),
            player.vitality_recovery_remainder(),
        ),
    );

    let outcome = validate_drink(&registries, &state, store, projection.volume())
        .unwrap_or_else(|error| panic!("projected drink validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("projected drink commit failed: {error}"));
    assert_eq!(
        outcome.completes_at().value() - state.tick().value(),
        projection.duration().value()
    );
    assert_eq!(
        finish_direct_consumption(&registries, &mut state),
        projection.duration().value()
    );
    assert_eq!(
        assess_survival(&registries, &state)
            .unwrap_or_else(|| panic!("drink projection player disappeared after execution"))
            .hydration(),
        projection.hydration_after()
    );
}

#[test]
fn minimum_drink_projection_enforces_current_hydration_bounds_exactly() {
    let registries = build_registries();
    let physiology = registries.survival().physiology();
    let drink = registries
        .survival()
        .get_drink(FLUID_WATER)
        .copied()
        .unwrap_or_else(|| panic!("water drink definition disappeared"));
    let maximum = physiology.maximum_hydration();

    assert_eq!(
        project_minimum_drink_to_hydration_target(physiology, drink, maximum, maximum),
        Ok(None)
    );

    let above_maximum = maximum
        .checked_add(Volume::from_microliters(1))
        .unwrap_or_else(|| panic!("hydration-boundary fixture overflowed"));
    assert_eq!(
        project_minimum_drink_to_hydration_target(physiology, drink, above_maximum, maximum),
        Err(
            DrinkHydrationProjectionError::CurrentHydrationExceedsMaximum {
                current: above_maximum,
                maximum,
            }
        )
    );
}

#[test]
fn minimum_drink_projection_allows_exact_maximum_intake_when_it_is_required() {
    let registries = build_registries();
    let physiology = registries.survival().physiology();
    let drink = registries
        .survival()
        .get_drink(FLUID_WATER)
        .copied()
        .unwrap_or_else(|| panic!("water drink definition disappeared"));
    let direct = physiology.direct_consumption();
    let maximum_volume = direct.maximum_drink_volume();
    let maximum_duration = direct
        .drink_duration(maximum_volume)
        .unwrap_or_else(|| panic!("maximum authored drink has no duration"));
    let hydration_loss = physiology
        .hydration_loss_per_tick()
        .microliters()
        .checked_mul(maximum_duration.value())
        .unwrap_or_else(|| panic!("maximum-drink hydration loss overflowed"));
    let target = Volume::from_microliters(
        maximum_volume
            .microliters()
            .checked_sub(hydration_loss)
            .unwrap_or_else(|| panic!("maximum drink cannot cover its own hydration loss")),
    );

    let projection =
        project_minimum_drink_to_hydration_target(physiology, drink, Volume::ZERO, target)
            .unwrap_or_else(|error| panic!("exact-maximum drink projection failed: {error}"))
            .unwrap_or_else(|| {
                panic!("exact-maximum drink projection unexpectedly needed no drink")
            });

    assert_eq!(projection.volume(), maximum_volume);
    assert_eq!(projection.duration(), maximum_duration);
    assert_eq!(projection.hydration_after(), target);
}

#[test]
fn minimum_drink_projection_reports_satisfied_and_unreachable_targets() {
    let registries = build_registries();
    let physiology = registries.survival().physiology();
    let drink = registries
        .survival()
        .get_drink(FLUID_WATER)
        .copied()
        .unwrap_or_else(|| panic!("water drink definition disappeared"));
    let current = physiology.thirsty_below();

    assert_eq!(
        project_minimum_drink_to_hydration_target(physiology, drink, current, current),
        Ok(None)
    );
    assert_eq!(
        project_minimum_drink_to_hydration_target(
            physiology,
            drink,
            Volume::ZERO,
            physiology.maximum_hydration(),
        ),
        Err(
            DrinkHydrationProjectionError::TargetUnreachableWithinIntakeLimit {
                maximum_drink_volume: physiology.direct_consumption().maximum_drink_volume(),
            }
        )
    );
}
