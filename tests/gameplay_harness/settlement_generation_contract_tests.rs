//! Cheap contracts guarding settlement organic-world and actor-policy variation.

use std::collections::BTreeSet;

use deep_hearth::content::EQUIPMENT_TIMBER_FRAME_SAW_BENCH;
use deep_hearth::maintenance::Condition;

use super::capital_investment_policy::{CapitalInvestmentPolicy, clears_attention_return};
use super::settlement_demand::{organic_lumber_batch_limits, organic_lumber_batches};
use super::settlement_generation::{
    SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES, crossover_workloads,
    organic_inherited_equipment_condition, organic_investment_policy,
};

#[test]
fn crossover_workloads_stay_on_their_live_economic_sides_and_vary_with_entropy() {
    for crossover in [2_u64, 7, 16, 41, 96] {
        let maintained = crossover_workloads(
            crossover,
            SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES,
            None,
        );
        assert!(maintained.short_batches < crossover);
        assert_eq!(maintained.marginal_batches, crossover - 1);
        assert_eq!(maintained.project_batches, crossover);

        let varied = (0_u64..64)
            .map(|entropy| {
                crossover_workloads(
                    crossover,
                    SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES,
                    Some(entropy),
                )
            })
            .collect::<Vec<_>>();
        assert!(varied.iter().all(|workload| {
            workload.short_batches < crossover && workload.project_batches >= crossover
        }));
        if crossover > 2 {
            assert!(
                varied
                    .iter()
                    .map(|workload| workload.short_batches)
                    .collect::<BTreeSet<_>>()
                    .len()
                    > 1
            );
        }
        if crossover < SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES {
            assert!(
                varied
                    .iter()
                    .map(|workload| workload.project_batches)
                    .collect::<BTreeSet<_>>()
                    .len()
                    > 1
            );
        }
    }
}

#[test]
fn organic_settlement_generation_varies_visible_demand_without_using_investment_outcomes() {
    let baseline = CapitalInvestmentPolicy::baseline();
    let return_floor = baseline.minimum_attention_return(100, 600);
    assert!(return_floor > 0);
    assert!(clears_attention_return(
        10_000,
        10_000 - return_floor,
        return_floor
    ));
    assert!(!clears_attention_return(
        10_000,
        10_001 - return_floor,
        return_floor,
    ));

    let opportunity_batches = SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES;
    let (minimum_batches, maximum_batches) = organic_lumber_batch_limits(opportunity_batches);
    let demands = (0_u64..=127)
        .map(|entropy| organic_lumber_batches(entropy & 0b11, entropy, opportunity_batches))
        .collect::<BTreeSet<_>>();
    assert!(
        demands.len() > 16,
        "organic settlement generation collapsed to a narrow disclosed-order set"
    );
    assert!(
        demands
            .iter()
            .all(|batches| (minimum_batches..=maximum_batches).contains(batches))
    );
    for stratum in 0_u64..4 {
        let values = (0_u64..64)
            .map(|entropy| {
                organic_lumber_batches(
                    stratum,
                    entropy.wrapping_mul(0x9E37_79B9_7F4A_7C15),
                    opportunity_batches,
                )
            })
            .collect::<BTreeSet<_>>();
        assert!(
            values.len() > 1,
            "settlement pressure stratum {stratum} became a rigid fixed scenario"
        );
    }
    assert!(
        (1_u64..=128)
            .map(organic_investment_policy)
            .map(CapitalInvestmentPolicy::minimum_return_ppm)
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "organic settlement actor policy collapsed to one capital-return preference"
    );
}

#[test]
fn organic_settlement_inherited_condition_varies_within_the_healthy_used_band() {
    let registries = deep_hearth::content::build_registries();
    let definition = EQUIPMENT_TIMBER_FRAME_SAW_BENCH;
    let warning = registries
        .equipment()
        .get_equipment(definition)
        .unwrap_or_else(|| panic!("settlement frame saw definition disappeared"))
        .maintenance_thresholds()
        .warning_below();
    let conditions = (1_u64..=16)
        .map(|entropy| {
            organic_inherited_equipment_condition(
                &registries,
                definition,
                entropy.wrapping_mul(0x9E37_79B9_7F4A_7C15),
            )
        })
        .collect::<BTreeSet<_>>();
    assert!(conditions.len() > 1, "organic inherited wear collapsed");
    assert!(
        conditions
            .iter()
            .all(|condition| *condition > warning && *condition < Condition::PRISTINE),
        "organic inherited wear must remain healthy, used, and player-visible"
    );
}
