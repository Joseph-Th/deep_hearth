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
fn exact_arbitration_compares_total_work_beyond_one_energy_value() {
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
    let half_plus = u128::MAX / 2 + 100;
    let pairs = vec![
        candidate(0, 1, 1, 2, u128::MAX, u128::MAX),
        candidate(1, 2, 3, 4, half_plus, half_plus),
        candidate(2, 3, 5, 6, half_plus, half_plus),
    ];
    let selected = select_shaft_pairs(&pairs, &runs);
    assert_eq!(
        selected.iter().map(|pair| pair.run).collect::<Vec<_>>(),
        vec![1, 2],
        "the two independent shafts deliver more energy than the single full-width shaft"
    );
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

#[test]
fn weak_seventeenth_path_cannot_displace_four_path_allocation_needing_three_releases() {
    // Three 80 J incumbent paths consume six distinct rollers. Four 65 J
    // paths share no stores or rollers with each other, but one crosses all
    // three incumbents. Releasing just one or two incumbents never improves
    // the greedy 240 J allocation; releasing all three yields 260 J.
    let mut runs = vec![
        ShaftRun {
            rollers: vec![StructuralElementId::new(1), StructuralElementId::new(2)],
        },
        ShaftRun {
            rollers: vec![StructuralElementId::new(3), StructuralElementId::new(4)],
        },
        ShaftRun {
            rollers: vec![StructuralElementId::new(5), StructuralElementId::new(6)],
        },
        ShaftRun {
            rollers: vec![
                StructuralElementId::new(1),
                StructuralElementId::new(3),
                StructuralElementId::new(5),
            ],
        },
        ShaftRun {
            rollers: vec![StructuralElementId::new(2)],
        },
        ShaftRun {
            rollers: vec![StructuralElementId::new(4)],
        },
        ShaftRun {
            rollers: vec![StructuralElementId::new(6)],
        },
    ];
    let mut pairs = (0..7)
        .map(|run| {
            let store = 2 * run as u64 + 1;
            let delivered = if run < 3 { 80 } else { 65 };
            candidate(
                run,
                run as u32 + 1,
                store,
                store + 1,
                delivered,
                delivered + 20,
            )
        })
        .collect::<Vec<_>>();
    for run in 7..=EXACT_CONFLICT_LIMIT {
        runs.push(ShaftRun {
            rollers: vec![StructuralElementId::new(1)],
        });
        let store = 2 * run as u64 + 1;
        pairs.push(candidate(run, run as u32 + 1, store, store + 1, 1, 2));
    }
    let chosen = |selected: Vec<ShaftPair>| {
        selected
            .into_iter()
            .map(|pair| pair.run)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        chosen(select_shaft_pairs(&pairs[..EXACT_CONFLICT_LIMIT], &runs)),
        vec![3, 4, 5, 6]
    );
    assert_eq!(
        chosen(select_shaft_pairs(&pairs, &runs)),
        vec![3, 4, 5, 6],
        "adding a weak candidate cannot reduce the physical work delivered"
    );
}

#[test]
fn weak_connector_cannot_reduce_independent_groups_total_delivery() {
    // Ten independent high-priority shafts coexist with a seven-candidate
    // conflict group whose exact optimum needs all four smaller paths.
    // A nearly useless extra shaft shares one roller with each side. It
    // must not make the former groups lose their proven useful work merely
    // by joining them into one group beyond the exact-search limit.
    let mut runs = (0..10)
        .map(|index| ShaftRun {
            rollers: vec![StructuralElementId::new(100 + index)],
        })
        .collect::<Vec<_>>();
    runs.extend([
        ShaftRun {
            rollers: vec![StructuralElementId::new(1), StructuralElementId::new(2)],
        },
        ShaftRun {
            rollers: vec![StructuralElementId::new(3), StructuralElementId::new(4)],
        },
        ShaftRun {
            rollers: vec![StructuralElementId::new(5), StructuralElementId::new(6)],
        },
        ShaftRun {
            rollers: vec![
                StructuralElementId::new(1),
                StructuralElementId::new(3),
                StructuralElementId::new(5),
            ],
        },
        ShaftRun {
            rollers: vec![StructuralElementId::new(2)],
        },
        ShaftRun {
            rollers: vec![StructuralElementId::new(4)],
        },
        ShaftRun {
            rollers: vec![StructuralElementId::new(6)],
        },
    ]);
    let mut pairs = (0..runs.len())
        .map(|run| {
            let useful = match run {
                0..10 => 100,
                10..13 => 80,
                _ => 65,
            };
            let store = 2 * run as u64 + 1;
            candidate(run, run as u32 + 1, store, store + 1, useful, useful + 20)
        })
        .collect::<Vec<_>>();
    let expected_runs = (0..10).chain(13..17).collect::<Vec<_>>();
    let chosen =
        |choices: &[ShaftPair]| choices.iter().map(|choice| choice.run).collect::<Vec<_>>();
    let original = select_shaft_pairs(&pairs, &runs);
    assert_eq!(chosen(&original), expected_runs);

    let connecting_rollers = std::iter::once(StructuralElementId::new(1))
        .chain((100..110).map(StructuralElementId::new))
        .collect::<Vec<_>>();
    runs.push(ShaftRun {
        rollers: connecting_rollers,
    });
    pairs.push(candidate(17, 18, 35, 36, 1, 2));
    let connected = select_shaft_pairs(&pairs, &runs);
    assert_eq!(
        chosen(&connected),
        expected_runs,
        "one weak connector must not reduce total delivery or displace independent shafts"
    );
    // The connector need not be last. Another even weaker run can share a
    // roller with only one side without connecting those two components.
    pairs[17] = candidate(17, 18, 35, 36, 2, 3);
    runs.push(ShaftRun {
        rollers: vec![StructuralElementId::new(100)],
    });
    pairs.push(candidate(18, 19, 37, 38, 1, 2));
    let with_weaker_tail = select_shaft_pairs(&pairs, &runs);
    assert_eq!(
        chosen(&with_weaker_tail),
        expected_runs,
        "a non-bridging lowest-ranked shaft must not hide the true weak connector"
    );
    // A crowded settlement may have many later alternatives sharing only
    // a single independent shaft. Their number should not hide the actual
    // connector by pushing it outside a fixed last-four candidate window.
    for run in 19..=26 {
        runs.push(ShaftRun {
            rollers: vec![StructuralElementId::new(100)],
        });
        let store = 2 * run as u64 + 1;
        pairs.push(candidate(run, run as u32 + 1, store, store + 1, 1, 2));
    }
    let with_many_weaker_nonbridges = select_shaft_pairs(&pairs, &runs);
    assert_eq!(
        chosen(&with_many_weaker_nonbridges),
        expected_runs,
        "non-bridging low-priority options must not hide a connector from arbitration"
    );
}

#[test]
fn articulation_probes_match_real_conflict_group_splits() {
    // Candidate ownership may overlap through stores, rollers, and duplicate
    // shaft runs. The sparse cycle graph must preserve connectivity after
    // removing any one candidate from the original conflict graph.
    let mut connected = 0;
    let mut with_bridges = 0;
    for pattern in 0..512_usize {
        // Interleave sparse chains with denser overlapping candidate owners
        // so both real articulations and non-bridging conflicts are checked.
        let chain = pattern.is_multiple_of(16);
        let runs = (0..9_usize)
            .map(|index| {
                let (primary, secondary) = if chain {
                    (index as u32 + 1, index as u32 + 2)
                } else {
                    (
                        (1 + (pattern + index * 3 + index * index) % 7) as u32,
                        (1 + (pattern / 7 + index * 5) % 7) as u32,
                    )
                };
                let primary = StructuralElementId::new(primary);
                let secondary = StructuralElementId::new(secondary);
                let mut rollers = vec![primary];
                if chain || pattern & (1 << index) != 0 {
                    rollers.push(secondary);
                }
                ShaftRun { rollers }
            })
            .collect::<Vec<_>>();
        let pairs = (0..runs.len())
            .map(|index| {
                let run = if !chain && pattern.is_multiple_of(5) && index % 4 == 3 {
                    index - 1
                } else {
                    index
                };
                let (source, destination) = if chain {
                    let store = 2 * index as u64 + 1;
                    (store, store + 1)
                } else {
                    (
                        (1 + (pattern / 11 + index * 3) % 9) as u64,
                        (10 + (pattern / 13 + index * 5) % 9) as u64,
                    )
                };
                candidate(
                    run,
                    index as u32 + 1,
                    source,
                    destination,
                    100 - index as u128,
                    110 - index as u128,
                )
            })
            .collect::<Vec<_>>();
        if conflict_groups(&pairs, &runs).len() != 1 {
            continue;
        }
        connected += 1;
        let indices = (0..pairs.len()).collect::<Vec<_>>();
        let expected = indices
            .iter()
            .rev()
            .copied()
            .filter(|&omitted| {
                let remaining = pairs
                    .iter()
                    .enumerate()
                    .filter_map(|(index, pair)| (index != omitted).then_some(*pair))
                    .collect::<Vec<_>>();
                conflict_groups(&remaining, &runs).len() > 1
            })
            .take(WEAK_CONNECTOR_PROBES)
            .collect::<Vec<_>>();
        with_bridges += usize::from(!expected.is_empty());
        assert_eq!(
            weak_connector_candidates(&pairs, &runs, &indices),
            expected,
            "incorrect articulation detection for ownership pattern {pattern}"
        );
    }
    assert!(connected > 0);
    assert!(
        with_bridges > 0,
        "test patterns must include real graph bridges"
    );
}

#[test]
fn articulation_search_handles_long_connected_shaft_chain() {
    // Physical networks may contain many paths. Low-link discovery must
    // neither recurse per path nor probe all possible removals.
    let count = 2_048_usize;
    let runs = (0..count)
        .map(|index| ShaftRun {
            rollers: vec![
                StructuralElementId::new(index as u32 + 1),
                StructuralElementId::new(index as u32 + 2),
            ],
        })
        .collect::<Vec<_>>();
    let pairs = (0..count)
        .map(|index| {
            let store = 2 * index as u64 + 1;
            candidate(index, index as u32 + 1, store, store + 1, 1, 2)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        weak_connector_candidates(&pairs, &runs, &(0..count).collect::<Vec<_>>()),
        vec![count - 2, count - 3, count - 4, count - 5]
    );
}
