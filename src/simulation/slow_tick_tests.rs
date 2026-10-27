//! Edge-case arithmetic contract for slow-tick local warmth aggregation.

use super::*;
use crate::core::quantity::Energy;

#[test]
fn local_warmth_aggregation_caps_before_energy_overflow() {
    let maximum = Energy::from_nanojoules(u128::MAX - 10);
    let almost_maximum = Energy::from_nanojoules(u128::MAX - 20);

    let first = accumulate_relevant_local_warmth(Energy::ZERO, almost_maximum, maximum);
    assert_eq!(first, almost_maximum);
    assert_eq!(
        accumulate_relevant_local_warmth(first, Energy::from_nanojoules(100), maximum),
        maximum
    );
    assert_eq!(
        accumulate_relevant_local_warmth(maximum, Energy::from_nanojoules(u128::MAX), maximum),
        maximum
    );
}
