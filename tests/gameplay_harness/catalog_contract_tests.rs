//! Aggregate gameplay-evidence coverage for authored runtime process catalogs.

use std::collections::BTreeSet;

use deep_hearth::content::{
    PROCESS_CAST_PURE_COPPER, PROCESS_CONCENTRATE_COPPER, PROCESS_CRUSH_ORE,
    PROCESS_FINE_GRIND_SCREEN_OVERSIZE, PROCESS_GRIND_CRUSHED_ORE, PROCESS_MELT_PURE_COPPER,
    PROCESS_SCREEN_CRUSHED_ORE, PROCESS_SEPARATE_NATIVE_COPPER, build_registries,
};

#[test]
fn gameplay_machine_process_catalog_has_evidence() {
    let registries = build_registries();
    let manual_processes = registries
        .crafting()
        .definitions()
        .map(|definition| definition.process())
        .collect::<BTreeSet<_>>();
    let actual_machine_processes = registries
        .production()
        .definitions()
        .map(|definition| definition.id())
        .filter(|process| !manual_processes.contains(process))
        .collect::<BTreeSet<_>>();
    let exercised_machine_processes = BTreeSet::from([
        PROCESS_CRUSH_ORE,
        PROCESS_GRIND_CRUSHED_ORE,
        PROCESS_SCREEN_CRUSHED_ORE,
        PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
        PROCESS_CONCENTRATE_COPPER,
        PROCESS_MELT_PURE_COPPER,
        PROCESS_CAST_PURE_COPPER,
        PROCESS_SEPARATE_NATIVE_COPPER,
    ]);

    assert_eq!(
        actual_machine_processes, exercised_machine_processes,
        "gameplay evidence coverage is stale: classify every authored non-manual production process in progression, workshop, ore, or foundry evaluation"
    );
}
