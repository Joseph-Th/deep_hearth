//! Bounded aggregate-work arbitration for independent sets of physical shaft transfers.
//!
//! Small connected conflict groups are solved exactly. Exceptionally large
//! groups retain an exact optimum over their first sixteen ranked candidates,
//! then compare it with greedy selection and bounded replacements. Weak
//! connector removal also considers small suffix cuts: parallel paths can
//! connect independent groups without any one path being an articulation.

use super::*;

mod graph;
use graph::{conflict_groups, weak_connector_candidates};

const EXACT_CONFLICT_LIMIT: usize = 16;
/// Double replacements are at most 120 trials per round, regardless of the
/// number of candidate runs in a large generated workshop.
const DOUBLE_REPLACEMENT_INCUMBENT_LIMIT: usize = 16;
/// Probe at most four low-priority articulation paths. Dense conflicts retain
/// bounded heuristics rather than exhaustive search.
const WEAK_CONNECTOR_PROBES: usize = 4;

/// An individual transfer fits `Energy`, but independent transfers need not
/// fit in one `u128` when ranked together. Saturation would tie physically
/// unequal allocations and could prefer less delivered work just because it
/// draws less after both totals had already hit the ceiling.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct AggregateWork {
    upper: u128,
    lower: u128,
}

impl AggregateWork {
    const ZERO: Self = Self { upper: 0, lower: 0 };

    fn with_added(self, amount: u128) -> Self {
        let (lower, carry) = self.lower.overflowing_add(amount);
        // Each addition carries at most one unit into the high word, and
        // at most `usize::MAX` candidates can be held in the input slice.
        let upper = self
            .upper
            .checked_add(u128::from(carry))
            .expect("aggregate candidate count fits the high word");
        Self { upper, lower }
    }
}

#[derive(Clone, Copy)]
struct Allocation {
    mask: u32,
    delivered: AggregateWork,
    drawn: AggregateWork,
}

fn is_better(candidate: Allocation, current: Allocation) -> bool {
    if candidate.delivered != current.delivered {
        return candidate.delivered > current.delivered;
    }
    if candidate.drawn != current.drawn {
        return candidate.drawn < current.drawn;
    }
    // Candidate indices follow the stable useful-work ordering. At equal
    // physics prefer the set containing the earliest differing candidate.
    let differences = candidate.mask ^ current.mask;
    differences != 0 && candidate.mask & (1_u32 << differences.trailing_zeros()) != 0
}

fn best_independent_set(
    candidates: &[ShaftPair],
    conflicts: &[u32],
    remaining: u32,
    allocation: Allocation,
    best: &mut Allocation,
) {
    if remaining == 0 {
        if is_better(allocation, *best) {
            *best = allocation;
        }
        return;
    }
    let index = remaining.trailing_zeros() as usize;
    let bit = 1_u32 << index;
    best_independent_set(candidates, conflicts, remaining & !bit, allocation, best);
    let pair = candidates[index].transfer;
    best_independent_set(
        candidates,
        conflicts,
        remaining & !bit & !conflicts[index],
        Allocation {
            mask: allocation.mask | bit,
            delivered: allocation
                .delivered
                .with_added(pair.delivered().nanojoules()),
            drawn: allocation.drawn.with_added(pair.drawn().nanojoules()),
        },
        best,
    );
}

/// One owner key collects every candidate requiring that store or roller.
/// Construct masks in linear path-membership time rather than repeatedly
/// comparing potentially long roller runs against each other.
fn conflict_masks(candidates: &[ShaftPair], runs: &[ShaftRun]) -> Vec<u32> {
    let mut stores = BTreeMap::<EnergyStoreId, u32>::new();
    let mut rollers = BTreeMap::<StructuralElementId, u32>::new();
    for (index, pair) in candidates.iter().enumerate() {
        let bit = 1_u32 << index;
        for store in [pair.transfer.source(), pair.transfer.destination()] {
            *stores.entry(store).or_default() |= bit;
        }
        for &roller in &runs[pair.run].rollers {
            *rollers.entry(roller).or_default() |= bit;
        }
    }
    let mut blocked = vec![0_u32; candidates.len()];
    for mask in stores.values().chain(rollers.values()) {
        for (index, blocked_for) in blocked.iter_mut().enumerate() {
            if mask & (1_u32 << index) != 0 {
                *blocked_for |= mask;
            }
        }
    }
    blocked
}

