//! Focused contracts for primitive human-power investment policy.

use super::*;

#[test]
fn primitive_treadle_requires_meaningful_attention_return() {
    let minimum = primitive_treadle_minimum_attention_return(300, 410);
    assert_eq!(minimum, 6);
    assert!(
        !primitive_treadle_clears_attention_return(522, 521, minimum),
        "one projected tick must not justify the heavier treadle investment"
    );
    assert!(
        primitive_treadle_clears_attention_return(522, 516, minimum),
        "the treadle should win once it repays the declared setup-return floor"
    );
}

#[test]
fn settlement_walking_wheel_requires_meaningful_attention_return() {
    let minimum = settlement_walking_minimum_attention_return(1_170, 1_410);
    assert_eq!(minimum, 12);
    assert!(
        !settlement_walking_clears_attention_return(1_993, 1_984, minimum),
        "nine projected ticks must not justify the heavier walking-wheel investment"
    );
    assert!(
        settlement_walking_clears_attention_return(2_024, 2_005, minimum),
        "the walking wheel should win once it clears the declared capital-return floor"
    );
}
