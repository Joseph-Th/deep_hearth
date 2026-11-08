//! Deterministic capacitated reassignment when identity-first selection falsely rejects a recipe.
//!
//! Edges carry exact milligrams. A reversed assignment edge lets a later, more restrictive
//! ingredient displace an earlier choice through any number of intermediate requirements.
//! No per-milligram iteration, mutation of inventory, or greedy-completion retry limit is used.

use std::collections::{BTreeMap, VecDeque};

use crate::core::quantity::Mass;
use crate::material::{CommodityKey, MaterialInputSpec};

use super::super::super::state::{InventoryState, LotSlice, MaterialLotRecord, StockpileRecord};

struct Edge {
    to: usize,
    reverse: usize,
    remaining_mg: u64,
}

struct ResidualGraph {
    edges: Vec<Vec<Edge>>,
}

impl ResidualGraph {
    fn new(nodes: usize) -> Self {
        Self {
            edges: (0..nodes).map(|_| Vec::new()).collect(),
        }
    }

    fn add(&mut self, from: usize, to: usize, mass_mg: u64) {
        let reverse = self.edges[to].len();
        let forward = self.edges[from].len();
        self.edges[from].push(Edge {
            to,
            reverse,
            remaining_mg: mass_mg,
        });
        self.edges[to].push(Edge {
            to: from,
            reverse: forward,
            remaining_mg: 0,
        });
    }

    /// Find one shortest residual path, respecting original input and lot identity ordering.
    fn path(&self, source: usize, sink: usize) -> Option<Vec<(usize, usize)>> {
        let mut previous = vec![None; self.edges.len()];
        previous[source] = Some((source, 0));
        let mut queue = VecDeque::from([source]);
        while let Some(node) = queue.pop_front() {
            for (index, edge) in self.edges[node].iter().enumerate() {
                if edge.remaining_mg == 0 || previous[edge.to].is_some() {
                    continue;
                }
                previous[edge.to] = Some((node, index));
                if edge.to == sink {
                    let mut path = Vec::new();
                    let mut current = sink;
                    while current != source {
                        let (prior, via) = previous[current]?;
                        path.push((prior, via));
                        current = prior;
                    }
                    return Some(path);
                }
                queue.push_back(edge.to);
            }
        }
        None
    }

    fn maximize(&mut self, source: usize, sink: usize) {
        while let Some(path) = self.path(source, sink) {
            let mass = path
                .iter()
                .map(|&(node, edge)| self.edges[node][edge].remaining_mg)
                .min()
                .expect("residual path contains at least one edge");
            for (node, edge) in path {
                let next = self.edges[node][edge].to;
                let reverse = self.edges[node][edge].reverse;
                self.edges[node][edge].remaining_mg -= mass;
                self.edges[next][reverse].remaining_mg = self.edges[next][reverse]
                    .remaining_mg
                    .checked_add(mass)
                    .expect("reverse residual flow cannot exceed its bounded edge capacity");
            }
        }
    }
}

/// One independent network per commodity prevents unrelated materials from inflating a
/// reassignment search. A failed group rejects the whole candidate, without partial custody.
pub(super) fn reassign_eligible_lots(
    inventory: &InventoryState,
    source: &StockpileRecord,
    inputs: &[MaterialInputSpec],
    eligible: &impl Fn(&MaterialLotRecord) -> bool,
) -> Option<Vec<LotSlice>> {
    let mut groups = BTreeMap::<CommodityKey, Vec<usize>>::new();
    for (index, input) in inputs.iter().enumerate() {
        groups.entry(input.commodity()).or_default().push(index);
    }
    let mut assigned = vec![Vec::new(); inputs.len()];
    for (commodity, indices) in groups {
        let slices = allocate_commodity(inventory, source, inputs, eligible, commodity, &indices)?;
        for (index, slices) in indices.into_iter().zip(slices) {
            assigned[index] = slices;
        }
    }
    Some(assigned.into_iter().flatten().collect())
}

fn allocate_commodity(
    inventory: &InventoryState,
    source: &StockpileRecord,
    inputs: &[MaterialInputSpec],
    eligible: &impl Fn(&MaterialLotRecord) -> bool,
    commodity: CommodityKey,
    indices: &[usize],
) -> Option<Vec<Vec<LotSlice>>> {
    let lot_ids = inventory
        .lot_ids_for_commodity(source.id, commodity)
        .filter(|id| {
            let lot = inventory
                .get_lot(*id)
                .expect("trusted stockpile lot index references existing lots");
            eligible(lot)
                && indices
                    .iter()
                    .any(|&index| inputs[index].is_satisfied_by(lot.composition()))
        })
        .collect::<Vec<_>>();
    let first_lot = 1 + indices.len();
    let sink = first_lot + lot_ids.len();
    let mut graph = ResidualGraph::new(sink + 1);
    for (local, &index) in indices.iter().enumerate() {
        graph.add(0, local + 1, inputs[index].mass().milligrams());
        for (lot_index, &lot_id) in lot_ids.iter().enumerate() {
            let lot = inventory
                .get_lot(lot_id)
                .expect("trusted indexed lot remains present throughout selection");
            if inputs[index].is_satisfied_by(lot.composition()) {
                graph.add(
                    local + 1,
                    first_lot + lot_index,
                    inputs[index].mass().milligrams(),
                );
            }
        }
    }
    for (index, &lot_id) in lot_ids.iter().enumerate() {
        let mass = inventory
            .get_lot(lot_id)
            .expect("trusted indexed lot remains present throughout selection")
            .mass
            .milligrams();
        graph.add(first_lot + index, sink, mass);
    }
    graph.maximize(0, sink);
    let mut assigned = vec![Vec::new(); indices.len()];
    for (local, slices) in assigned.iter_mut().enumerate() {
        if graph.edges[0][local].remaining_mg != 0 {
            return None;
        }
        for edge in &graph.edges[local + 1] {
            if !(first_lot..sink).contains(&edge.to) {
                continue;
            }
            let mass = graph.edges[edge.to][edge.reverse].remaining_mg;
            if mass != 0 {
                slices.push(LotSlice {
                    lot: lot_ids[edge.to - first_lot],
                    mass: Mass::from_milligrams(mass),
                });
            }
        }
    }
    Some(assigned)
}
