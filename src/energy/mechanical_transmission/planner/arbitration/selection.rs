//! Incremental selection of physically compatible work through shaft conflict components.

use std::collections::BTreeSet;

use crate::energy::EnergyStoreId;
use crate::structural::StructuralElementId;

use super::graph::RankedConflictComponents;
use super::{
    DOUBLE_REPLACEMENT_INCUMBENT_LIMIT, EXACT_CONFLICT_LIMIT, ShaftPair, ShaftRun, aggregate_work,
    better_aggregate_work, exact_group_indices, greedy_group_indices, refine_bounded_replacements,
};

/// Selected work and its transient physical ownership within one component.
/// Both occupancy sets are derived solely from the selected shaft paths.
#[derive(Default)]
struct LiveAllocation {
    candidates: Vec<usize>,
    selected: Vec<usize>,
    occupied_stores: BTreeSet<EnergyStoreId>,
    occupied_rollers: BTreeSet<StructuralElementId>,
}

impl LiveAllocation {
    fn absorb(&mut self, previous: Self) {
        self.candidates.extend(previous.candidates);
        self.selected.extend(previous.selected);
        self.occupied_stores.extend(previous.occupied_stores);
        self.occupied_rollers.extend(previous.occupied_rollers);
    }

    /// Reindex exact or bounded replacements without persisting a second owner.
    fn rebuild_occupancy(&mut self, pairs: &[ShaftPair], runs: &[ShaftRun]) {
        self.occupied_stores.clear();
        self.occupied_rollers.clear();
        for &index in &self.selected {
            let pair = pairs[index];
            self.occupied_stores.insert(pair.transfer.source());
            self.occupied_stores.insert(pair.transfer.destination());
            self.occupied_rollers
                .extend(runs[pair.run].rollers.iter().copied());
        }
    }

    fn can_add(&self, pair: ShaftPair, runs: &[ShaftRun]) -> bool {
        !self.occupied_stores.contains(&pair.transfer.source())
            && !self.occupied_stores.contains(&pair.transfer.destination())
            && runs[pair.run]
                .rollers
                .iter()
                .all(|roller| !self.occupied_rollers.contains(roller))
    }

    fn add(&mut self, index: usize, pair: ShaftPair, runs: &[ShaftRun]) {
        debug_assert!(self.can_add(pair, runs));
        self.selected.push(index);
        self.occupied_stores.insert(pair.transfer.source());
        self.occupied_stores.insert(pair.transfer.destination());
        self.occupied_rollers
            .extend(runs[pair.run].rollers.iter().copied());
    }

    /// A new path cannot invalidate the independent old allocations that were
    /// feasible before this component grew. Retain their work first, then
    /// accept only strictly improving replacements or additions.
    fn incorporate(&mut self, index: usize, pairs: &[ShaftPair], runs: &[ShaftRun], merged: bool) {
        self.candidates.push(index);
        if merged {
            self.candidates.sort_unstable();
            self.selected.sort_unstable();
        }
        if self.candidates.len() <= EXACT_CONFLICT_LIMIT {
            self.selected = exact_group_indices(pairs, runs, &self.candidates);
            self.rebuild_occupancy(pairs, runs);
            return;
        }

        self.try_new_path(index, pairs, runs);
        // Expensive conflict-topology probes are relevant when components
        // merge or when an exact group first crosses the solver's threshold,
        // not on every weak addition to a long connected chain.
        if merged || self.candidates.len() == EXACT_CONFLICT_LIMIT + 1 {
            self.probe_merged_topology(pairs, runs);
        }
    }

    fn try_new_path(&mut self, index: usize, pairs: &[ShaftPair], runs: &[ShaftRun]) {
        let pair = pairs[index];
        if pair.transfer.delivered().nanojoules() > 0 && self.can_add(pair, runs) {
            self.add(index, pair, runs);
            return;
        }
        // A rejected lower-priority path can complete a previously blocked
        // multi-path exchange. Limit replacement probes by the number of
        // incumbent transfers, never by the total rejected history.
        if self.selected.len() <= DOUBLE_REPLACEMENT_INCUMBENT_LIMIT {
            let better =
                refine_bounded_replacements(pairs, runs, &self.candidates, self.selected.clone());
            if better != self.selected {
                self.selected = better;
                self.rebuild_occupancy(pairs, runs);
            }
        }
    }

    fn probe_merged_topology(&mut self, pairs: &[ShaftPair], runs: &[ShaftRun]) {
        let alternative = greedy_group_indices(pairs, runs, &self.candidates, true);
        if better_aggregate_work(
            aggregate_work(pairs, &alternative),
            aggregate_work(pairs, &self.selected),
        ) {
            self.selected = alternative;
            self.rebuild_occupancy(pairs, runs);
        }
    }
}

/// Keep previously feasible work when new shafts merge independent workshops.
/// This prevents a weak path from lowering aggregate delivered energy merely
/// by changing the shape of an otherwise productive conflict graph.
pub(in crate::energy::mechanical_transmission::planner) fn select_shaft_pairs(
    pairs: &[ShaftPair],
    runs: &[ShaftRun],
) -> Vec<ShaftPair> {
    let mut graph = RankedConflictComponents::new(pairs.len());
    let mut components = (0..pairs.len())
        .map(|_| None::<LiveAllocation>)
        .collect::<Vec<_>>();
    for (index, &pair) in pairs.iter().enumerate() {
        let (root, absorbed) = graph.insert(pair, runs, index);
        let mut live = components[root].take().unwrap_or_default();
        let merged = !absorbed.is_empty();
        for component in absorbed {
            let previous = components[component]
                .take()
                .expect("conflict union references an existing allocation");
            live.absorb(previous);
        }
        live.incorporate(index, pairs, runs, merged);
        components[root] = Some(live);
    }
    let mut selected = components
        .into_iter()
        .flatten()
        .flat_map(|component| component.selected.into_iter().map(|index| pairs[index]))
        .collect::<Vec<_>>();
    selected.sort_unstable_by_key(|pair| pair.run);
    selected
}
