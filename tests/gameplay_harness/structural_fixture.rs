//! Structural fixture sizing shared only by gameplay probes that install supported infrastructure.

use deep_hearth::core::quantity::{Area, Force, Length};
use deep_hearth::material::MaterialId;
use deep_hearth::registry::Registries;
use deep_hearth::structural::{
    STRUCTURAL_PARTS_PER_MILLION, StructuralProfileId, calculate_prismatic_material_mass_ceiling,
    calculate_pristine_member_capacity, calculate_structural_utilization_ppm,
    calculate_weight_force_ceiling,
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
    let member_mass =
        calculate_prismatic_material_mass_ceiling(registries.materials(), material, area, length)
            .unwrap_or_else(|error| {
                panic!("gameplay harness support mass resolution failed: {error}")
            });
    let self_weight = calculate_weight_force_ceiling(member_mass, registries.core().gravity());
    let total_load = external_load
        .millinewtons()
        .checked_add(self_weight.millinewtons())
        .unwrap_or_else(|| panic!("gameplay harness support load overflowed"));
    let profile = registries
        .structural()
        .get_profile(profile)
        .unwrap_or_else(|| panic!("gameplay harness structural profile disappeared"));
    let material_definition = registries
        .materials()
        .get_material(material)
        .unwrap_or_else(|| panic!("gameplay harness support material disappeared"));
    let capacity = calculate_pristine_member_capacity(profile, material_definition, area)
        .unwrap_or_else(|| panic!("gameplay harness support material has no structural strengths"));
    calculate_structural_utilization_ppm(Force::from_millinewtons(total_load), capacity)
        <= u128::from(target_utilization_ppm)
}

/// Returns the smallest prismatic support area whose actual material self-weight plus the requested
/// external load stays at or below an authored utilization target.
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
