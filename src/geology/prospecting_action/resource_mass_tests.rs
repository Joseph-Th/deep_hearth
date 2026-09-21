//! Resource-mass observation contracts for localized and ambiguous geological bodies.

use crate::content::{FORM_ORE, MATERIAL_COPPER, build_registries};
use crate::core::quantity::{Mass, Pressure, Temperature};
use crate::core::state::AppState;
use crate::core::time::WorldSeed;
use crate::geology::{GeneratedDepositSpec, insert_generated_deposit};
use crate::material::{CommodityKey, MaterialComposition};
use crate::spatial::{VoxelBounds, VoxelCoord};

use super::resolve_region_resource_mass;

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

#[test]
fn fully_localized_single_body_returns_conservative_mass_bucket() {
    let mut state = AppState::new(WorldSeed::new(1));
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
fn exact_bucket_boundary_does_not_reveal_hidden_reserve_exactly() {
    let mut state = AppState::new(WorldSeed::new(4));
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
fn partial_or_ambiguous_body_localization_does_not_claim_resource_mass() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(2));
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

    let mut ambiguous = AppState::new(WorldSeed::new(3));
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
