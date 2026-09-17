//! Finite reserves change experienced output, never pre-action tool choice or charged effort.

use super::*;
use deep_hearth::maintenance::Condition;

fn replay(seed: u64) -> FocusedProbeCase {
    FocusedProbeCase::new(
        seed,
        None,
        super::super::focused_seeds::FocusedProbeRole::ExplicitReplay,
    )
}

fn assert_supply_stop(supply_batches: u64, half_batch: bool, expected_stop: FieldworkStop) {
    let registries = deep_hearth::content::build_registries();
    let requested =
        short_fieldwork_order(fieldwork_mining_limits(&registries).base_quarry_batch, 1);
    let rich = run_fieldwork_order(&registries, replay(1), requested);
    let definition = registries
        .equipment()
        .get_equipment(rich.tool)
        .unwrap_or_else(|| panic!("selected tool disappeared"));
    let method = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("hand mining disappeared"));
    let CapabilityValue::Mass(batch) = definition
        .capabilities()
        .get_capability(method.max_batch_mass_capability())
        .unwrap_or_else(|| panic!("selected batch limit disappeared"))
    else {
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
    assert_eq!(
        scarce.tool, rich.tool,
        "hidden reserve must not select a different tool"
    );
    assert_eq!(scarce.observed_hardness, rich.observed_hardness);
    assert_eq!(scarce.preparation_ticks, rich.preparation_ticks);
    assert_eq!(scarce.projected_ticks, rich.projected_ticks);
    assert_eq!(
        scarce.extraction.first_ore_ticks,
        rich.extraction.first_ore_ticks
    );
    assert_eq!(
        scarce.extraction.output_grade_ppm,
        rich.extraction.output_grade_ppm
    );
    assert_eq!(scarce.extraction.extracted, reserve);
    assert_eq!(scarce.extraction.stop, expected_stop);
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
fn short_first_claim_stops_without_changing_hidden_reserve_blind_choice() {
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
fn world_seeded_shallow_opportunity_reports_partial_order() {
    let registries = deep_hearth::content::build_registries();
    let requested = fieldwork_order(&registries, 6);
    let reserve = fieldwork_supply(6);
    assert!(reserve < requested);
    let episode = run_fieldwork_order(&registries, replay(6), requested);
    assert_eq!(episode.extraction.extracted, reserve);
    assert_eq!(episode.extraction.stop, FieldworkStop::ShortClaim);
    assert_eq!(episode.extraction.stop.outcome(), "known-target-supply");
}
