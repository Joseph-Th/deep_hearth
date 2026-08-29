//! Tests for evidence-gated mining-target resolution.

use super::*;
use crate::content::{FORM_ORE, MATERIAL_COPPER, build_registries};
use crate::core::quantity::{Mass, Pressure, Temperature};
use crate::core::state::AppState;
use crate::core::time::WorldSeed;
use crate::geology::{
    GeneratedDepositSpec, GeologicalEvidenceKind, MaterialAbundanceEstimate, ProspectingResolution,
    insert_generated_deposit, validate_record_prospecting,
};
use crate::material::{CommodityKey, MaterialComposition};
use crate::registry::Registries;
use crate::spatial::{VoxelBounds, VoxelCoord};

fn bounds(min_x: i64, max_x: i64) -> VoxelBounds {
    VoxelBounds::new(VoxelCoord::new(min_x, -4, 0), VoxelCoord::new(max_x, -3, 1))
        .unwrap_or_else(|error| panic!("mining target bounds fixture failed: {error}"))
}

fn insert_copper_deposit(registries: &Registries, state: &mut AppState, region: VoxelBounds) {
    let spec = GeneratedDepositSpec::new(
        region,
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        Mass::from_milligrams(1_000),
        Temperature::from_millikelvin(293_150),
        Pressure::from_pascals(350_000_000),
        MaterialComposition::pure(MATERIAL_COPPER),
    )
    .unwrap_or_else(|error| panic!("mining target deposit fixture failed: {error}"));
    insert_generated_deposit(registries, state, spec)
        .unwrap_or_else(|error| panic!("mining target deposit insertion failed: {error}"));
}

fn record_copper_evidence(
    registries: &Registries,
    state: &mut AppState,
    region: VoxelBounds,
    lower_ppm: u32,
    upper_ppm: u32,
) {
    let estimate = MaterialAbundanceEstimate::new(MATERIAL_COPPER, lower_ppm, upper_ppm)
        .unwrap_or_else(|error| panic!("mining target estimate fixture failed: {error}"));
    let resolution = ProspectingResolution::new_for_fixture(
        region,
        GeologicalEvidenceKind::SurfaceExposure,
        vec![estimate],
    );
    validate_record_prospecting(registries, state, resolution)
        .unwrap_or_else(|error| panic!("mining target evidence validation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("mining target evidence commit failed: {error}"));
}

#[test]
fn mining_target_requires_acquired_evidence() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA11E_1001));
    let region = bounds(0, 1);
    insert_copper_deposit(&registries, &mut state, region);

    assert_eq!(
        resolve_mining_target(&state, MiningTargetRequest::new(region, MATERIAL_COPPER),),
        Err(MiningTargetResolutionError::NoEvidence {
            material: MATERIAL_COPPER,
            region,
        })
    );
}

#[test]
fn compatible_local_evidence_resolves_one_opaque_target() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA11E_1002));
    let region = bounds(0, 1);
    insert_copper_deposit(&registries, &mut state, region);
    record_copper_evidence(&registries, &mut state, region, 900_000, 1_000_000);

    let target = resolve_mining_target(&state, MiningTargetRequest::new(region, MATERIAL_COPPER))
        .unwrap_or_else(|error| panic!("local mining target resolution failed: {error}"));
    assert_eq!(target.region(), region);
    assert_eq!(target.material(), MATERIAL_COPPER);
}

#[test]
fn zero_upper_bound_rules_out_target_even_when_hidden_truth_contains_material() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA11E_1003));
    let region = bounds(0, 1);
    insert_copper_deposit(&registries, &mut state, region);
    record_copper_evidence(&registries, &mut state, region, 0, 0);

    assert_eq!(
        resolve_mining_target(&state, MiningTargetRequest::new(region, MATERIAL_COPPER),),
        Err(MiningTargetResolutionError::EvidenceRulesOutMaterial {
            material: MATERIAL_COPPER,
            region,
        })
    );
}

#[test]
fn uncertain_zero_lower_bound_cannot_use_hidden_truth_as_presence_oracle() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA11E_1008));
    let region = bounds(0, 1);
    insert_copper_deposit(&registries, &mut state, region);
    record_copper_evidence(&registries, &mut state, region, 0, 100_000);

    assert_eq!(
        resolve_mining_target(&state, MiningTargetRequest::new(region, MATERIAL_COPPER),),
        Err(
            MiningTargetResolutionError::EvidenceInsufficientToResolveTarget {
                material: MATERIAL_COPPER,
                region,
            }
        )
    );
}

