//! Bounded shaft arbitration selection and deterministic fallback.

use super::*;

fn candidate(
    run: usize,
    coupling: u32,
    source: u64,
    destination: u64,
    delivered: u128,
    drawn: u128,
) -> ShaftPair {
    ShaftPair {
        run,
        transfer: MechanicalTransmissionReceipt {
            coupling: StructuralElementId::new(coupling),
            segments: 1,
            source: EnergyStoreId::new(source),
            destination: EnergyStoreId::new(destination),
            drawn: Energy::from_nanojoules(drawn),
            delivered: Energy::from_nanojoules(delivered),
        },
    }
}

#[test]
fn equal_aggregate_delivery_prefers_less_friction_then_stable_physical_order() {
    let runs = vec![
        ShaftRun {
            rollers: vec![StructuralElementId::new(1), StructuralElementId::new(2)],
        },
        ShaftRun {
            rollers: vec![StructuralElementId::new(1)],
        },
        ShaftRun {
            rollers: vec![StructuralElementId::new(2)],
        },
    ];
    // One 100 J transfer conflicts with two disjoint 50 J transfers.
    // The latter pair draws less energy in total.
    let pairs = vec![
        candidate(0, 1, 1, 2, 100, 150),
        candidate(1, 2, 3, 4, 50, 60),
        candidate(2, 3, 5, 6, 50, 60),
    ];
    let selected = select_shaft_pairs(&pairs, &runs);
    assert_eq!(selected.len(), 2);
    assert_eq!(selected[0].run, 1);
    assert_eq!(selected[1].run, 2);

    // With equal useful energy and total draw, prefer the earlier ranked
    // individual candidate, not an arbitrary subset enumeration.
    let pairs = vec![
        candidate(0, 1, 1, 2, 100, 120),
        candidate(1, 2, 3, 4, 50, 60),
        candidate(2, 3, 5, 6, 50, 60),
    ];
    let selected = select_shaft_pairs(&pairs, &runs);
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].run, 0);
}

#[test]
fn crowded_shared_roller_group_uses_bounded_stable_fallback() {
    let runs = (0..=EXACT_CONFLICT_LIMIT)
        .map(|_| ShaftRun {
            rollers: vec![StructuralElementId::new(1)],
        })
        .collect::<Vec<_>>();
    let pairs = (0..=EXACT_CONFLICT_LIMIT)
        .map(|index| {
            let store = 2 * index as u64 + 1;
            candidate(
                index,
                index as u32 + 1,
                store,
                store + 1,
                (EXACT_CONFLICT_LIMIT + 1 - index) as u128,
                (EXACT_CONFLICT_LIMIT + 1 - index) as u128,
            )
        })
        .collect::<Vec<_>>();
    let selected = select_shaft_pairs(&pairs, &runs);
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].run, 0);
    assert_eq!(selected[0].transfer.delivered().nanojoules(), 17);
}

#[test]
fn weak_extra_candidate_does_not_reduce_useful_work_at_exact_solver_limit() {
    let runs = (0..=EXACT_CONFLICT_LIMIT)
        .map(|index| ShaftRun {
            rollers: match index {
                0 => vec![StructuralElementId::new(1), StructuralElementId::new(2)],
                2 => vec![StructuralElementId::new(2)],
                _ => vec![StructuralElementId::new(1)],
            },
        })
        .collect::<Vec<_>>();
    // The largest individual shaft monopolizes both rollers. Two smaller
    // shafts deliver more useful work together and draw correspondingly more.
    let mut pairs = vec![
        candidate(0, 1, 1, 2, 100, 125),
        candidate(1, 1, 3, 4, 60, 75),
        candidate(2, 2, 5, 6, 60, 75),
    ];
    for run in 3..=EXACT_CONFLICT_LIMIT {
        let store = 2 * run as u64 + 1;
        pairs.push(candidate(run, 1, store, store + 1, 1, 2));
    }
    let exact = select_shaft_pairs(&pairs[..EXACT_CONFLICT_LIMIT], &runs);
    let fallback = select_shaft_pairs(&pairs, &runs);
    let selected_runs = |selected: Vec<ShaftPair>| {
        selected
            .into_iter()
            .map(|pair| pair.run)
            .collect::<Vec<_>>()
    };
    assert_eq!(selected_runs(exact), vec![1, 2]);
    assert_eq!(selected_runs(fallback.clone()), vec![1, 2]);
    assert_eq!(
        fallback
            .iter()
            .map(|pair| pair.transfer.delivered().nanojoules())
            .sum::<u128>(),
        120
    );

    // At equal aggregate delivery, shaft friction is still a real energy
    // cost: two independent shafts should win when they draw less in total.
    pairs[0] = candidate(0, 1, 1, 2, 100, 160);
    pairs[1] = candidate(1, 1, 3, 4, 50, 60);
    pairs[2] = candidate(2, 2, 5, 6, 50, 60);
    assert_eq!(selected_runs(select_shaft_pairs(&pairs, &runs)), vec![1, 2]);
}

