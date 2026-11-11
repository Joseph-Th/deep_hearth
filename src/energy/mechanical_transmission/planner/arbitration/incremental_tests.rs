//! Late-path improvement and large-workshop continuation without lost shaft work.

use super::*;

fn path(index: usize, rollers: &[u32], delivered: u128) -> (ShaftRun, ShaftPair) {
    let store = index as u64 * 2 + 1;
    (
        ShaftRun {
            rollers: rollers
                .iter()
                .copied()
                .map(StructuralElementId::new)
                .collect(),
        },
        ShaftPair {
            run: index,
            transfer: MechanicalTransmissionReceipt {
                coupling: StructuralElementId::new(index as u32 + 1),
                segments: 1,
                source: EnergyStoreId::new(store),
                destination: EnergyStoreId::new(store + 1),
                drawn: Energy::from_nanojoules(delivered + 10),
                delivered: Energy::from_nanojoules(delivered),
            },
        },
    )
}

#[test]
fn late_path_unlocks_more_work_than_two_stronger_incumbents() {
    let mut runs = Vec::new();
    let mut pairs = Vec::new();
    for (index, rollers, work) in [
        (0, vec![1, 2], 100),
        (1, vec![3, 4], 100),
        (2, vec![1, 3], 90),
        (3, vec![2], 70),
    ] {
        let (run, pair) = path(index, &rollers, work);
        runs.push(run);
        pairs.push(pair);
    }
    for index in 4..=16 {
        let (run, pair) = path(index, &[1, 2], (73 - index) as u128);
        runs.push(run);
        pairs.push(pair);
    }
    let before = select_shaft_pairs(&pairs, &runs);
    assert_eq!(
        before.iter().map(|pair| pair.run).collect::<Vec<_>>(),
        vec![0, 1]
    );

    // Candidate 18 ranks below every earlier path but completes a three-path
    // set that delivers 90 + 70 + 56 instead of the former 100 + 100.
    let (run, pair) = path(17, &[4], 56);
    runs.push(run);
    pairs.push(pair);
    let after = select_shaft_pairs(&pairs, &runs);
    assert_eq!(
        after.iter().map(|pair| pair.run).collect::<Vec<_>>(),
        vec![2, 3, 17]
    );
    assert_eq!(
        after
            .iter()
            .map(|pair| pair.transfer.delivered().nanojoules())
            .sum::<u128>(),
        216
    );
}

#[test]
fn long_connected_chain_keeps_disjoint_roller_work() {
    let count = 512;
    let (runs, pairs): (Vec<_>, Vec<_>) = (0..count)
        .map(|index| path(index, &[index as u32 + 1, index as u32 + 2], 10))
        .unzip();
    let selected = select_shaft_pairs(&pairs, &runs);
    assert_eq!(selected.len(), count / 2);
    assert_eq!(
        selected
            .iter()
            .map(|pair| pair.transfer.delivered().nanojoules())
            .sum::<u128>(),
        2_560
    );
    assert!(
        selected
            .windows(2)
            .all(|pair| pair[1].run - pair[0].run >= 2)
    );
}
