//! Content-relative world-generation contracts for the first-foundry decision episode.

use std::collections::BTreeSet;

use deep_hearth::content::{
    EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR, EQUIPMENT_FOUR_CAVITY_STONE_INGOT_MOLD,
    EQUIPMENT_STONE_ARC_CRUCIBLE_FURNACE, EQUIPMENT_STONE_INGOT_MOLD, FORM_NATIVE_METAL,
    MATERIAL_COPPER, PROCESS_CAST_PURE_COPPER, PROCESS_MELT_PURE_COPPER,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::maintenance::Condition;
use deep_hearth::material::CommodityKey;
use deep_hearth::registry::Registries;

use super::copper_progression_world::progression_ore_grades;
use super::equipment_support::nominal_equipment_mass_capability;
use super::first_foundry_planning::{
    FoundryBootstrapRoutePlan, FoundryResourceOpportunity, foundry_bootstrap_route_plan,
    foundry_resource_opportunity, inherited_equipment_condition, settlement_mold_ingot_requirement,
};
use super::first_foundry_recovery_planning::minimum_powered_ore_feed_for_target_recovery;
use super::focused_case::{FocusedProbeCase, FocusedProbeRole};
use super::primitive_workload::primitive_mining_cycle_mass;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum OpportunityRegime {
    NativeSufficient,
    RecoverableOwnedOre,
    StillShort,
}

fn opportunity_inputs(
    registries: &Registries,
    case: FocusedProbeCase,
) -> (FoundryBootstrapRoutePlan, Mass, Mass) {
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
        foundry_bootstrap_route_plan(registries, case, immediate),
        settlement_mold_ingot_requirement(registries),
        settlement_cast,
    )
}

fn classify(registries: &Registries, opportunity: FoundryResourceOpportunity) -> OpportunityRegime {
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
    let needed_feed = minimum_powered_ore_feed_for_target_recovery(
        registries,
        shortfall,
        opportunity.owned_ore.copper_ppm,
        opportunity.recovery_batch_limit,
    )
    .unwrap_or_else(|| panic!("generated foundry recovery projection overflowed"));
    if opportunity.owned_ore.mass >= needed_feed {
        OpportunityRegime::RecoverableOwnedOre
    } else {
        OpportunityRegime::StillShort
    }
}

#[test]
fn organic_foundry_generation_does_not_read_the_build_threshold() {
    let registries = deep_hearth::content::build_registries();
    for (seed, role) in [
        (0_u64, FocusedProbeRole::OrganicVariation),
        (1, FocusedProbeRole::OrganicVariation),
        (2, FocusedProbeRole::OrganicVariation),
        (3, FocusedProbeRole::OrganicVariation),
        (0x1234_5678_9ABC_DEF0, FocusedProbeRole::ExplicitReplay),
    ] {
        let case = FocusedProbeCase::new(seed, None, role);
        let (route_plan, ingots, settlement_cast) = opportunity_inputs(&registries, case);
        let baseline = foundry_resource_opportunity(
            &registries,
            case,
            &route_plan,
            ingots,
            settlement_cast,
            false,
        );
        let mut more_expensive = route_plan.clone();
        let native = CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL);
        let extra_capital = settlement_cast
            .milligrams()
            .checked_mul(3)
            .and_then(|extra| {
                more_expensive
                    .capital_native_copper()
                    .milligrams()
                    .checked_add(extra)
            })
            .map(Mass::from_milligrams)
            .unwrap_or_else(|| panic!("foundry threshold-independence fixture overflowed"));
        more_expensive.capital_raw.insert(native, extra_capital);
        let shifted = foundry_resource_opportunity(
            &registries,
            case,
            &more_expensive,
            ingots,
            settlement_cast,
            false,
        );

        assert_ne!(
            baseline.required_after_current,
            shifted.required_after_current
        );
        assert_eq!(baseline.native, shifted.native);
        assert_eq!(baseline.owned_ore, shifted.owned_ore);
        assert_eq!(baseline.recovery_batch_limit, shifted.recovery_batch_limit);
    }
}

#[test]
fn organic_foundry_worlds_follow_prior_progression_scales_and_vary() {
    let registries = deep_hearth::content::build_registries();
    let opportunities = (1_u64..=96)
        .map(|seed| {
            let case = FocusedProbeCase::new(seed, None, FocusedProbeRole::OrganicVariation);
            let (route_plan, ingots, settlement_cast) = opportunity_inputs(&registries, case);
            let opportunity = foundry_resource_opportunity(
                &registries,
                case,
                &route_plan,
                ingots,
                settlement_cast,
                false,
            );
            (seed, opportunity)
        })
        .collect::<Vec<_>>();

    for (seed, opportunity) in &opportunities {
        let cycle = primitive_mining_cycle_mass(&registries, *seed);
        assert!(
            opportunity
                .owned_ore
                .mass
                .milligrams()
                .is_multiple_of(cycle.milligrams())
        );
        let ore_batches = opportunity.owned_ore.mass.milligrams() / cycle.milligrams();
        assert!((1..=3).contains(&ore_batches));
        let grades = progression_ore_grades(*seed);
        assert!(
            opportunity.owned_ore.copper_ppm == grades.0
                || opportunity.owned_ore.copper_ppm == grades.1
        );
        assert!(opportunity.native > opportunity.immediate_native_input);
    }

    let native_reserves = opportunities
        .iter()
        .map(|(_, opportunity)| opportunity.native.milligrams())
        .collect::<BTreeSet<_>>();
    let ore_batches = opportunities
        .iter()
        .map(|(seed, opportunity)| {
            let cycle = primitive_mining_cycle_mass(&registries, *seed);
            opportunity.owned_ore.mass.milligrams() / cycle.milligrams()
        })
        .collect::<BTreeSet<_>>();
    let grades = opportunities
        .iter()
        .map(|(_, opportunity)| opportunity.owned_ore.copper_ppm)
        .collect::<BTreeSet<_>>();
    assert!(native_reserves.len() > 1);
    assert!(ore_batches.len() > 1);
    assert!(grades.len() > 1);
}

#[test]
fn maintained_foundry_worlds_pin_opposite_sides_of_current_threshold() {
    let registries = deep_hearth::content::build_registries();
    let opportunity = |case, recovery_coverage| {
        let (route_plan, ingots, settlement_cast) = opportunity_inputs(&registries, case);
        foundry_resource_opportunity(
            &registries,
            case,
            &route_plan,
            ingots,
            settlement_cast,
            recovery_coverage,
        )
    };
    let anchor = opportunity(
        FocusedProbeCase::new(1, None, FocusedProbeRole::MaintainedAnchor),
        false,
    );
    let recovery_coverage = opportunity(
        FocusedProbeCase::new(2, None, FocusedProbeRole::MaintainedCoverage),
        true,
    );
    let coverage = opportunity(
        FocusedProbeCase::new(3, None, FocusedProbeRole::MaintainedCoverage),
        false,
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
    assert!(organic.len() > 1);
    assert!(
        organic
            .iter()
            .all(|condition| *condition > warning && *condition < Condition::PRISTINE)
    );
}
