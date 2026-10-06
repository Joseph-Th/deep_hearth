//! Cross-scope contracts for the shared capital-investment attention-return policy.

use super::capital_investment_crossover::first_attention_return_crossover;
use super::capital_investment_policy::{CapitalInvestmentPolicy, clears_attention_return};

#[test]
fn organic_policy_varies_investment_tolerance_without_changing_the_baseline() {
    let baseline = CapitalInvestmentPolicy::baseline();
    let eager = CapitalInvestmentPolicy::from_behavior_seed(0);
    let cautious = CapitalInvestmentPolicy::from_behavior_seed(100_000);

    assert_eq!(CapitalInvestmentPolicy::organic_bounds(), [eager, cautious]);
    assert!(eager.minimum_return_ppm() < baseline.minimum_return_ppm());
    assert!(baseline.minimum_return_ppm() < cautious.minimum_return_ppm());
    assert!(eager.minimum_attention_return(100, 300) < baseline.minimum_attention_return(100, 300));
    assert!(
        baseline.minimum_attention_return(100, 300) < cautious.minimum_attention_return(100, 300)
    );
}

#[test]
fn identical_economics_can_produce_different_actor_investment_choices() {
    let incumbent_attention = 500;
    let candidate_attention = 460;
    let incumbent_setup = 100;
    let candidate_setup = 300;
    let eager = CapitalInvestmentPolicy::from_behavior_seed(0);
    let cautious = CapitalInvestmentPolicy::from_behavior_seed(100_000);

    assert!(clears_attention_return(
        incumbent_attention,
        candidate_attention,
        eager.minimum_attention_return(incumbent_setup, candidate_setup),
    ));
    assert!(!clears_attention_return(
        incumbent_attention,
        candidate_attention,
        cautious.minimum_attention_return(incumbent_setup, candidate_setup),
    ));
}

#[test]
fn capital_return_floor_scales_with_extra_setup_without_encoding_tuning_constants() {
    let policy = CapitalInvestmentPolicy::baseline();
    let smaller = policy.minimum_attention_return(300, 410);
    let larger = policy.minimum_attention_return(300, 520);

    assert!(smaller > 0);
    assert!(larger >= smaller);
}

#[test]
fn capital_return_requires_a_positive_saving_that_meets_the_computed_floor() {
    let baseline = 1_000;
    let minimum = CapitalInvestmentPolicy::baseline().minimum_attention_return(300, 410);
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
    let minimum = CapitalInvestmentPolicy::baseline().minimum_attention_return(500, 400);
    assert_eq!(minimum, 0);
    assert!(!clears_attention_return(1_000, 1_000, minimum));
    assert!(clears_attention_return(1_000, 999, minimum));
}

#[test]
fn crossover_search_follows_live_attention_relationships_without_encoding_a_batch_count() {
    let minimum = 20;
    let crossover = first_attention_return_crossover(32, minimum, |batches| {
        Some((batches * 10, 60 + batches * 4))
    });
    let expected = (1_u64..=32)
        .find(|&batches| clears_attention_return(batches * 10, 60 + batches * 4, minimum));
    assert_eq!(crossover, expected);

    assert_eq!(
        first_attention_return_crossover(4, minimum, |batches| {
            Some((batches * 10, 60 + batches * 4))
        }),
        None
    );
}
