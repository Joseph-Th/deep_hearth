//! Heat aggregation never overflows the finite thermal comfort budget.

use super::*;

#[test]
fn local_warmth_aggregation_caps_before_energy_overflow() {
    let maximum = Energy::from_nanojoules(u128::MAX - 10);
    let almost_maximum = Energy::from_nanojoules(u128::MAX - 20);

    let first = cap_warmth(Energy::ZERO, almost_maximum, maximum);
    assert_eq!(first, almost_maximum);
    assert_eq!(
        cap_warmth(first, Energy::from_nanojoules(100), maximum),
        maximum
    );
    assert_eq!(
        cap_warmth(maximum, Energy::from_nanojoules(u128::MAX), maximum),
        maximum
    );
}
