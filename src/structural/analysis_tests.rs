//! Focused tests for public structural planning projections owned by the analysis module.

use super::*;
use crate::content::{MATERIAL_WOOD, STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries};

#[test]
fn pristine_member_capacity_uses_the_authored_load_axis_and_exact_units() {
    let registries = build_registries();
    let profile = registries
        .structural()
        .get_profile(STRUCTURAL_PROFILE_AXIAL_COMPRESSION)
        .unwrap_or_else(|| panic!("compression profile fixture disappeared"));
    let material = registries
        .materials()
        .get_material(MATERIAL_WOOD)
        .unwrap_or_else(|| panic!("wood material fixture disappeared"));
    let area = Area::from_square_millimeters(1_234);
    let structural = material
        .properties()
        .structural()
        .unwrap_or_else(|| panic!("wood structural properties disappeared"));
    let expected =
        u128::from(structural.compressive_strength_kpa()) * u128::from(area.square_millimeters());

    assert_eq!(
        calculate_pristine_member_capacity(profile, material, area),
        Some(Force::from_millinewtons(expected))
    );
}

#[test]
fn structural_utilization_projection_handles_ratio_and_zero_capacity() {
    assert_eq!(
        calculate_structural_utilization_ppm(
            Force::from_millinewtons(50),
            Force::from_millinewtons(100),
        ),
        500_000
    );
    assert_eq!(
        calculate_structural_utilization_ppm(Force::ZERO, Force::ZERO),
        0
    );
    assert_eq!(
        calculate_structural_utilization_ppm(Force::from_millinewtons(1), Force::ZERO),
        u128::MAX
    );
}
