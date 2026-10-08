//! Content-relative variation contracts for ordinary power-provider workloads.

use std::collections::BTreeSet;

use deep_hearth::content::{
    FORM_LOG, FORM_LUMP, FORM_NATIVE_METAL, MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD,
    PROCESS_CRUSH_ORE, PROCESS_POWER_SAW_WOOD_BOARDS,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::material::CommodityKey;

use super::bulk_fieldwork_workload::{
    BULK_FIELDWORK_ORDER_MAX_BATCHES, BULK_FIELDWORK_ORDER_MIN_BATCHES, primitive_quarry_batch_mass,
};
use super::power_provider_generation::{
    PowerProjectEra, PrimitiveCrushingWorkload, ROUTINE_STOCKPILE_MAX_CYCLES,
    ROUTINE_STOCKPILE_MIN_CYCLES, declared_primitive_crushing_project,
    declared_settlement_lumber_project, power_project_survival_start,
    primitive_accumulator_for_current_crusher,
};
use super::power_provider_market::{
    PrimitivePowerChoice, SettlementPowerChoice, reachable_mechanical_power_providers,
};
use super::primitive_workload::primitive_mining_cycle_mass;
use super::settlement_demand::{
    SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES, organic_lumber_batch_limits,
};
use super::stationary_survival_start::StationarySurvivalStart;

#[test]
fn organic_power_sample_pairs_each_workload_scale_with_inherited_survival_pressure() {
    let seeds = 0_u64..4;
    let primitive = seeds
        .clone()
        .map(|seed| power_project_survival_start(seed, false, PowerProjectEra::Primitive))
        .collect::<Vec<_>>();
    let settlement = seeds
        .map(|seed| power_project_survival_start(seed, false, PowerProjectEra::Settlement))
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
    assert_ne!(primitive, settlement);
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
        assert!(primitive.contains(&reference.provider()));
    }
    for reference in [
        SettlementPowerChoice::StoneCrank,
        SettlementPowerChoice::CopperCrank,
        SettlementPowerChoice::Treadle,
        SettlementPowerChoice::TreadleDynamo,
        SettlementPowerChoice::DoubleWoundTreadleDynamo,
        SettlementPowerChoice::WalkingWheel,
    ] {
        assert!(settlement.contains(&reference.provider()));
    }
}

#[test]
fn organic_power_workload_sampling_varies_each_disclosed_demand_stratum() {
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
            (seed, mass, cycle, workload)
        })
        .collect::<Vec<_>>();
    let settlement = (1_u64..=256)
        .map(|seed| {
            (
                seed,
                declared_settlement_lumber_project(&registries, seed).0,
            )
        })
        .collect::<Vec<_>>();

    let quarry_batch = primitive_quarry_batch_mass(&registries);
    for (_, mass, cycle, workload) in &primitive {
        match workload {
            PrimitiveCrushingWorkload::RoutineStockpile => {
                assert!(mass.milligrams().is_multiple_of(cycle.milligrams()));
            }
            PrimitiveCrushingWorkload::BulkFieldwork => {
                assert!(mass.milligrams().is_multiple_of(quarry_batch.milligrams()));
                let batches = mass.milligrams() / quarry_batch.milligrams();
                assert!(
                    (BULK_FIELDWORK_ORDER_MIN_BATCHES..=BULK_FIELDWORK_ORDER_MAX_BATCHES)
                        .contains(&batches)
                );
            }
        }
    }

    for stratum in 0_u64..2 {
        let units = primitive
            .iter()
            .filter(|(seed, _, _, workload)| {
                seed & 0b11 == stratum && *workload == PrimitiveCrushingWorkload::RoutineStockpile
            })
            .map(|(_, mass, cycle, _)| mass.milligrams() / cycle.milligrams())
            .collect::<BTreeSet<_>>();
        assert!(
            units.len() > 1,
            "routine workload stratum {stratum} stopped varying"
        );
        assert!(units.iter().all(|units| {
            (ROUTINE_STOCKPILE_MIN_CYCLES..=ROUTINE_STOCKPILE_MAX_CYCLES).contains(units)
        }));
    }

    let (minimum_settlement_batches, maximum_settlement_batches) =
        organic_lumber_batch_limits(SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES);
    for stratum in 0_u64..4 {
        let units = settlement
            .iter()
            .filter(|(seed, _)| seed & 0b11 == stratum)
            .map(|(_, mass)| {
                assert!(
                    mass.milligrams()
                        .is_multiple_of(saw_batch_mass.milligrams())
                );
                mass.milligrams() / saw_batch_mass.milligrams()
            })
            .collect::<BTreeSet<_>>();
        assert!(
            units.len() > 1,
            "settlement workload stratum {stratum} stopped varying"
        );
        assert!(units.iter().all(|units| {
            (minimum_settlement_batches..=maximum_settlement_batches).contains(units)
        }));
    }

    for root in [0_u64, 4, 0x1234_5678_9ABC_DEF0] {
        let bounded = (0_u64..4)
            .map(|offset| {
                let seed = root + offset;
                declared_primitive_crushing_project(&registries, seed, store_definition)
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
                > 1
        );
        let midpoint = (BULK_FIELDWORK_ORDER_MIN_BATCHES + BULK_FIELDWORK_ORDER_MAX_BATCHES) / 2;
        let bulk_batches = bulk
            .iter()
            .map(|(mass, _, _)| mass.milligrams() / quarry_batch.milligrams())
            .collect::<Vec<_>>();
        assert!(
            bulk_batches.iter().any(|&batches| batches <= midpoint)
                && bulk_batches.iter().any(|&batches| batches > midpoint)
        );
    }
}