#[test]
fn two_shaft_replacement_recovers_three_independent_paths_in_large_groups() {
    // Two top-ranked runs consume rollers 1..4. Three smaller crossing/side
    // runs collectively deliver more, but releasing either incumbent alone
    // frees only one of the three smaller runs.
    let mut runs = vec![
        ShaftRun {
            rollers: vec![StructuralElementId::new(1), StructuralElementId::new(2)],
        },
        ShaftRun {
            rollers: vec![StructuralElementId::new(3), StructuralElementId::new(4)],
        },
        ShaftRun {
            rollers: vec![StructuralElementId::new(1), StructuralElementId::new(3)],
        },
        ShaftRun {
            rollers: vec![StructuralElementId::new(2)],
        },
        ShaftRun {
            rollers: vec![StructuralElementId::new(4)],
        },
    ];
    let mut pairs = (0..5)
        .map(|run| {
            let store = 2 * run as u64 + 1;
            let useful = if run < 2 { 80 } else { 60 };
            candidate(run, run as u32 + 1, store, store + 1, useful, useful + 20)
        })
        .collect::<Vec<_>>();
    // Weak paths sharing roller 1 connect the fallback conflict group,
    // without changing its physically best allocation.
    for run in 5..=35 {
        runs.push(ShaftRun {
            rollers: vec![StructuralElementId::new(1)],
        });
        let store = 2 * run as u64 + 1;
        pairs.push(candidate(run, run as u32 + 1, store, store + 1, 1, 2));
    }
    let selected_runs = |selected: Vec<ShaftPair>| {
        selected
            .into_iter()
            .map(|pair| pair.run)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        selected_runs(select_shaft_pairs(&pairs[..EXACT_CONFLICT_LIMIT], &runs)),
        vec![2, 3, 4]
    );
    assert_eq!(
        selected_runs(select_shaft_pairs(
            &pairs[..EXACT_CONFLICT_LIMIT + 1],
            &runs
        )),
        vec![2, 3, 4],
        "crossing the exact-solver threshold must not reduce useful work"
    );
    let crowded = select_shaft_pairs(&pairs, &runs);
    assert_eq!(selected_runs(crowded.clone()), vec![2, 3, 4]);
    assert_eq!(
        crowded
            .iter()
            .map(|pair| pair.transfer.delivered().nanojoules())
            .sum::<u128>(),
        180
    );

    // When both choices deliver 180 J, minimizing actual lost source work
    // must still outweigh the high per-shaft priority of the two incumbents.
    let mut less_friction = pairs.clone();
    less_friction[0] = candidate(0, 1, 1, 2, 90, 130);
    less_friction[1] = candidate(1, 2, 3, 4, 90, 130);
    for (run, pair) in less_friction.iter_mut().enumerate().skip(2).take(3) {
        let store = 2 * run as u64 + 1;
        *pair = candidate(run, run as u32 + 1, store, store + 1, 60, 80);
    }
    assert_eq!(
        selected_runs(select_shaft_pairs(&less_friction, &runs)),
        vec![2, 3, 4]
    );
    less_friction[0] = candidate(0, 1, 1, 2, 90, 120);
    less_friction[1] = candidate(1, 2, 3, 4, 90, 120);
    assert_eq!(
        selected_runs(select_shaft_pairs(&less_friction, &runs)),
        vec![0, 1],
        "at identical delivered work and total draw, stable priority wins"
    );
}