fn exact_group_indices(pairs: &[ShaftPair], runs: &[ShaftRun], indices: &[usize]) -> Vec<usize> {
    let candidates = indices
        .iter()
        .map(|&index| pairs[index])
        .collect::<Vec<_>>();
    let blockers = conflict_masks(&candidates, runs);
    let mut best = Allocation {
        mask: 0,
        delivered: AggregateWork::ZERO,
        drawn: AggregateWork::ZERO,
    };
    best_independent_set(
        &candidates,
        &blockers,
        (1_u32 << candidates.len()) - 1,
        best,
        &mut best,
    );
    indices
        .iter()
        .enumerate()
        .filter_map(|(index, &original)| (best.mask & (1_u32 << index) != 0).then_some(original))
        .collect()
}

fn exact_group(pairs: &[ShaftPair], runs: &[ShaftRun], indices: &[usize]) -> Vec<ShaftPair> {
    exact_group_indices(pairs, runs, indices)
        .into_iter()
        .map(|index| pairs[index])
        .collect()
}

/// Complete a compatible set in useful-work order, leaving up to two
/// incumbents out so other physical shaft paths may use their freed assets.
fn greedy_completion(
    pairs: &[ShaftPair],
    runs: &[ShaftRun],
    indices: &[usize],
    fixed: &[usize],
    excluded: &[usize],
) -> Vec<usize> {
    let mut used_stores = BTreeSet::new();
    let mut used_rollers = BTreeSet::new();
    let mut selected = fixed.to_vec();
    for &index in fixed {
        let pair = pairs[index];
        used_stores.insert(pair.transfer.source());
        used_stores.insert(pair.transfer.destination());
        used_rollers.extend(runs[pair.run].rollers.iter().copied());
    }
    for &index in indices {
        if excluded.contains(&index) || fixed.binary_search(&index).is_ok() {
            continue;
        }
        let pair = pairs[index];
        if used_stores.contains(&pair.transfer.source())
            || used_stores.contains(&pair.transfer.destination())
            || runs[pair.run]
                .rollers
                .iter()
                .any(|id| used_rollers.contains(id))
        {
            continue;
        }
        used_stores.insert(pair.transfer.source());
        used_stores.insert(pair.transfer.destination());
        used_rollers.extend(runs[pair.run].rollers.iter().copied());
        selected.push(index);
    }
    selected.sort_unstable();
    selected
}

fn aggregate_work(pairs: &[ShaftPair], indices: &[usize]) -> (AggregateWork, AggregateWork) {
    indices.iter().fold(
        (AggregateWork::ZERO, AggregateWork::ZERO),
        |(delivered, drawn), &index| {
            let transfer = pairs[index].transfer;
            (
                delivered.with_added(transfer.delivered().nanojoules()),
                drawn.with_added(transfer.drawn().nanojoules()),
            )
        },
    )
}

fn better_aggregate_work(
    proposed: (AggregateWork, AggregateWork),
    current: (AggregateWork, AggregateWork),
) -> bool {
    proposed.0 > current.0 || (proposed.0 == current.0 && proposed.1 < current.1)
}

/// Refill available paths after releasing one or two incumbents. An equal
/// delivery with greater source draw is not an improvement.
fn improved_completion(
    pairs: &[ShaftPair],
    runs: &[ShaftRun],
    indices: &[usize],
    selected: &[usize],
    excluded: &[usize],
    current_work: (AggregateWork, AggregateWork),
) -> Option<Vec<usize>> {
    let retained = selected
        .iter()
        .copied()
        .filter(|index| !excluded.contains(index))
        .collect::<Vec<_>>();
    let alternative = greedy_completion(pairs, runs, indices, &retained, excluded);
    let alternative_work = aggregate_work(pairs, &alternative);
    better_aggregate_work(alternative_work, current_work).then_some(alternative)
}

/// Local one- and two-shaft exchanges cannot solve arbitrary dense conflicts,
/// but recover useful alternatives without unbounded subset enumeration.
fn refine_bounded_replacements(
    pairs: &[ShaftPair],
    runs: &[ShaftRun],
    indices: &[usize],
    mut selected: Vec<usize>,
) -> Vec<usize> {
    // Some layouts need two shafts freed together before three smaller
    // independent paths improve delivery. Limit pair trials to sixteen
    // incumbents and two rounds to bound each physical tick's search.
    for _ in 0..2 {
        let current_work = aggregate_work(pairs, &selected);
        let mut alternative = None;
        for incumbent in selected.iter().copied() {
            alternative =
                improved_completion(pairs, runs, indices, &selected, &[incumbent], current_work);
            if alternative.is_some() {
                break;
            }
        }
        if alternative.is_none() {
            let limit = selected.len().min(DOUBLE_REPLACEMENT_INCUMBENT_LIMIT);
            'replacement: for first in 0..limit {
                for second in first + 1..limit {
                    alternative = improved_completion(
                        pairs,
                        runs,
                        indices,
                        &selected,
                        &[selected[first], selected[second]],
                        current_work,
                    );
                    if alternative.is_some() {
                        break 'replacement;
                    }
                }
            }
        }
        let Some(improved) = alternative else {
            break;
        };
        selected = improved;
    }
    selected
}

