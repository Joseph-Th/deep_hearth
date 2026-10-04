//! Acquired resource scale informs investment without leaking exact hidden reserve.

use std::collections::BTreeSet;

use super::extraction::FieldworkStop;
use super::world::{
    FIELDWORK_BULK_SUPPLY_MIN_PPM, FIELDWORK_COMMON_SUPPLY_MAX_PPM,
    FIELDWORK_COMMON_SUPPLY_MIN_PPM, FIELDWORK_SHALLOW_SUPPLY_MAX_PPM, scaled_fieldwork_supply,
};
use super::*;
use deep_hearth::maintenance::Condition;

fn replay(seed: u64) -> FocusedProbeCase {
    FocusedProbeCase::new(
        seed,
        Some(super::super::seed::mix64(seed ^ 0x4649_454C_445F_5355)),
        super::super::focused_case::FocusedProbeRole::ExplicitReplay,
    )
}

#[test]
fn exploratory_supply_spans_shallow_common_and_bulk_opportunities() {
    let registries = deep_hearth::content::build_registries();
    let base_batch = fieldwork_mining_limits(&registries).base_quarry_batch;
    let shallow_max =
        scaled_fieldwork_supply(base_batch, FIELDWORK_SHALLOW_SUPPLY_MAX_PPM).milligrams();
    let common_min =
        scaled_fieldwork_supply(base_batch, FIELDWORK_COMMON_SUPPLY_MIN_PPM).milligrams();
    let common_max =
        scaled_fieldwork_supply(base_batch, FIELDWORK_COMMON_SUPPLY_MAX_PPM).milligrams();
    let bulk_min = scaled_fieldwork_supply(base_batch, FIELDWORK_BULK_SUPPLY_MIN_PPM).milligrams();
    let supplies = (0_u64..256)
        .map(|seed| fieldwork_supply(&registries, seed))
        .map(Mass::milligrams)
        .collect::<Vec<_>>();
    assert!(supplies.iter().any(|&mass| mass <= shallow_max));
    assert!(
        supplies
            .iter()
            .any(|&mass| (common_min..=common_max).contains(&mass))
    );
    assert!(supplies.iter().any(|&mass| mass >= bulk_min));
}

