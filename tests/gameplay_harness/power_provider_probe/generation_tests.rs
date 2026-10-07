//! Content-relative variation contracts for ordinary power-provider workloads.

use std::collections::BTreeSet;

use deep_hearth::content::{
    ENERGY_TIMBER_FRAME_FLYWHEEL_BANK, FORM_LOG, FORM_LUMP, FORM_NATIVE_METAL, MATERIAL_COPPER,
    MATERIAL_STONE, MATERIAL_WOOD, PROCESS_CRUSH_ORE, PROCESS_POWER_SAW_WOOD_BOARDS,
};
use deep_hearth::material::CommodityKey;

use super::super::bulk_fieldwork_workload::{
    BULK_FIELDWORK_ORDER_MAX_BATCHES, BULK_FIELDWORK_ORDER_MIN_BATCHES, primitive_quarry_batch_mass,
};
use super::super::settlement_demand::{
    SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES, organic_lumber_batches,
};
use super::planning::{
    PrimitivePowerChoice, SettlementPowerChoice, reachable_mechanical_power_providers,
};
use super::*;

#[test]
fn organic_power_sample_pairs_each_workload_scale_with_inherited_survival_pressure() {
    let cases = (0_u64..4)
        .map(|seed| FocusedProbeCase::new(seed, None, FocusedProbeRole::OrganicVariation))
        .collect::<Vec<_>>();
    let primitive = cases
        .iter()
        .map(|&case| power_project_survival_start(case, PowerProjectEra::Primitive))
        .collect::<Vec<_>>();
    let settlement = cases
        .iter()
        .map(|&case| power_project_survival_start(case, PowerProjectEra::Settlement))
        .collect::<Vec<_>>();

    for starts in [&primitive, &settlement] {
        for profile in [
            StationarySurvivalStart::FullReserve,
            StationarySurvivalStart::HungerWarningBoundary,
            StationarySurvivalStart::HydrationWarningBoundary,
        ] {
            assert!(
                starts.contains(&profile),
                "bounded power-provider sampling lost inherited survival-pressure class {profile:?}"
            );
        }
    }
    assert_ne!(
        primitive, settlement,
        "primitive and settlement projects should not inherit identical pressure from one world stratum"
    );
}

#[test]
fn report_reference_power_providers_remain_members_of_the_live_buildable_market() {
    let registries = deep_hearth::content::build_registries();
    let primitive = reachable_mechanical_power_providers(
        &registries,
        [
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        ],
    );
    let settlement = reachable_mechanical_power_providers(
        &registries,
        [
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
        ],
    );
    assert!(!primitive.is_empty());
    assert!(!settlement.is_empty());
    for reference in [
        PrimitivePowerChoice::Crank,
        PrimitivePowerChoice::Treadle,
        PrimitivePowerChoice::WalkingWheel,
    ] {
        assert!(
            primitive.contains(&reference.provider()),
            "primitive report reference {:?} is no longer directly buildable from disclosed roots",
            reference
        );
    }
    for reference in [
        SettlementPowerChoice::StoneCrank,
        SettlementPowerChoice::CopperCrank,
        SettlementPowerChoice::Treadle,
        SettlementPowerChoice::TreadleDynamo,
        SettlementPowerChoice::DoubleWoundTreadleDynamo,
        SettlementPowerChoice::WalkingWheel,
    ] {
        assert!(
            settlement.contains(&reference.provider()),
            "settlement report reference {:?} is no longer directly buildable from disclosed roots",
            reference
        );
    }
}

#[test]
fn organic_power_workload_sampling_varies_projects_without_consulting_provider_outcomes() {
    let registries = deep_hearth::content::build_registries();
    let crusher = registries
        .ore_processing()
        .get_comminution(PROCESS_CRUSH_ORE)
        .unwrap_or_else(|| panic!("primitive crusher process disappeared"));
    let saw = registries
        .crafting()
        .get_powered(PROCESS_POWER_SAW_WOOD_BOARDS)
        .unwrap_or_else(|| panic!("settlement saw process disappeared"));
    let saw_batch_mass = registries
        .crafting()
        .get_manual(saw.transform())
        .map(|transform| transform.input_mass())
        .unwrap_or_else(|| panic!("settlement manual saw transform disappeared"));

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
    let settlement = (1_u64..=256)
        .map(|seed| declared_settlement_lumber_project(&registries, seed).0)
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
            .is_multiple_of(saw_batch_mass.milligrams())),
        "settlement organic projects must remain whole current lumber-production batches"
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
        .map(|mass| mass.milligrams() / saw_batch_mass.milligrams())
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
    assert!(
        settlement_units.len() > 16,
        "settlement workload variation collapsed"
    );
    assert!(
        settlement_units
            .iter()
            .all(|units| (8..=72).contains(units))
    );
    for seed in 1_u64..=256 {
        let (mass, _) = declared_settlement_lumber_project(&registries, seed);
        let batches = mass.milligrams() / saw_batch_mass.milligrams();
        assert_eq!(
            batches,
            organic_lumber_batches(
                seed & 0b11,
                mix64(seed ^ 0x5345_5454_4C55_4D42),
                SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES,
            ),
            "power-provider settlement demand diverged from lived settlement lumber generation"
        );
    }

    for root in [0_u64, 4, 0x1234_5678_9ABC_DEF0] {
        let bounded = (0_u64..4)
            .map(|offset| {
                let seed = root + offset;
                let (mass, work, workload) =
                    declared_primitive_crushing_project(&registries, seed, store_definition);
                (mass, work, workload)
            })
            .collect::<Vec<_>>();
        let routine = bounded
            .iter()
            .filter(|(_, _, workload)| *workload == PrimitiveCrushingWorkload::RoutineStockpile)
            .collect::<Vec<_>>();
        let bulk = bounded
            .iter()
            .filter(|(_, _, workload)| *workload == PrimitiveCrushingWorkload::BulkFieldwork)
            .collect::<Vec<_>>();
        assert!(
            routine
                .iter()
                .map(|(mass, _, _)| mass.milligrams())
                .collect::<BTreeSet<_>>()
                .len()
                > 1,
            "bounded primitive sampling must retain distinct routine workloads"
        );
        assert!(
            bulk.len() > 1,
            "bounded primitive sampling must retain multiple bulk workloads around the provider frontier"
        );
        let midpoint = (BULK_FIELDWORK_ORDER_MIN_BATCHES + BULK_FIELDWORK_ORDER_MAX_BATCHES) / 2;
        let bulk_batches = bulk
            .iter()
            .map(|(mass, _, _)| mass.milligrams() / quarry_batch.milligrams())
            .collect::<Vec<_>>();
        assert!(
            bulk_batches.iter().any(|&batches| batches <= midpoint)
                && bulk_batches.iter().any(|&batches| batches > midpoint),
            "bounded primitive sampling must span both halves of ordinary bulk fieldwork demand without selecting for provider outcome"
        );
    }
}