fn greedy_group_indices(
    pairs: &[ShaftPair],
    runs: &[ShaftRun],
    indices: &[usize],
    consider_weak_connector: bool,
) -> Vec<usize> {
    let mut selected = greedy_completion(pairs, runs, indices, &[], &[]);
    // The exact solver's first sixteen ranked paths are still physically
    // available when a seventeenth, weaker path appears. Preserve that
    // solution as a bounded baseline, then greedily fill any free assets
    // with remaining candidates. This catches correlated three-or-more-path
    // exchanges without a combinatorial search over the entire large group.
    let prefix = exact_group_indices(pairs, runs, &indices[..EXACT_CONFLICT_LIMIT]);
    let baseline = greedy_completion(pairs, runs, indices, &prefix, &[]);
    if better_aggregate_work(
        aggregate_work(pairs, &baseline),
        aggregate_work(pairs, &selected),
    ) {
        selected = baseline;
    }
    selected = refine_bounded_replacements(pairs, runs, indices, selected);
    if consider_weak_connector {
        selected = refine_weak_connector_splits(pairs, runs, indices, selected);
    }
    selected
}

/// One weak path can connect independent exact-search components. Probe
/// actual articulations and bounded low-work suffix cuts: two parallel
/// bridges are not articulations, but removing both may restore a better
/// allocation in what were previously independent workshops.
fn refine_weak_connector_splits(
    pairs: &[ShaftPair],
    runs: &[ShaftRun],
    indices: &[usize],
    mut selected: Vec<usize>,
) -> Vec<usize> {
    let mut current_work = aggregate_work(pairs, &selected);
    for connector in weak_connector_candidates(pairs, runs, indices) {
        let Some(separated) = completion_without_candidates(pairs, runs, indices, &[connector])
        else {
            continue;
        };
        let separated_work = aggregate_work(pairs, &separated);
        if better_aggregate_work(separated_work, current_work) {
            selected = separated;
            current_work = separated_work;
        }
    }
    // Parallel connectors defeat one-at-a-time articulation analysis. Probe
    // only a constant number of low-work tail exclusions, not every subset.
    // Later non-bridging paths can be dropped together with the real bridges.
    for omitted in 2..=indices.len().min(WEAK_CONNECTOR_PROBES) {
        let suffix = &indices[indices.len() - omitted..];
        let Some(separated) = completion_without_candidates(pairs, runs, indices, suffix) else {
            continue;
        };
        let separated_work = aggregate_work(pairs, &separated);
        if better_aggregate_work(separated_work, current_work) {
            selected = separated;
            current_work = separated_work;
        }
    }
    selected
}

/// Excluding a bounded number of paths allows each remaining independent
/// component to be solved exactly. Larger components keep the bounded
/// fallback; no recursive candidate-removal search is attempted.
fn completion_without_candidates(
    pairs: &[ShaftPair],
    runs: &[ShaftRun],
    indices: &[usize],
    omitted: &[usize],
) -> Option<Vec<usize>> {
    let remaining = indices
        .iter()
        .copied()
        .filter(|index| !omitted.contains(index))
        .collect::<Vec<_>>();
    let candidates = remaining
        .iter()
        .map(|&index| pairs[index])
        .collect::<Vec<_>>();
    let groups = conflict_groups(&candidates, runs);
    if groups.len() < 2 {
        return None;
    }
    let mut selected = Vec::new();
    for group in groups {
        let original_indices = group
            .into_iter()
            .map(|index| remaining[index])
            .collect::<Vec<_>>();
        if original_indices.len() <= EXACT_CONFLICT_LIMIT {
            selected.extend(exact_group_indices(pairs, runs, &original_indices));
        } else {
            selected.extend(greedy_group_indices(pairs, runs, &original_indices, false));
        }
    }
    selected.sort_unstable();
    Some(selected)
}

/// Select the greatest combined useful energy for compact conflicting groups.
/// All groups are physically disjoint; large or unusually dense groups use
/// bounded local improvements rather than exponential subset enumeration.
pub(super) fn select_shaft_pairs(pairs: &[ShaftPair], runs: &[ShaftRun]) -> Vec<ShaftPair> {
    let mut selected = Vec::new();
    for indices in conflict_groups(pairs, runs) {
        if indices.len() <= EXACT_CONFLICT_LIMIT {
            selected.extend(exact_group(pairs, runs, &indices));
        } else {
            selected.extend(
                greedy_group_indices(pairs, runs, &indices, true)
                    .into_iter()
                    .map(|index| pairs[index]),
            );
        }
    }
    selected.sort_unstable_by_key(|pair| pair.run);
    selected
}

#[cfg(test)]
#[path = "arbitration_tests.rs"]
mod tests;
