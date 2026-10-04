//! Content-relative variation contracts for ordinary power-provider workloads.

use std::collections::BTreeSet;

use deep_hearth::content::{
    ENERGY_TIMBER_FRAME_FLYWHEEL_BANK, PROCESS_CRUSH_ORE, PROCESS_POWER_SAW_WOOD_BOARDS,
};

use super::super::primitive_workload::{
    BULK_FIELDWORK_ORDER_MAX_BATCHES, BULK_FIELDWORK_ORDER_MIN_BATCHES, primitive_quarry_batch_mass,
};
use super::planning::{
    assert_primitive_power_provider_market_current, assert_settlement_power_provider_market_current,
};
use super::*;

#[test]
fn played_power_provider_sets_match_current_buildable_mechanical_content() {
    let registries = deep_hearth::content::build_registries();
    assert_primitive_power_provider_market_current(&registries);
    assert_settlement_power_provider_market_current(&registries);
}

#[test]
fn organic_power_workload_sampling_visits_each_declared_market_regime() {
    let registries = deep_hearth::content::build_registries();
    let crusher = registries
        .ore_processing()
        .get_comminution(PROCESS_CRUSH_ORE)
        .unwrap_or_else(|| panic!("primitive crusher process disappeared"));
    let saw = registries
        .crafting()
        .get_powered(PROCESS_POWER_SAW_WOOD_BOARDS)
        .unwrap_or_else(|| panic!("settlement saw process disappeared"));
    let bank = registries
        .energy()
        .get_store(ENERGY_TIMBER_FRAME_FLYWHEEL_BANK)
        .unwrap_or_else(|| panic!("settlement flywheel bank disappeared"));
    let saw_mass_per_bank = deep_hearth::energy::calculate_mass_specific_energy_capacity(
        bank.capacity(),
        saw.specific_energy(),
    );

    let store_definition = primitive_accumulator_for_current_crusher(&registries);
    let primitive_store = registries
        .energy()
        .get_store(store_definition)
        .unwrap_or_else(|| panic!("primitive accumulator disappeared"));
    assert!(
        deep_hearth::energy::calculate_mass_specific_energy_capacity(
            primitive_store.capacity(),
            crusher.specific_energy(),
        ) > Mass::ZERO
    );
    let primitive = (1_u64..=256)
        .map(|seed| {
            let cycle = primitive_mining_cycle_mass(&registries, seed);
            let (mass, _work, workload) =
                declared_primitive_crushing_project(&registries, seed, store_definition);
            (mass, cycle, workload)
        })
        .collect::<Vec<_>>();
    let market_regimes = [1, 8, 24];
    let settlement = (1_u64..=256)
        .map(|seed| declared_settlement_lumber_project(&registries, seed, &market_regimes).0)
        .collect::<Vec<_>>();

    let quarry_batch = primitive_quarry_batch_mass(&registries);
    for (mass, cycle, workload) in &primitive {
        match workload {
            PrimitiveCrushingWorkload::RoutineStockpile => assert!(
                mass.milligrams().is_multiple_of(cycle.milligrams()),
                "routine primitive projects must remain whole current mining/processing cycles"
            ),
            PrimitiveCrushingWorkload::BulkFieldwork => {
                assert!(mass.milligrams().is_multiple_of(quarry_batch.milligrams()));
                let batches = mass.milligrams() / quarry_batch.milligrams();
                assert!(
                    (BULK_FIELDWORK_ORDER_MIN_BATCHES..=BULK_FIELDWORK_ORDER_MAX_BATCHES)
                        .contains(&batches),
                    "bulk primitive projects must carry forward the ordinary fieldwork horizon"
                );
            }
        }
    }
    assert!(
        settlement.iter().all(|mass| mass
            .milligrams()
            .is_multiple_of(saw_mass_per_bank.milligrams())),
        "settlement organic projects must remain whole current flywheel-backed saw workloads"
    );
    assert!(
        primitive
            .iter()
            .map(|(mass, _, _)| mass.milligrams())
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "fresh primitive roots must vary declared productive work"
    );
    assert!(
        settlement
            .iter()
            .map(|mass| mass.milligrams())
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "fresh settlement roots must vary declared productive work"
    );
    let routine_units = primitive
        .iter()
        .filter(|(_, _, workload)| *workload == PrimitiveCrushingWorkload::RoutineStockpile)
        .map(|(mass, cycle, _)| mass.milligrams() / cycle.milligrams())
        .collect::<BTreeSet<_>>();
    let settlement_units = settlement
        .iter()
        .map(|mass| mass.milligrams() / saw_mass_per_bank.milligrams())
        .collect::<BTreeSet<_>>();
    assert!(routine_units.iter().all(|units| (8..=24).contains(units)));
    assert!(
        routine_units.len() > 8,
        "routine primitive workload variation collapsed"
    );
    assert!(
        primitive
            .iter()
            .any(|(_, _, workload)| *workload == PrimitiveCrushingWorkload::BulkFieldwork),
        "primitive workload sampling lost the bulk fieldwork continuation"
    );
    for (lower, upper) in [(1, 7), (8, 23), (24, u64::MAX)] {
        assert!(
            settlement_units
                .iter()
                .any(|units| (lower..=upper).contains(units)),
            "settlement workload sampling missed declared market regime beginning at {lower}"
        );
    }

    let no_settlement_frontier = (1_u64..=64)
        .map(|seed| declared_settlement_lumber_project(&registries, seed, &[]).0)
        .map(|mass| mass.milligrams() / saw_mass_per_bank.milligrams())
        .collect::<BTreeSet<_>>();
    assert!(no_settlement_frontier.len() > 1);
}
