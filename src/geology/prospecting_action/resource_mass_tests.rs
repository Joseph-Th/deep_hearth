//! Resource-mass observation contracts for localized and ambiguous geological bodies.

use crate::content::{FORM_LUMP, FORM_ORE, MATERIAL_COPPER, MATERIAL_STONE, build_registries};
use crate::core::quantity::{Mass, Pressure, Temperature};
use crate::core::state::AppState;
use crate::geology::{GeneratedDepositSpec, insert_generated_deposit};
use crate::material::{CommodityKey, MaterialComposition};
use crate::spatial::{VoxelBounds, VoxelCoord};

use super::{
    resolve_region_resource_mass, resource_mass_band_matches_resolution, resource_mass_bucket,
};
use crate::geology::ResourceMassEstimate;

fn voxel(x: i64) -> VoxelBounds {
    VoxelBounds::new(VoxelCoord::new(x, 0, 0), VoxelCoord::new(x + 1, 1, 1))
        .unwrap_or_else(|error| panic!("resource-mass test bounds failed: {error}"))
}

fn insert_copper(state: &mut AppState, bounds: VoxelBounds, mass: Mass) {
    let registries = build_registries();
    let spec = GeneratedDepositSpec::new(
        bounds,
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        mass,
        Temperature::from_millikelvin(293_150),
        Pressure::from_pascals(300_000_000),
        MaterialComposition::pure(MATERIAL_COPPER),
    )
    .unwrap_or_else(|error| panic!("resource-mass deposit fixture failed: {error}"));
    insert_generated_deposit(&registries, state, spec)
        .unwrap_or_else(|error| panic!("resource-mass deposit insertion failed: {error}"));
}

