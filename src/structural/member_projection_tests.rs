//! Contract tests for canonical pristine prismatic-member load projection.

use super::*;
use crate::content::{
    MATERIAL_WATER, MATERIAL_WOOD, STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
};
use crate::core::quantity::{Area, Force, Length, Mass};

#[test]
fn prismatic_member_projection_composes_canonical_mass_load_capacity_and_utilization() {
    let registries = build_registries();
    let area = Area::from_square_millimeters(1_000);
    let length = Length::from_micrometers(10_000);
    let external_load = Force::from_millinewtons(1_000);

    let projection = project_prismatic_member_load(
        &registries,
        PrismaticMemberLoadRequest::new(
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            length,
            area,
            external_load,
        ),
    )
    .unwrap_or_else(|error| panic!("member projection failed: {error}"));

    assert_eq!(projection.material_mass(), Mass::from_milligrams(6_500));
    let expected_self_weight =
        calculate_weight_force_ceiling(projection.material_mass(), registries.core().gravity());
    assert_eq!(projection.self_weight(), expected_self_weight);
    assert_eq!(
        projection.total_load(),
        Force::from_millinewtons(
            external_load.millinewtons() + expected_self_weight.millinewtons()
        )
    );
    let profile = registries
        .structural()
        .get_profile(STRUCTURAL_PROFILE_AXIAL_COMPRESSION)
        .unwrap_or_else(|| panic!("compression profile disappeared"));
    let material = registries
        .materials()
        .get_material(MATERIAL_WOOD)
        .unwrap_or_else(|| panic!("wood material disappeared"));
    let expected_capacity = calculate_pristine_member_capacity(profile, material, area)
        .unwrap_or_else(|| panic!("wood structural capacity disappeared"));
    assert_eq!(projection.pristine_capacity(), expected_capacity);
    assert_eq!(
        projection.utilization_ppm(),
        calculate_structural_utilization_ppm(projection.total_load(), expected_capacity)
    );
}

#[test]
fn prismatic_member_projection_rejects_unknown_and_nonstructural_definitions() {
    let registries = build_registries();
    let length = Length::from_micrometers(1_000);
    let area = Area::from_square_millimeters(1);

    let unknown_profile = StructuralProfileId::new(u32::MAX);
    assert_eq!(
        project_prismatic_member_load(
            &registries,
            PrismaticMemberLoadRequest::new(
                unknown_profile,
                MATERIAL_WOOD,
                length,
                area,
                Force::ZERO,
            ),
        ),
        Err(StructuralMemberLoadProjectionError::UnknownProfile {
            profile: unknown_profile
        })
    );

    let unknown_material = MaterialId::new(u32::MAX);
    assert_eq!(
        project_prismatic_member_load(
            &registries,
            PrismaticMemberLoadRequest::new(
                STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
                unknown_material,
                length,
                area,
                Force::ZERO,
            ),
        ),
        Err(StructuralMemberLoadProjectionError::UnknownMaterial {
            material: unknown_material
        })
    );

    assert_eq!(
        project_prismatic_member_load(
            &registries,
            PrismaticMemberLoadRequest::new(
                STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
                MATERIAL_WATER,
                length,
                area,
                Force::ZERO,
            ),
        ),
        Err(StructuralMemberLoadProjectionError::NonStructuralMaterial {
            material: MATERIAL_WATER
        })
    );
}

#[test]
fn prismatic_member_projection_rejects_invalid_geometry_and_load_overflow() {
    let registries = build_registries();
    assert_eq!(
        project_prismatic_member_load(
            &registries,
            PrismaticMemberLoadRequest::new(
                STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
                MATERIAL_WOOD,
                Length::from_micrometers(1_000),
                Area::ZERO,
                Force::ZERO,
            ),
        ),
        Err(StructuralMemberLoadProjectionError::Geometry(
            StructuralGeometryError::ZeroCrossSection
        ))
    );
    assert_eq!(
        project_prismatic_member_load(
            &registries,
            PrismaticMemberLoadRequest::new(
                STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
                MATERIAL_WOOD,
                Length::from_micrometers(1_000),
                Area::from_square_millimeters(1),
                Force::from_millinewtons(u128::MAX),
            ),
        ),
        Err(StructuralMemberLoadProjectionError::LoadOverflow)
    );
}
