//! Acquired resource scale informs investment without leaking exact hidden reserve.

use super::extraction::FieldworkStop;
use super::*;
use deep_hearth::maintenance::Condition;

fn replay(seed: u64) -> FocusedProbeCase {
    FocusedProbeCase::new(
        seed,
        None,
        super::super::focused_seeds::FocusedProbeRole::ExplicitReplay,
    )
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
    assert!(fieldwork_supply_for_case(case) > requested);

    let episode = run_fieldwork_order(&registries, case, requested);
    assert_eq!(episode.full_order_tool, Some(EQUIPMENT_STONE_QUARRY_PICK));
    assert_eq!(episode.tool, EQUIPMENT_STONE_QUARRY_PICK);
    assert_eq!(episode.resource_knowledge_effect, "same-tool");
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
    let lower = run_fieldwork_with_supply(
        &registries,
        replay(1),
        requested,
        Mass::from_milligrams(4_100_000),
    );
    let upper = run_fieldwork_with_supply(
        &registries,
        replay(1),
        requested,
        Mass::from_milligrams(4_900_000),
    );

    assert_eq!(lower.observed_resource_mass, upper.observed_resource_mass);
    assert_eq!(
        lower.observed_resource_mass.lower(),
        Mass::from_milligrams(4_000_000)
    );
    assert_eq!(
        lower.observed_resource_mass.upper(),
        Mass::from_milligrams(5_000_000)
    );
    assert_eq!(lower.planned_local_mass, Mass::from_milligrams(5_000_000));
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
    let reserve = fieldwork_supply(6);
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
    assert_eq!(episode.extraction.stop.outcome(), "known-target-supply");
}

#[test]
fn acquired_resource_scale_changes_current_project_workload_before_depletion() {
    let registries = deep_hearth::content::build_registries();
    let demonstrated = (1_u64..=16).find_map(|seed| {
        let requested = fieldwork_order(&registries, seed);
        if fieldwork_supply(seed) >= requested {
            return None;
        }
        let episode = run_fieldwork_order(&registries, replay(seed), requested);
        (episode.planned_local_mass < requested).then_some(episode)
    });
    let episode = demonstrated.unwrap_or_else(|| {
        panic!("bounded fieldwork variation lost its quantity-informed project workload")
    });
    assert!(
        episode.planned_local_mass <= Mass::from_milligrams(1_000_000),
        "current shallow opportunity should become actionable through the authored one-kilogram resource-scale band"
    );
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
    assert!(fieldwork_supply(case.seed()) < requested);

    let episode = run_fieldwork_order(&registries, case, requested);
    assert_eq!(
        episode.full_order_tool,
        Some(EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK)
    );
    assert_eq!(episode.tool, EQUIPMENT_COPPER_REINFORCED_PICK);
    assert_eq!(episode.resource_knowledge_effect, "changed-tool");
    assert!(episode.planned_local_mass < requested);
}
