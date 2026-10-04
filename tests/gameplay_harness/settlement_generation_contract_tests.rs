//! Cheap contracts guarding settlement organic-world and actor-policy variation.

use std::collections::BTreeSet;

use deep_hearth::content::EQUIPMENT_TIMBER_FRAME_SAW_BENCH;
use deep_hearth::maintenance::Condition;

use super::capital_investment_policy::CapitalInvestmentPolicy;
use super::settlement_generation::{
    organic_inherited_equipment_condition, organic_investment_policy, organic_lumber_batches,
};

#[test]
fn organic_settlement_generation_straddles_supplied_crossovers_and_varies_actor_policy() {
    for crossover in [4_u64, 8, 16, 32, 64, 96] {
        let demands = (0_u64..=127)
            .map(|entropy| organic_lumber_batches(entropy & 0b11, entropy, Some(crossover), 192))
            .collect::<BTreeSet<_>>();
        assert!(
            demands.len() > 1,
            "organic settlement generation collapsed to one disclosed order around crossover {crossover}"
        );
        assert!(
            demands.iter().any(|batches| *batches < crossover)
                && demands.iter().any(|batches| *batches > crossover),
            "organic settlement demand must sample both sides of supplied crossover {crossover}"
        );
        for root in [0_u64, 4, 0x1234_5678_9ABC_DEF0] {
            let bounded = (0_u64..4)
                .map(|offset| {
                    let world = root + offset;
                    organic_lumber_batches(world & 0b11, world, Some(crossover), 192)
                })
                .collect::<Vec<_>>();
            assert!(
                bounded.iter().any(|batches| *batches < crossover)
                    && bounded.iter().any(|batches| *batches > crossover),
                "one four-stratum settlement sample must cross supplied crossover {crossover}"
            );
        }
        for stratum in 0_u64..4 {
            let values = (0_u64..32)
                .map(|entropy| {
                    organic_lumber_batches(
                        stratum,
                        entropy.wrapping_mul(0x9E37_79B9_7F4A_7C15),
                        Some(crossover),
                        192,
                    )
                })
                .collect::<BTreeSet<_>>();
            assert!(
                values.len() > 1,
                "settlement pressure stratum {stratum} became a rigid fixed scenario"
            );
            if stratum & 1 == 0 {
                assert!(values.iter().all(|batches| *batches < crossover));
            } else {
                assert!(values.iter().all(|batches| *batches > crossover));
            }
        }
    }
    let no_crossover = (0_u64..=127)
        .map(|entropy| organic_lumber_batches(entropy & 0b11, entropy, None, 192))
        .collect::<BTreeSet<_>>();
    assert!(
        no_crossover.len() > 1 && no_crossover.iter().all(|batches| *batches < 192),
        "settlement demand must remain varied and leave a follow-up opportunity when mechanization has no disclosed crossover"
    );
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
