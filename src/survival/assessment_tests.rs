//! Warning-horizon arithmetic for player-facing resource forecasts.

use super::warning_runway;
use crate::core::time::TickSpan;

#[test]
fn resting_warning_timing_honors_strict_threshold_and_partial_last_tick() {
    assert_eq!(warning_runway(100, 50, 10), Some(TickSpan::new(6)));
    assert_eq!(warning_runway(50, 50, 10), Some(TickSpan::new(1)));
    assert_eq!(warning_runway(49, 50, 10), Some(TickSpan::ZERO));
    assert_eq!(warning_runway(51, 50, 3), Some(TickSpan::new(1)));
    assert_eq!(warning_runway(52, 50, 3), Some(TickSpan::new(1)));
    assert_eq!(warning_runway(53, 50, 3), Some(TickSpan::new(2)));
    assert_eq!(warning_runway(0, 0, 1), Some(TickSpan::ZERO));
    assert_eq!(warning_runway(3, 0, 1), Some(TickSpan::new(3)));
    assert_eq!(warning_runway(100, 50, 0), None);
    assert_eq!(warning_runway(49, 50, 0), Some(TickSpan::ZERO));
    assert_eq!(warning_runway(u128::MAX, 1, 1), None);
}
