//! Content-relative world-generation contracts for the first-foundry decision episode.

use std::collections::BTreeSet;

use super::*;
use crate::focused_case::FocusedProbeRole;
use crate::focused_witnesses::FOUNDRY_BOOTSTRAP_RECOVERY_COVERAGE_SEED;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum OpportunityRegime {
    NativeSufficient,
    RecoverableOwnedOre,
    StillShort,
}

fn opportunity_inputs(
    registries: &Registries,
) -> (planning::FoundryBootstrapRoutePlan, Mass, Mass) {
    let melting = registries
        .thermal()
        .get_melting(PROCESS_MELT_PURE_COPPER)
        .unwrap_or_else(|| panic!("first-foundry melting definition disappeared"));
    let casting = registries
        .thermal()
        .get_casting(PROCESS_CAST_PURE_COPPER)
        .unwrap_or_else(|| panic!("first-foundry casting definition disappeared"));
    let immediate = nominal_equipment_mass_capability(
        registries,
        EQUIPMENT_STONE_ARC_CRUCIBLE_FURNACE,
        melting.max_batch_mass_capability(),
    )
    .min(nominal_equipment_mass_capability(
        registries,
        EQUIPMENT_STONE_INGOT_MOLD,
        casting.max_batch_mass_capability(),
    ));
    let settlement_cast = nominal_equipment_mass_capability(
        registries,
        EQUIPMENT_FOUR_CAVITY_STONE_INGOT_MOLD,
        casting.max_batch_mass_capability(),
    );
    (
        foundry_bootstrap_route_plan(registries, immediate),
        settlement_mold_ingot_requirement(registries),
        settlement_cast,
    )
}

fn classify(
    registries: &Registries,
    opportunity: planning::FoundryResourceOpportunity,
) -> OpportunityRegime {
    let remaining_after_immediate = opportunity
        .native
        .checked_sub(opportunity.immediate_native_input)
        .unwrap_or_else(|| panic!("generated foundry world cannot fund its immediate order"));
    if remaining_after_immediate >= opportunity.required_after_current {
        return OpportunityRegime::NativeSufficient;
    }
    let shortfall = opportunity
        .required_after_current
        .checked_sub(remaining_after_immediate)
        .unwrap_or_else(|| unreachable!("foundry shortfall was established"));
    let needed_feed = recovery::minimum_powered_ore_feed_for_target_recovery(
        registries,
        shortfall,
        opportunity.owned_ore.copper_ppm,
    )
    .unwrap_or_else(|| panic!("generated foundry recovery projection overflowed"));
    if opportunity.owned_ore.mass >= needed_feed {
        OpportunityRegime::RecoverableOwnedOre
    } else {
        OpportunityRegime::StillShort
    }
}

#[test]
fn organic_foundry_worlds_span_current_workload_decision_regimes() {
    let registries = deep_hearth::content::build_registries();
    let (route_plan, ingots, settlement_cast) = opportunity_inputs(&registries);
    let opportunities = (1_u64..=96)
        .map(|seed| {
            foundry_resource_opportunity(
                &registries,
                FocusedProbeCase::new(seed, None, FocusedProbeRole::OrganicVariation),
                &route_plan,
                ingots,
                settlement_cast,
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        opportunities
            .iter()
            .copied()
            .map(|opportunity| classify(&registries, opportunity))
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            OpportunityRegime::NativeSufficient,
            OpportunityRegime::RecoverableOwnedOre,
            OpportunityRegime::StillShort,
        ]),
        "fresh foundry roots must still cover build-now, recover-then-build, and defer outcomes against current authored demand"
    );
    assert!(
        opportunities
            .iter()
            .map(|opportunity| opportunity.native.milligrams())
            .collect::<BTreeSet<_>>()
            .len()
            > 1
    );
    assert!(
        opportunities
            .iter()
            .map(|opportunity| opportunity.owned_ore.mass.milligrams())
            .collect::<BTreeSet<_>>()
            .len()
            > 1
    );
    assert!(
        opportunities
            .iter()
            .map(|opportunity| opportunity.owned_ore.copper_ppm)
            .collect::<BTreeSet<_>>()
            .len()
            > 1
    );
}

#[test]
fn maintained_foundry_worlds_pin_opposite_sides_of_current_threshold() {
    let registries = deep_hearth::content::build_registries();
    let (route_plan, ingots, settlement_cast) = opportunity_inputs(&registries);
    let anchor = foundry_resource_opportunity(
        &registries,
        FocusedProbeCase::new(1, None, FocusedProbeRole::MaintainedAnchor),
        &route_plan,
        ingots,
        settlement_cast,
    );
    let recovery_coverage = foundry_resource_opportunity(
        &registries,
        FocusedProbeCase::new(
            FOUNDRY_BOOTSTRAP_RECOVERY_COVERAGE_SEED,
            None,
            FocusedProbeRole::MaintainedCoverage,
        ),
        &route_plan,
        ingots,
        settlement_cast,
    );
    let coverage = foundry_resource_opportunity(
        &registries,
        FocusedProbeCase::new(2, None, FocusedProbeRole::MaintainedCoverage),
        &route_plan,
        ingots,
        settlement_cast,
    );

    assert_eq!(
        classify(&registries, anchor),
        OpportunityRegime::NativeSufficient
    );
    assert_eq!(
        classify(&registries, coverage),
        OpportunityRegime::StillShort
    );
    assert_eq!(
        classify(&registries, recovery_coverage),
        OpportunityRegime::RecoverableOwnedOre
    );
}

#[test]
fn inherited_workshop_condition_keeps_maintained_witnesses_pristine_and_varies_organic_worlds() {
    let registries = deep_hearth::content::build_registries();
    let definition = EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR;
    for role in [
        FocusedProbeRole::MaintainedAnchor,
        FocusedProbeRole::MaintainedCoverage,
    ] {
        assert_eq!(
            inherited_equipment_condition(
                &registries,
                definition,
                FocusedProbeCase::new(7, None, role),
            ),
            Condition::PRISTINE
        );
    }

    let equipment = registries
        .equipment()
        .get_equipment(definition)
        .unwrap_or_else(|| panic!("first-foundry separator definition disappeared"));
    let warning = equipment.maintenance_thresholds().warning_below();
    let organic = (1_u64..=12)
        .map(|seed| {
            inherited_equipment_condition(
                &registries,
                definition,
                FocusedProbeCase::new(seed, None, FocusedProbeRole::OrganicVariation),
            )
        })
        .collect::<BTreeSet<_>>();
    assert!(
        organic.len() > 1,
        "organic inherited workshop condition must vary across replay seeds"
    );
    assert!(
        organic
            .iter()
            .all(|condition| { *condition > warning && *condition < Condition::PRISTINE })
    );
}