#[test]
fn exploratory_demand_and_reserve_scale_are_not_coupled() {
    let registries = deep_hearth::content::build_registries();
    let base_batch = fieldwork_mining_limits(&registries).base_quarry_batch;
    let shallow_max =
        scaled_fieldwork_supply(base_batch, FIELDWORK_SHALLOW_SUPPLY_MAX_PPM).milligrams();
    let bulk_min = scaled_fieldwork_supply(base_batch, FIELDWORK_BULK_SUPPLY_MIN_PPM).milligrams();
    let combinations = (0_u64..256)
        .map(|seed| {
            let supply = fieldwork_supply(&registries, seed).milligrams();
            let supply_class = if supply <= shallow_max {
                "shallow"
            } else if supply >= bulk_min {
                "bulk"
            } else {
                "common"
            };
            let order = fieldwork_order(&registries, seed);
            (fieldwork_order_horizon(&registries, order), supply_class)
        })
        .collect::<BTreeSet<_>>();

    let expected = ["short", "project", "bulk"]
        .into_iter()
        .flat_map(|horizon| {
            ["shallow", "common", "bulk"]
                .into_iter()
                .map(move |supply| (horizon, supply))
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        combinations, expected,
        "every demand horizon must occur against every reserve scale so the evaluator does not manufacture investment payback by correlating goals with hidden supply"
    );
}

#[test]
fn followup_sites_have_independent_reserve_opportunities() {
    let registries = deep_hearth::content::build_registries();
    let base_batch = fieldwork_mining_limits(&registries).base_quarry_batch;
    let shallow_max =
        scaled_fieldwork_supply(base_batch, FIELDWORK_SHALLOW_SUPPLY_MAX_PPM).milligrams();
    let common_min =
        scaled_fieldwork_supply(base_batch, FIELDWORK_COMMON_SUPPLY_MIN_PPM).milligrams();
    let common_max =
        scaled_fieldwork_supply(base_batch, FIELDWORK_COMMON_SUPPLY_MAX_PPM).milligrams();
    let bulk_min = scaled_fieldwork_supply(base_batch, FIELDWORK_BULK_SUPPLY_MIN_PPM).milligrams();
    let opportunities = (0_u64..128)
        .flat_map(|seed| super::world::fieldwork_followup_opportunities(&registries, seed))
        .collect::<Vec<_>>();
    let supplies = opportunities
        .iter()
        .copied()
        .flatten()
        .map(Mass::milligrams)
        .collect::<Vec<_>>();
    assert!(supplies.iter().any(|&mass| mass <= shallow_max));
    assert!(
        supplies
            .iter()
            .any(|&mass| (common_min..=common_max).contains(&mass))
    );
    assert!(supplies.iter().any(|&mass| mass >= bulk_min));
    assert!(
        (0_u64..128).any(|seed| {
            super::world::fieldwork_followup_opportunities(&registries, seed)
                .windows(2)
                .any(|pair| pair[0] != pair[1])
        }),
        "follow-up sites must not copy one geological outcome across the local search area"
    );
    assert!(
        opportunities.iter().any(Option::is_none),
        "follow-up fieldwork must include searched areas with no localized copper opportunity"
    );
    assert!(
        opportunities.iter().any(Option::is_some),
        "follow-up fieldwork must retain productive neighboring opportunities"
    );
}

#[test]
fn maintained_bulk_order_replays_quarry_investment_from_seed_alone() {
    let registries = deep_hearth::content::build_registries();
    let case = replay(FIELDWORK_BULK_INVESTMENT_COVERAGE_SEED);
    let requested = fieldwork_order_for_case(&registries, case);
    assert_eq!(
        requested,
        multiplied_mass(
            fieldwork_mining_limits(&registries).base_quarry_batch,
            FIELDWORK_BULK_INVESTMENT_COVERAGE_BATCHES,
            "bulk-investment coverage expectation",
        )
    );
    assert!(fieldwork_supply_for_case(&registries, case) > requested);

    let episode = run_fieldwork_order(&registries, case, requested);
    assert_eq!(episode.full_order_tool, Some(EQUIPMENT_STONE_QUARRY_PICK));
    assert_eq!(episode.tool, EQUIPMENT_STONE_QUARRY_PICK);
    assert_eq!(
        episode.resource_knowledge_effect,
        FieldworkResourceKnowledgeEffect::SameTool
    );
    assert_eq!(episode.planned_local_mass, requested);
    assert_eq!(episode.extraction.stop, FieldworkStop::OrderComplete);
}

#[test]
fn maintained_reinforcement_bulk_order_selects_reinforced_quarry_from_visible_scale() {
    let registries = deep_hearth::content::build_registries();
    let case = replay(FIELDWORK_REINFORCED_BULK_COVERAGE_SEED);
    let requested = fieldwork_order_for_case(&registries, case);
    assert_eq!(
        requested,
        multiplied_mass(
            fieldwork_mining_limits(&registries).base_quarry_batch,
            FIELDWORK_REINFORCED_BULK_COVERAGE_BATCHES,
            "reinforced bulk-investment coverage expectation",
        )
    );
    assert!(fieldwork_supply_for_case(&registries, case) > requested);

    let episode = run_fieldwork_order(&registries, case, requested);
    assert_eq!(
        episode.full_order_tool,
        Some(EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK)
    );
    assert_eq!(episode.tool, EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK);
    assert_eq!(
        episode.resource_knowledge_effect,
        FieldworkResourceKnowledgeEffect::SameTool
    );
    assert_eq!(episode.planned_local_mass, requested);
    assert_eq!(episode.extraction.stop, FieldworkStop::OrderComplete);
}

fn assert_supply_stop(supply_batches: u64, half_batch: bool, expected_stop: FieldworkStop) {
    let registries = deep_hearth::content::build_registries();
    let requested =
        short_fieldwork_order(fieldwork_mining_limits(&registries).base_quarry_batch, 1);
    let method = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("hand mining disappeared"));
    let reference = run_fieldwork_order(&registries, replay(1), requested);
    let CapabilityValue::Mass(batch) = pristine_equipment_capability(
        &registries,
        reference.tool,
        method.max_batch_mass_capability(),
    ) else {
        panic!("batch physical kind changed")
    };
    let reserve = multiplied_mass(batch, supply_batches, "shortage regression")
        .checked_add(Mass::from_milligrams(if half_batch {
            batch.milligrams() / 2
        } else {
            0
        }))
        .unwrap_or_else(|| panic!("shortage reserve overflowed"));
    assert!(reserve < requested);
    let scarce = run_fieldwork_with_supply(&registries, replay(1), requested, reserve);
    assert!(
        scarce.observed_resource_mass.lower() <= reserve
            && reserve <= scarce.observed_resource_mass.upper(),
        "acquired resource-scale evidence must conservatively contain hidden reserve truth"
    );
    assert_eq!(
        scarce.planned_local_mass,
        requested.min(scarce.observed_resource_mass.upper()),
        "tool investment must be sized from acquired quantity evidence, not exact hidden reserve"
    );
    assert_eq!(scarce.extraction.extracted, reserve);
    assert_eq!(scarce.extraction.stop, expected_stop);
    let definition = registries
        .equipment()
        .get_equipment(scarce.tool)
        .unwrap_or_else(|| panic!("selected fieldwork equipment definition disappeared"));
    let attempted_batches = supply_batches + u64::from(half_batch);
    assert_eq!(
        scarce.extraction.batches, attempted_batches,
        "no work after observed shortage"
    );
    let effort =
        multiplied_mass(batch, attempted_batches, "charged effort regression").min(requested);
    let projection = resolve_mining_order(
        registries.core().physical_tick_duration(),
        method,
        definition,
        MiningOrderRequest::new(
            Condition::PRISTINE,
            scarce.observed_hardness.upper(),
            effort,
            batch,
            256,
        ),
    )
    .unwrap_or_else(|error| panic!("charged effort projection failed: {error}"));
    assert_eq!(
        scarce.extraction.ticks,
        projection.duration().value(),
        "short claims pay full requested batch effort"
    );
    assert_eq!(
        scarce.extraction.condition_after,
        projection.condition_after(),
        "short claims pay full requested batch wear"
    );
    // Full episodes above also reconcile matter and validate trusted load at both endpoints.
    if attempted_batches == 1 {
        assert_eq!(scarce.extraction.ticks, scarce.extraction.first_ore_ticks);
    }
}