#[test]
fn broad_evidence_does_not_choose_between_multiple_hidden_deposits() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA11E_1004));
    let west = bounds(0, 1);
    let east = bounds(2, 3);
    let broad = bounds(0, 3);
    insert_copper_deposit(&registries, &mut state, west);
    insert_copper_deposit(&registries, &mut state, east);
    record_copper_evidence(&registries, &mut state, broad, 1, 1_000_000);

    assert_eq!(
        resolve_mining_target(&state, MiningTargetRequest::new(broad, MATERIAL_COPPER),),
        Err(
            MiningTargetResolutionError::EvidenceInsufficientToResolveTarget {
                material: MATERIAL_COPPER,
                region: broad,
            }
        )
    );
}

#[test]
fn broad_positive_evidence_does_not_reveal_one_hidden_deposit() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA11E_1009));
    let target = bounds(1, 2);
    let broad = bounds(0, 3);
    insert_copper_deposit(&registries, &mut state, target);
    record_copper_evidence(&registries, &mut state, broad, 1, 1_000_000);

    assert_eq!(
        resolve_mining_target(&state, MiningTargetRequest::new(broad, MATERIAL_COPPER)),
        Err(
            MiningTargetResolutionError::EvidenceInsufficientToResolveTarget {
                material: MATERIAL_COPPER,
                region: broad,
            }
        ),
        "hidden deposit count must not turn area evidence into an exact extraction target"
    );
}

#[test]
fn narrow_request_cannot_turn_broad_positive_evidence_into_localization() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA11E_1010));
    let target = bounds(1, 2);
    let broad = bounds(0, 3);
    insert_copper_deposit(&registries, &mut state, target);
    record_copper_evidence(&registries, &mut state, broad, 1, 1_000_000);

    assert_eq!(
        resolve_mining_target(&state, MiningTargetRequest::new(target, MATERIAL_COPPER)),
        Err(
            MiningTargetResolutionError::EvidenceInsufficientToResolveTarget {
                material: MATERIAL_COPPER,
                region: broad,
            }
        ),
        "request geometry must not be usable as a hidden-location oracle"
    );
}

#[test]
fn overlapping_acquired_observations_can_genuinely_localize_one_voxel() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA11E_1011));
    let west = bounds(0, 2);
    let east = bounds(1, 3);
    let localized = bounds(1, 2);
    let query = bounds(0, 3);
    insert_copper_deposit(&registries, &mut state, localized);
    record_copper_evidence(&registries, &mut state, west, 900_000, 1_000_000);
    record_copper_evidence(&registries, &mut state, east, 900_000, 1_000_000);

    let resolved = resolve_mining_target(&state, MiningTargetRequest::new(query, MATERIAL_COPPER))
        .unwrap_or_else(|error| {
            panic!("acquired evidence overlap did not localize target: {error}")
        });
    assert_eq!(resolved.region(), localized);
    assert_eq!(resolved.material(), MATERIAL_COPPER);
}

#[test]
fn abundance_bounds_must_fit_the_hidden_target_composition() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA11E_1007));
    let region = bounds(0, 1);
    insert_copper_deposit(&registries, &mut state, region);
    record_copper_evidence(&registries, &mut state, region, 100_000, 200_000);

    assert_eq!(
        resolve_mining_target(&state, MiningTargetRequest::new(region, MATERIAL_COPPER),),
        Err(
            MiningTargetResolutionError::EvidenceInsufficientToResolveTarget {
                material: MATERIAL_COPPER,
                region,
            }
        )
    );
}

#[test]
fn false_positive_evidence_uses_the_same_non_oracular_failure_as_ambiguous_evidence() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA11E_1006));
    let region = bounds(0, 1);
    record_copper_evidence(&registries, &mut state, region, 1, 900_000);

    assert_eq!(
        resolve_mining_target(&state, MiningTargetRequest::new(region, MATERIAL_COPPER),),
        Err(
            MiningTargetResolutionError::EvidenceInsufficientToResolveTarget {
                material: MATERIAL_COPPER,
                region,
            }
        )
    );
}

#[test]
fn contradictory_local_evidence_requires_more_information() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA11E_1005));
    let region = bounds(0, 1);
    insert_copper_deposit(&registries, &mut state, region);
    record_copper_evidence(&registries, &mut state, region, 700_000, 900_000);
    record_copper_evidence(&registries, &mut state, region, 100_000, 300_000);

    assert_eq!(
        resolve_mining_target(&state, MiningTargetRequest::new(region, MATERIAL_COPPER),),
        Err(MiningTargetResolutionError::ConflictingEvidence {
            material: MATERIAL_COPPER,
            region,
            highest_lower_ppm: 700_000,
            lowest_upper_ppm: 300_000,
        })
    );
}
