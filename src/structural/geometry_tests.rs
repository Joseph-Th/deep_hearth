//! Contract tests for structural geometry.

use super::*;
use crate::content::{MATERIAL_COPPER, MATERIAL_WOOD, build_registries};

#[test]
fn prismatic_volume_preserves_sub_millimeter_length() {
    assert_eq!(
        calculate_prismatic_volume_ceiling(
            Area::from_square_millimeters(1_000),
            Length::from_micrometers(1),
        ),
        Ok(Volume::from_microliters(1))
    );
}

#[test]
fn material_mass_uses_density_without_compounded_volume_rounding() {
    let registries = build_registries();
    let area = Area::from_square_millimeters(1);
    let length = Length::from_micrometers(1);

    assert_eq!(
        calculate_prismatic_material_mass_ceiling(
            registries.materials(),
            MATERIAL_WOOD,
            area,
            length,
        ),
        Ok(Mass::from_milligrams(1))
    );
    assert_eq!(
        calculate_prismatic_material_mass_ceiling(
            registries.materials(),
            MATERIAL_COPPER,
            area,
            length,
        ),
        Ok(Mass::from_milligrams(1))
    );
}

#[test]
fn equal_geometry_requires_more_dense_material_mass() {
    let registries = build_registries();
    let area = Area::from_square_millimeters(1_000);
    let length = Length::from_micrometers(10_000);
    let wood = calculate_prismatic_material_mass_ceiling(
        registries.materials(),
        MATERIAL_WOOD,
        area,
        length,
    );
    let copper = calculate_prismatic_material_mass_ceiling(
        registries.materials(),
        MATERIAL_COPPER,
        area,
        length,
    );

    assert_eq!(wood, Ok(Mass::from_milligrams(6_500)));
    assert_eq!(copper, Ok(Mass::from_milligrams(89_600)));
}
