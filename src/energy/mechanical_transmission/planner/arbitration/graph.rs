//! Conflict topology and articulation analysis for physical shaft arbitration.
//!
//! Candidate stores, embodied rollers, and shaft paths are the only conflict
//! authorities. The graph is transient; it never owns simulation state.

use std::collections::{BTreeMap, BTreeSet};

use crate::energy::EnergyStoreId;
use crate::structural::StructuralElementId;

use super::{ShaftPair, ShaftRun, WEAK_CONNECTOR_PROBES};

/// A shared endpoint, roller, or run makes all its candidate shafts mutually
/// conflicting. A cycle connects that owner's candidates in linear space and
/// preserves their connectivity after removing any *one* candidate, unlike a
/// star whose center would become a false articulation point.
fn connect_owner_cycle(graph: &mut [Vec<usize>], members: &[usize]) {
    let mut connect = |left: usize, right: usize| {
        graph[left].push(right);
        graph[right].push(left);
    };
    for edge in members.windows(2) {
        connect(edge[0], edge[1]);
    }
    if members.len() > 2 {
        connect(members[0], members[members.len() - 1]);
    }
}

fn candidate_conflict_graph(
    pairs: &[ShaftPair],
    runs: &[ShaftRun],
    indices: &[usize],
) -> Vec<Vec<usize>> {
    let mut stores = BTreeMap::<EnergyStoreId, Vec<usize>>::new();
    let mut rollers = BTreeMap::<StructuralElementId, Vec<usize>>::new();
    let mut same_run = BTreeMap::<usize, Vec<usize>>::new();
    for (local, &original) in indices.iter().enumerate() {
        let pair = pairs[original];
        for store in [pair.transfer.source(), pair.transfer.destination()] {
            stores.entry(store).or_default().push(local);
        }
        same_run.entry(pair.run).or_default().push(local);
        for &roller in &runs[pair.run].rollers {
            rollers.entry(roller).or_default().push(local);
        }
    }
    let mut graph = vec![Vec::new(); indices.len()];
    for members in stores
        .values()
        .chain(rollers.values())
        .chain(same_run.values())
    {
        connect_owner_cycle(&mut graph, members);
    }
    graph
}

/// Iterative low-link search avoids stack overflow on long connected networks.
/// A vertex is an articulation only if removing it separates other candidates;
/// the owner cycles above prevent spurious bridges from high-degree rollers.
fn articulation_candidates(graph: &[Vec<usize>]) -> Vec<bool> {
    let mut visited = vec![0_usize; graph.len()];
    let mut low = vec![0_usize; graph.len()];
    let mut parent = vec![usize::MAX; graph.len()];
    let mut children = vec![0_usize; graph.len()];
    let mut articulation = vec![false; graph.len()];
    let mut clock = 0_usize;
    for start in 0..graph.len() {
        if visited[start] != 0 {
            continue;
        }
        clock += 1;
        visited[start] = clock;
        low[start] = clock;
        let mut stack = vec![(start, 0_usize)];
        while let Some(&(vertex, next)) = stack.last() {
            if let Some(&neighbor) = graph[vertex].get(next) {
                stack.last_mut().expect("active DFS frame").1 += 1;
                if visited[neighbor] == 0 {
                    parent[neighbor] = vertex;
                    children[vertex] += 1;
                    clock += 1;
                    visited[neighbor] = clock;
                    low[neighbor] = clock;
                    stack.push((neighbor, 0));
                } else if neighbor != parent[vertex] {
                    low[vertex] = low[vertex].min(visited[neighbor]);
                }
            } else {
                stack.pop();
                let previous = parent[vertex];
                if previous == usize::MAX {
                    articulation[vertex] = children[vertex] > 1;
                } else {
                    low[previous] = low[previous].min(low[vertex]);
                    if parent[previous] != usize::MAX && low[vertex] >= visited[previous] {
                        articulation[previous] = true;
                    }
                }
            }
        }
    }
    articulation
}

pub(super) fn weak_connector_candidates(
    pairs: &[ShaftPair],
    runs: &[ShaftRun],
    indices: &[usize],
) -> Vec<usize> {
    let graph = candidate_conflict_graph(pairs, runs, indices);
    let articulation = articulation_candidates(&graph);
    indices
        .iter()
        .enumerate()
        .rev()
        .filter_map(|(local, &original)| articulation[local].then_some(original))
        .take(WEAK_CONNECTOR_PROBES)
        .collect()
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

/// Tracks physical ownership while ranked candidates are introduced. The
/// first candidate for each run records its rollers once; additional endpoint
/// pairs share that run without rescanning its possibly long physical path.
#[derive(Default)]
struct RankedCandidateOwners {
    stores: BTreeMap<EnergyStoreId, usize>,
    rollers: BTreeMap<StructuralElementId, usize>,
    runs: BTreeMap<usize, usize>,
}

impl RankedCandidateOwners {
    fn prior_roots(
        &self,
        pair: ShaftPair,
        runs: &[ShaftRun],
        parents: &mut [usize],
    ) -> BTreeSet<usize> {
        let mut components = BTreeSet::new();
        for store in [pair.transfer.source(), pair.transfer.destination()] {
            if let Some(&previous) = self.stores.get(&store) {
                components.insert(root(parents, previous));
            }
        }
        if let Some(&previous) = self.runs.get(&pair.run) {
            components.insert(root(parents, previous));
        } else {
            for &roller in &runs[pair.run].rollers {
                if let Some(&previous) = self.rollers.get(&roller) {
                    components.insert(root(parents, previous));
                }
            }
        }
        components
    }

    fn observe(&mut self, pair: ShaftPair, runs: &[ShaftRun], local: usize) {
        for store in [pair.transfer.source(), pair.transfer.destination()] {
            self.stores.entry(store).or_insert(local);
        }
        if let std::collections::btree_map::Entry::Vacant(entry) = self.runs.entry(pair.run) {
            entry.insert(local);
            for &roller in &runs[pair.run].rollers {
                self.rollers.entry(roller).or_insert(local);
            }
        }
    }
}

/// Ranked cutoffs immediately before a low-priority candidate merges two
/// previously independent conflict components. Parallel weak connectors
/// leave no individual articulation in the final graph, but the first such
/// connector still exposes an exact pre-merge partition. Bound the number of
/// partitions examined without repeatedly rebuilding candidate prefixes.
pub(super) fn ranked_merge_prefixes(
    pairs: &[ShaftPair],
    runs: &[ShaftRun],
    indices: &[usize],
) -> Vec<usize> {
    let mut parents = (0..indices.len()).collect::<Vec<_>>();
    let mut owners = RankedCandidateOwners::default();
    let mut cutoffs = Vec::new();
    for (local, &index) in indices.iter().enumerate() {
        let pair = pairs[index];
        let prior_components = owners.prior_roots(pair, runs, &mut parents);
        if prior_components.len() > 1
            && local > super::EXACT_CONFLICT_LIMIT
            && cutoffs.len() < WEAK_CONNECTOR_PROBES
        {
            cutoffs.push(local);
        }
        for previous in prior_components {
            join(&mut parents, local, previous);
        }
        owners.observe(pair, runs, local);
    }
    cutoffs
}

/// Build independent connected groups without scanning every pair against
/// every other pair. An endpoint store or embodied roller connects all
/// transfer choices that could compete for it.
pub(super) fn conflict_groups(pairs: &[ShaftPair], runs: &[ShaftRun]) -> Vec<Vec<usize>> {
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
