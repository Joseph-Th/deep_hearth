//! Structural fixture sizing shared only by gameplay probes that install supported infrastructure.

use deep_hearth::core::quantity::{Area, Force, Length};
use deep_hearth::material::MaterialId;
use deep_hearth::registry::Registries;
use deep_hearth::structural::{
    PrismaticMemberLoadRequest, STRUCTURAL_PARTS_PER_MILLION, StructuralProfileId,
    project_prismatic_member_load,
};

fn support_area_meets_utilization(
    registries: &Registries,
    material: MaterialId,
    profile: StructuralProfileId,
    length: Length,
    external_load: Force,
    target_utilization_ppm: u32,
    area_mm2: u64,
) -> bool {
    let area = Area::from_square_millimeters(area_mm2);
    project_prismatic_member_load(
        registries,
        PrismaticMemberLoadRequest::new(profile, material, length, area, external_load),
    )
    .unwrap_or_else(|error| panic!("gameplay harness support projection failed: {error}"))
    .is_within_utilization_limit(target_utilization_ppm)
}

/// Returns a compact prismatic support area whose canonical self-weight plus the requested external
/// load stays at or below an authored utilization target.
pub(super) fn support_area_for_utilization(
    registries: &Registries,
    material: MaterialId,
    profile: StructuralProfileId,
    length: Length,
    external_load: Force,
    target_utilization_ppm: u32,
) -> Area {
    assert!((1..=STRUCTURAL_PARTS_PER_MILLION).contains(&target_utilization_ppm));
    let fits = |area_mm2| {
        support_area_meets_utilization(
            registries,
            material,
            profile,
            length,
            external_load,
            target_utilization_ppm,
            area_mm2,
        )
    };
    let mut upper = 1_u64;
    while !fits(upper) {
        let next = upper.saturating_mul(2);
        assert!(
            next != upper,
            "gameplay harness could not size a legal structural support"
        );
        upper = next;
    }
    let mut lower = 1_u64;
    while lower < upper {
        let midpoint = lower + (upper - lower) / 2;
        if fits(midpoint) {
            upper = midpoint;
        } else {
            lower = midpoint + 1;
        }
    }
    Area::from_square_millimeters(lower)
}
