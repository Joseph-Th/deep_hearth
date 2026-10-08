//! Content-relative variation contracts for ordinary woodworking evaluation.

use std::collections::BTreeSet;

use super::woodworking_generation::{organic_woodworking_native_copper, plan_woodworking_demand};

#[test]
fn organic_woodworking_generation_varies_demand_and_disclosed_copper_pressure() {
    let registries = deep_hearth::content::build_registries();
    let demands = (1_u64..=64)
        .map(|seed| plan_woodworking_demand(&registries, seed, true))
        .collect::<Vec<_>>();
    for demand in &demands {
        assert!(!demand.immediate_boards.is_zero());
        assert!(demand.pipeline_boards >= demand.immediate_boards);
        assert!(demand.adze_batches >= demand.immediate_scale);
        assert!(demand.saw_batches > 0);
        assert!(!demand.adze_input_mass.is_zero());
        assert!(!demand.saw_input_mass.is_zero());
    }
    assert!(
        demands
            .iter()
            .any(|demand| demand.pipeline_boards == demand.immediate_boards),
        "organic woodworking generation lost immediate-only demand"
    );
    assert!(
        demands
            .iter()
            .any(|demand| demand.pipeline_boards > demand.immediate_boards),
        "organic woodworking generation lost queued demand"
    );
    assert!(
        demands
            .iter()
            .map(|demand| (demand.immediate_scale, demand.saw_batches))
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "organic woodworking generation collapsed to one disclosed workload"
    );

    let copper = (1_u64..=32)
        .map(|seed| organic_woodworking_native_copper(&registries, seed).milligrams())
        .collect::<BTreeSet<_>>();
    assert!(
        copper.len() > 1,
        "organic woodworking generation collapsed to one inherited-copper opportunity"
    );
}

#[test]
fn four_world_woodworking_sample_spans_workload_without_encoding_copper_decisions() {
    let registries = deep_hearth::content::build_registries();
    let base = 0x51A2_7F00_u64;
    let demands = (0_u64..4)
        .map(|offset| plan_woodworking_demand(&registries, base + offset, true))
        .collect::<Vec<_>>();
    assert!(
        demands
            .iter()
            .any(|demand| demand.pipeline_boards == demand.immediate_boards),
        "four-world woodworking exploration lost an immediate-only workload"
    );
    assert!(
        demands
            .iter()
            .any(|demand| demand.pipeline_boards > demand.immediate_boards),
        "four-world woodworking exploration lost queued work"
    );
    assert!(
        demands
            .iter()
            .filter(|demand| demand.pipeline_boards > demand.immediate_boards)
            .map(|demand| demand.saw_batches)
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "bounded woodworking exploration must retain materially different queued workloads"
    );

    let copper = (0_u64..4)
        .map(|offset| organic_woodworking_native_copper(&registries, base + offset).milligrams())
        .collect::<BTreeSet<_>>();
    assert!(
        copper.len() > 1,
        "bounded woodworking exploration must retain physical copper variation without pinning route thresholds"
    );
}

#[test]
fn organic_woodworking_copper_varies_within_each_demand_stratum() {
    let registries = deep_hearth::content::build_registries();
    for stratum in 0_u64..4 {
        let values = (0_u64..16)
            .map(|entropy| {
                let seed = stratum | (entropy << 2);
                organic_woodworking_native_copper(&registries, seed).milligrams()
            })
            .collect::<BTreeSet<_>>();
        assert!(
            values.len() > 1,
            "woodworking copper opportunity became fixed by demand stratum {stratum}"
        );
    }
}
