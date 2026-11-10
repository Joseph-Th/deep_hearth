//! Reassignment must preserve exact flow through residual reverse edges.

use super::ResidualGraph;

#[test]
fn a_restrictive_ingredient_displaces_an_earlier_generic_lot_choice() {
    // General accepts either lot; specific accepts only the first one. The
    // identity-first shortest path initially gives the first lot to general.
    // A later alternating path must return that lot and move general to the
    // second lot, rather than incorrectly declaring a recipe shortage.
    let mut graph = ResidualGraph::new(6);
    graph.add(0, 1, 5);
    graph.add(0, 2, 5);
    graph.add(1, 3, 5);
    graph.add(1, 4, 5);
    graph.add(2, 3, 5);
    graph.add(3, 5, 5);
    graph.add(4, 5, 5);

    graph.maximize(0, 5);
    assert_eq!(graph.edges[0][0].remaining_mg, 0);
    assert_eq!(graph.edges[0][1].remaining_mg, 0);
    assert_eq!(graph.edges[1][1].remaining_mg, 5);
    assert_eq!(graph.edges[1][2].remaining_mg, 0);
    assert_eq!(graph.edges[2][1].remaining_mg, 0);
}

#[test]
fn many_one_milligram_lots_reassign_without_losing_a_single_unit() {
    const LOTS: usize = 128;
    const HALF: usize = LOTS / 2;
    const FIRST_LOT: usize = 3;
    let sink = FIRST_LOT + LOTS;
    let mut graph = ResidualGraph::new(sink + 1);
    graph.add(0, 1, HALF as u64);
    graph.add(0, 2, HALF as u64);
    for index in 0..LOTS {
        graph.add(1, FIRST_LOT + index, 1);
        if index < HALF {
            graph.add(2, FIRST_LOT + index, 1);
        }
        graph.add(FIRST_LOT + index, sink, 1);
    }

    graph.maximize(0, sink);
    assert!(graph.edges[0].iter().all(|edge| edge.remaining_mg == 0));
    assert!((FIRST_LOT..sink).all(|lot| {
        graph.edges[lot]
            .iter()
            .any(|edge| edge.to == sink && edge.remaining_mg == 0)
    }));
}
