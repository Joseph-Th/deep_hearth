//! Cheap contracts guarding settlement organic-world and actor-policy variation.

use std::collections::BTreeSet;

use super::capital_investment_policy::CapitalInvestmentPolicy;
use super::settlement_generation::{organic_investment_policy, organic_lumber_batches};

#[test]
fn organic_settlement_generation_straddles_supplied_crossovers_and_varies_actor_policy() {
    for crossover in [4_u64, 8, 16, 32, 64, 96] {
        let demands = (0_u64..=127)
            .map(|entropy| organic_lumber_batches(entropy, Some(crossover), 192))
            .collect::<BTreeSet<_>>();
        assert!(
            demands.len() > 1,
            "organic settlement generation collapsed to one disclosed order around crossover {crossover}"
        );
        assert!(
            demands.iter().any(|batches| *batches < crossover)
                && demands.iter().any(|batches| *batches > crossover),
            "organic settlement demand must sample both sides of supplied crossover {crossover}"
        );
    }
    let no_crossover = (0_u64..=127)
        .map(|entropy| organic_lumber_batches(entropy, None, 192))
        .collect::<BTreeSet<_>>();
    assert!(
        no_crossover.len() > 1 && no_crossover.iter().all(|batches| *batches < 192),
        "settlement demand must remain varied and leave a follow-up opportunity when mechanization has no disclosed crossover"
    );
    assert!(
        (1_u64..=128)
            .map(organic_investment_policy)
            .map(CapitalInvestmentPolicy::minimum_return_ppm)
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "organic settlement actor policy collapsed to one capital-return preference"
    );
}
