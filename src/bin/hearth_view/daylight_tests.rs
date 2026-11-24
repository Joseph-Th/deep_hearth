//! Adapter time-of-day tests use explicit frame intervals, not wall time.

use super::*;

#[test]
fn fractional_frames_accumulate_and_match_one_whole_interval() {
    let mut frames = DisplayClock::new(0.25);
    let mut whole = DisplayClock::new(0.25);
    for _ in 0..144 {
        frames.shift_hours(0.5 / 144.0);
    }
    whole.shift_hours(0.5);
    assert!((frames.day_fraction() - whole.day_fraction()).abs() < 1e-7);
    assert_ne!(
        Daylight::at(frames.day_fraction()).to_sun,
        Daylight::at(0.25).to_sun
    );
}

#[test]
fn display_clock_wraps_in_both_directions() {
    let mut clock = DisplayClock::new(0.0);
    clock.shift_hours(-0.5);
    assert!((clock.day_fraction() - 23.5 / 24.0).abs() < 1e-7);
    clock.shift_hours(0.5);
    assert_eq!(clock.day_fraction(), 0.0);
}