fn insert_stone(state: &mut AppState, bounds: VoxelBounds, mass: Mass) {
    let registries = build_registries();
    let spec = GeneratedDepositSpec::new(
        bounds,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        mass,
        Temperature::from_millikelvin(293_150),
        Pressure::from_pascals(300_000_000),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap_or_else(|error| panic!("resource-mass stone deposit fixture failed: {error}"));
    insert_generated_deposit(&registries, state, spec)
        .unwrap_or_else(|error| panic!("resource-mass stone deposit insertion failed: {error}"));
}

#[test]
fn fully_localized_single_body_returns_conservative_mass_bucket() {
    let mut state = AppState::new();
    let region = voxel(0);
    let actual = Mass::from_milligrams(4_300_000);
    insert_copper(&mut state, region, actual);

    let estimate = resolve_region_resource_mass(
        &state,
        region,
        MATERIAL_COPPER,
        Mass::from_milligrams(1_000_000),
    )
    .unwrap_or_else(|| panic!("fully localized body must produce a resource-mass estimate"));
    assert_eq!(estimate.lower(), Mass::from_milligrams(4_000_000));
    assert_eq!(estimate.upper(), Mass::from_milligrams(5_000_000));
    assert!(estimate.lower() <= actual && actual <= estimate.upper());
}

#[test]
fn resource_mass_resolution_tracks_owner_depletion_and_disappears_at_zero() {
    let registries = build_registries();
    let mut state = AppState::new();
    let region = voxel(0);
    let initial = Mass::from_milligrams(4_300_000);
    let spec = GeneratedDepositSpec::new(
        region,
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        initial,
        Temperature::from_millikelvin(293_150),
        Pressure::from_pascals(300_000_000),
        MaterialComposition::pure(MATERIAL_COPPER),
    )
    .unwrap_or_else(|error| panic!("depletion resource-mass fixture failed: {error}"));
    let deposit = insert_generated_deposit(&registries, &mut state, spec)
        .unwrap_or_else(|error| panic!("depletion resource-mass insertion failed: {error}"));
    let resolution = Mass::from_milligrams(1_000_000);

    assert_eq!(
        resolve_region_resource_mass(&state, region, MATERIAL_COPPER, resolution),
        Some(
            super::ResourceMassEstimate::new(
                Mass::from_milligrams(4_000_000),
                Mass::from_milligrams(5_000_000),
            )
            .unwrap_or_else(|error| panic!("initial resource-mass expectation failed: {error}"))
        )
    );

    let next_revision = state
        .geology()
        .revision()
        .checked_add(1)
        .unwrap_or_else(|| panic!("depletion resource-mass revision overflowed"));
    state.geology_state_mut().apply_extraction(
        deposit,
        Mass::from_milligrams(2_000_000),
        next_revision,
    );
    assert_eq!(
        resolve_region_resource_mass(&state, region, MATERIAL_COPPER, resolution),
        Some(
            super::ResourceMassEstimate::new(
                Mass::from_milligrams(2_000_000),
                Mass::from_milligrams(3_000_000),
            )
            .unwrap_or_else(|error| panic!("depleted resource-mass expectation failed: {error}"))
        )
    );

    let next_revision = state
        .geology()
        .revision()
        .checked_add(1)
        .unwrap_or_else(|| panic!("terminal resource-mass revision overflowed"));
    state.geology_state_mut().apply_extraction(
        deposit,
        Mass::from_milligrams(2_300_000),
        next_revision,
    );
    assert_eq!(
        resolve_region_resource_mass(&state, region, MATERIAL_COPPER, resolution),
        None,
        "a fully depleted body must no longer produce a positive remaining-mass estimate"
    );
}

#[test]
fn exact_bucket_boundary_does_not_reveal_hidden_reserve_exactly() {
    let mut state = AppState::new();
    let region = voxel(0);
    let actual = Mass::from_milligrams(4_000_000);
    insert_copper(&mut state, region, actual);

    let estimate = resolve_region_resource_mass(
        &state,
        region,
        MATERIAL_COPPER,
        Mass::from_milligrams(1_000_000),
    )
    .unwrap_or_else(|| panic!("fully localized body must produce a resource-mass estimate"));
    assert_eq!(estimate.lower(), actual);
    assert_eq!(estimate.upper(), Mass::from_milligrams(5_000_000));
    assert!(estimate.width() > Mass::ZERO);
}

#[test]
fn representational_ceiling_does_not_collapse_resource_mass_uncertainty() {
    let actual = Mass::from_milligrams(u64::MAX);
    let estimate = resource_mass_bucket(actual, Mass::from_milligrams(1));

    assert_eq!(estimate.upper(), actual);
    assert_eq!(estimate.lower(), Mass::from_milligrams(u64::MAX - 1));
    assert_eq!(estimate.width(), Mass::from_milligrams(1));
}

#[test]
fn representational_ceiling_retains_full_authored_resource_mass_resolution() {
    let resolution = Mass::from_milligrams(1_000_000);
    let actual = Mass::from_milligrams(u64::MAX - 100);
    let estimate = resource_mass_bucket(actual, resolution);

    assert_eq!(estimate.upper(), Mass::from_milligrams(u64::MAX));
    assert_eq!(estimate.width(), resolution);
    assert!(estimate.lower() <= actual && actual <= estimate.upper());
    assert!(resource_mass_band_matches_resolution(estimate, resolution));
}

#[test]
fn resource_mass_resolution_shape_rejects_narrow_ceiling_band() {
    let resolution = Mass::from_milligrams(1_000_000);
    let narrow = ResourceMassEstimate::new(
        Mass::from_milligrams((u64::MAX / 1_000_000) * 1_000_000),
        Mass::from_milligrams(u64::MAX),
    )
    .unwrap_or_else(|error| panic!("narrow ceiling estimate fixture failed: {error}"));

    assert!(!resource_mass_band_matches_resolution(narrow, resolution));
}

#[test]
fn unrelated_remote_copper_body_does_not_make_a_localized_body_ambiguous() {
    let mut state = AppState::new();
    let region = voxel(0);
    insert_copper(&mut state, region, Mass::from_milligrams(4_300_000));
    insert_copper(&mut state, voxel(10), Mass::from_milligrams(7_000_000));

    let estimate = resolve_region_resource_mass(
        &state,
        region,
        MATERIAL_COPPER,
        Mass::from_milligrams(1_000_000),
    )
    .unwrap_or_else(|| panic!("remote copper body must not make local resource mass ambiguous"));
    assert_eq!(estimate.lower(), Mass::from_milligrams(4_000_000));
    assert_eq!(estimate.upper(), Mass::from_milligrams(5_000_000));
}

#[test]
fn overlapping_body_without_requested_material_does_not_create_false_ambiguity() {
    let mut state = AppState::new();
    let region = voxel(0);
    insert_copper(&mut state, region, Mass::from_milligrams(4_300_000));
    insert_stone(&mut state, region, Mass::from_milligrams(9_000_000));

    let estimate = resolve_region_resource_mass(
        &state,
        region,
        MATERIAL_COPPER,
        Mass::from_milligrams(1_000_000),
    )
    .unwrap_or_else(|| panic!("zero-copper body must not count as a copper reserve"));
    assert_eq!(estimate.lower(), Mass::from_milligrams(4_000_000));
    assert_eq!(estimate.upper(), Mass::from_milligrams(5_000_000));
}

#[test]
fn broader_observation_region_does_not_claim_single_body_resource_mass() {
    let mut state = AppState::new();
    let body = voxel(0);
    let broad = VoxelBounds::new(VoxelCoord::new(0, 0, 0), VoxelCoord::new(2, 1, 1))
        .unwrap_or_else(|error| panic!("broad resource-mass bounds failed: {error}"));
    insert_copper(&mut state, body, Mass::from_milligrams(4_300_000));

    assert_eq!(
        resolve_region_resource_mass(
            &state,
            broad,
            MATERIAL_COPPER,
            Mass::from_milligrams(1_000_000),
        ),
        None,
        "an observation footprint broader than the hidden body must not reveal that body's total remaining mass"
    );
}

#[test]
fn partial_or_ambiguous_body_localization_does_not_claim_resource_mass() {
    let registries = build_registries();
    let mut state = AppState::new();
    let wide = VoxelBounds::new(VoxelCoord::new(0, 0, 0), VoxelCoord::new(2, 1, 1))
        .unwrap_or_else(|error| panic!("wide resource-mass bounds failed: {error}"));
    let spec = GeneratedDepositSpec::new(
        wide,
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        Mass::from_milligrams(5_000_000),
        Temperature::from_millikelvin(293_150),
        Pressure::from_pascals(300_000_000),
        MaterialComposition::pure(MATERIAL_COPPER),
    )
    .unwrap_or_else(|error| panic!("wide resource-mass deposit fixture failed: {error}"));
    insert_generated_deposit(&registries, &mut state, spec)
        .unwrap_or_else(|error| panic!("wide resource-mass insertion failed: {error}"));
    assert_eq!(
        resolve_region_resource_mass(
            &state,
            voxel(0),
            MATERIAL_COPPER,
            Mass::from_milligrams(1_000_000),
        ),
        None,
        "a partial sample must not reveal the total mass of a larger hidden body"
    );

    let mut ambiguous = AppState::new();
    insert_copper(&mut ambiguous, voxel(10), Mass::from_milligrams(2_000_000));
    insert_copper(&mut ambiguous, voxel(11), Mass::from_milligrams(3_000_000));
    let combined = VoxelBounds::new(VoxelCoord::new(10, 0, 0), VoxelCoord::new(12, 1, 1))
        .unwrap_or_else(|error| panic!("combined resource-mass bounds failed: {error}"));
    assert_eq!(
        resolve_region_resource_mass(
            &ambiguous,
            combined,
            MATERIAL_COPPER,
            Mass::from_milligrams(1_000_000),
        ),
        None,
        "one observation must not collapse multiple hidden bodies into a false single reserve"
    );
}
