//! Bounded crossover search used by settlement investment decisions.

use super::capital_investment_policy::clears_attention_return;

/// Finds the first disclosed workload where a candidate clears the actor's attention-return floor.
///
/// The caller supplies production-derived lifecycle attention for each workload. Returning `None`
/// means either the bounded opportunity never justifies the investment or the incumbent ceases to
/// have a comparable projection before a crossover is established.
pub(super) fn first_attention_return_crossover(
    maximum_batches: u64,
    minimum_attention_return_ticks: u64,
    mut lifecycle_attention: impl FnMut(u64) -> Option<(u64, u64)>,
) -> Option<u64> {
    for batches in 1..=maximum_batches {
        let (incumbent, candidate) = lifecycle_attention(batches)?;
        if clears_attention_return(incumbent, candidate, minimum_attention_return_ticks) {
            return Some(batches);
        }
    }
    None
}
