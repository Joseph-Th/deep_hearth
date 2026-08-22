//! Tests for the sibling assembly module; isolated so test-only edits do not invalidate production builds.

use super::*;
use crate::content::{FORM_CRUSHED, FORM_MOLTEN, MATERIAL_COPPER, build_registries};

#[test]
fn infrastructure_assembly_rejects_liquid_and_particulate_forms() {
    let registries = build_registries();
    let liquid = CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN);
    let particulate = CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED);

    assert_eq!(
        MaterialAssemblyProfile::new(vec![MaterialInputSpec::new(
            liquid,
            Mass::from_milligrams(1),
        )])
        .validate_infrastructure_references(registries.materials()),
        Err(MaterialAssemblyReferenceError::UnsupportedPhase {
            commodity: liquid,
            phase: MaterialPhase::Liquid,
        })
    );
    assert_eq!(
        MaterialAssemblyProfile::new(vec![MaterialInputSpec::new(
            particulate,
            Mass::from_milligrams(1),
        )])
        .validate_infrastructure_references(registries.materials()),
        Err(MaterialAssemblyReferenceError::UnsupportedParticulateForm {
            commodity: particulate,
        })
    );
}
