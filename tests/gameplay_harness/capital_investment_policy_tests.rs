//! Cross-scope contracts for the shared capital-investment attention-return policy.

use super::capital_investment_policy::{clears_attention_return, minimum_attention_return};

#[test]
fn capital_return_floor_scales_with_extra_setup_without_encoding_tuning_constants() {
    let smaller = minimum_attention_return(300, 410);
    let larger = minimum_attention_return(300, 520);

    assert!(smaller > 0);
    assert!(larger >= smaller);
}

#[test]
fn capital_return_requires_a_positive_saving_that_meets_the_computed_floor() {
    let baseline = 1_000;
    let minimum = minimum_attention_return(300, 410);
    assert!(minimum > 0 && minimum < baseline);

    assert!(!clears_attention_return(baseline, baseline, minimum));
    assert!(!clears_attention_return(
        baseline,
        baseline - (minimum - 1),
        minimum
    ));
    assert!(clears_attention_return(
        baseline,
        baseline - minimum,
        minimum
    ));
}

#[test]
fn cheaper_setup_still_requires_a_real_lifecycle_saving() {
    let minimum = minimum_attention_return(500, 400);
    assert_eq!(minimum, 0);
    assert!(!clears_attention_return(1_000, 1_000, minimum));
    assert!(clears_attention_return(1_000, 999, minimum));
}
