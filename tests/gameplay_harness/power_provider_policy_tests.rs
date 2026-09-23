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
