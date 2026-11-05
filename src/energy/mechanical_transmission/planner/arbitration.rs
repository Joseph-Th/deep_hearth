//! Bounded aggregate-work arbitration for independent sets of physical shaft transfers.
//!
//! Small connected conflict groups are solved exactly. Exceptionally large
//! groups use deterministic greedy selection followed by bounded replacements,
//! so adding one weak candidate does not discard obvious aggregate-work gains.

use super::*;

const EXACT_CONFLICT_LIMIT: usize = 16;

#[derive(Clone, Copy)]
struct Allocation {
    mask: u32,
    delivered: u128,
    drawn: u128,
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
                .saturating_add(pair.delivered().nanojoules()),
            drawn: allocation.drawn.saturating_add(pair.drawn().nanojoules()),
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

fn exact_group(pairs: &[ShaftPair], runs: &[ShaftRun], indices: &[usize]) -> Vec<ShaftPair> {
    let candidates = indices
        .iter()
        .map(|&index| pairs[index])
        .collect::<Vec<_>>();
    let blockers = conflict_masks(&candidates, runs);
    let mut best = Allocation {
        mask: 0,
        delivered: 0,
        drawn: 0,
    };
    best_independent_set(
        &candidates,
        &blockers,
        (1_u32 << candidates.len()) - 1,
        best,
        &mut best,
    );
    candidates
        .into_iter()
        .enumerate()
        .filter_map(|(index, pair)| (best.mask & (1_u32 << index) != 0).then_some(pair))
        .collect()
}

/// Complete a compatible set in useful-work order, optionally leaving one
/// incumbent out so other physical shaft paths may use its freed assets.
fn greedy_completion(
    pairs: &[ShaftPair],
    runs: &[ShaftRun],
    indices: &[usize],
    fixed: &[usize],
    excluded: Option<usize>,
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
        if excluded == Some(index) || fixed.binary_search(&index).is_ok() {
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

fn aggregate_work(pairs: &[ShaftPair], indices: &[usize]) -> (u128, u128) {
    indices
        .iter()
        .fold((0_u128, 0_u128), |(delivered, drawn), &index| {
            let transfer = pairs[index].transfer;
            (
                delivered.saturating_add(transfer.delivered().nanojoules()),
                drawn.saturating_add(transfer.drawn().nanojoules()),
            )
        })
}

fn greedy_group(pairs: &[ShaftPair], runs: &[ShaftRun], indices: &[usize]) -> Vec<ShaftPair> {
    let mut selected = greedy_completion(pairs, runs, indices, &[], None);
    let mut current_work = aggregate_work(pairs, &selected);
    // A large group must not suddenly lose useful work when its seventeenth
    // candidate disables the exact solver. A high-output shaft can monopolize
    // two rollers even when two independent lower-output shafts deliver more.
    // Consider at most two rounds of single-shaft replacement, retaining the
    // bounded tick cost and accepting only strictly better aggregate physics.
    for _ in 0..2 {
        let mut improved = false;
        for incumbent in selected.iter().copied() {
            let retained = selected
                .iter()
                .copied()
                .filter(|&index| index != incumbent)
                .collect::<Vec<_>>();
            let alternative = greedy_completion(pairs, runs, indices, &retained, Some(incumbent));
            let alternative_work = aggregate_work(pairs, &alternative);
            if alternative_work.0 > current_work.0
                || (alternative_work.0 == current_work.0 && alternative_work.1 < current_work.1)
            {
                selected = alternative;
                current_work = alternative_work;
                improved = true;
                break;
            }
        }
        if !improved {
            break;
        }
    }
    selected.into_iter().map(|index| pairs[index]).collect()
}

fn root(parents: &mut [usize], index: usize) -> usize {
    let mut current = index;
    while parents[current] != current {
        current = parents[current];
    }
    let root = current;
    let mut current = index;
    while parents[current] != current {
        let next = parents[current];
        parents[current] = root;
        current = next;
    }
    root
}

fn join(parents: &mut [usize], left: usize, right: usize) {
    let first = root(parents, left);
    let second = root(parents, right);
    if first != second {
        parents[second] = first;
    }
}

/// Build independent connected groups without scanning every pair against
/// every other pair. An endpoint store or embodied roller connects all
/// transfer choices that could compete for it.
fn conflict_groups(pairs: &[ShaftPair], runs: &[ShaftRun]) -> Vec<Vec<usize>> {
    let mut parents = (0..pairs.len()).collect::<Vec<_>>();
    let mut first_store = BTreeMap::new();
    let mut first_roller = BTreeMap::new();
    let mut first_run = BTreeMap::new();
    for (index, pair) in pairs.iter().enumerate() {
        for store in [pair.transfer.source(), pair.transfer.destination()] {
            if let Some(previous) = first_store.insert(store, index) {
                join(&mut parents, index, previous);
            }
        }
        if let Some(previous) = first_run.insert(pair.run, index) {
            join(&mut parents, index, previous);
        } else {
            for &roller in &runs[pair.run].rollers {
                if let Some(previous) = first_roller.insert(roller, index) {
                    join(&mut parents, index, previous);
                }
            }
        }
    }
    let mut groups = BTreeMap::<usize, Vec<usize>>::new();
    for index in 0..pairs.len() {
        groups
            .entry(root(&mut parents, index))
            .or_default()
            .push(index);
    }
    groups.into_values().collect()
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
            selected.extend(greedy_group(pairs, runs, &indices));
        }
    }
    selected.sort_unstable_by_key(|pair| pair.run);
    selected
}

#[cfg(test)]
#[path = "arbitration_tests.rs"]
mod tests;