#[test]
fn short_first_claim_stops_after_quantity_informed_investment() {
    assert_supply_stop(0, true, FieldworkStop::ShortClaim);
}

#[test]
fn short_later_claim_stops_after_full_requested_batch_effort() {
    assert_supply_stop(1, true, FieldworkStop::ShortClaim);
}

#[test]
fn exact_batch_exhaustion_stops_on_canonical_target_refresh() {
    assert_supply_stop(1, false, FieldworkStop::TargetNoLongerResolved);
}

#[test]
fn exact_hidden_reserve_inside_same_acquired_band_cannot_change_pre_action_plan() {
    let registries = deep_hearth::content::build_registries();
    let requested = Mass::from_milligrams(20_000_000);
    let lower_supply = Mass::from_milligrams(4_100_000);
    let upper_supply = Mass::from_milligrams(4_900_000);
    let lower = run_fieldwork_with_supply(&registries, replay(1), requested, lower_supply);
    let upper = run_fieldwork_with_supply(&registries, replay(1), requested, upper_supply);

    assert_eq!(lower.observed_resource_mass, upper.observed_resource_mass);
    assert!(lower.observed_resource_mass.lower() <= lower_supply);
    assert!(upper_supply <= lower.observed_resource_mass.upper());
    assert_eq!(
        lower.planned_local_mass,
        lower.observed_resource_mass.upper()
    );
    assert_eq!(lower.planned_local_mass, upper.planned_local_mass);
    assert_eq!(lower.observed_hardness, upper.observed_hardness);
    assert_eq!(lower.tool, upper.tool);
    assert_eq!(lower.preparation_ticks, upper.preparation_ticks);
    assert_eq!(lower.projected_ticks, upper.projected_ticks);
    assert_ne!(lower.extraction.extracted, upper.extraction.extracted);
}

#[test]
fn world_seeded_shallow_opportunity_reports_partial_order() {
    let registries = deep_hearth::content::build_registries();
    let requested = fieldwork_order(&registries, 6);
    let reserve = fieldwork_supply(&registries, 6);
    assert!(reserve < requested);
    let episode = run_fieldwork_order(&registries, replay(6), requested);
    assert!(episode.observed_resource_mass.lower() <= reserve);
    assert!(reserve <= episode.observed_resource_mass.upper());
    assert!(episode.planned_local_mass < requested);
    assert_eq!(
        episode.planned_local_mass,
        episode.observed_resource_mass.upper()
    );
    assert_eq!(episode.extraction.extracted, reserve);
    assert_eq!(episode.extraction.stop, FieldworkStop::ShortClaim);
}

#[test]
fn maintained_reserve_scale_case_replays_overinvestment_avoidance_from_seed_alone() {
    let registries = deep_hearth::content::build_registries();
    let case = replay(FIELDWORK_RESERVE_SCALE_COVERAGE_SEED);
    let requested = fieldwork_order_for_case(&registries, case);
    assert_eq!(
        requested,
        multiplied_mass(
            fieldwork_mining_limits(&registries).base_quarry_batch,
            FIELDWORK_RESERVE_SCALE_COVERAGE_BATCHES,
            "reserve-scale coverage expectation",
        )
    );
    assert!(fieldwork_supply(&registries, case.seed()) < requested);

    let episode = run_fieldwork_order(&registries, case, requested);
    assert_eq!(
        episode.full_order_tool,
        Some(EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK)
    );
    assert_eq!(episode.tool, EQUIPMENT_COPPER_REINFORCED_PICK);
    assert_eq!(
        episode.resource_knowledge_effect,
        FieldworkResourceKnowledgeEffect::ChangedTool
    );
    assert!(episode.planned_local_mass < requested);
}
