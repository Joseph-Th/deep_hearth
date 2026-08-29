//! Contract tests for material assembly requirements.

use super::*;
use crate::content::{
    FORM_CRUSHED, FORM_INGOT, FORM_MOLTEN, FORM_SCRAP, MATERIAL_COPPER, build_registries,
};

#[test]
fn infrastructure_assembly_rejects_every_unconsolidated_form() {
    let registries = build_registries();
    let liquid = CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN);
    let particulate = CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED);
    let scrap = CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP);

    for commodity in [liquid, particulate, scrap] {
        assert_eq!(
            MaterialAssemblyProfile::new(vec![MaterialInputSpec::pure(
                commodity,
                Mass::from_milligrams(1),
            )])
            .validate_infrastructure_references(registries.materials()),
            Err(MaterialAssemblyReferenceError::UnconsolidatedForm { commodity })
        );
    }
}

#[test]
fn infrastructure_assembly_requires_pure_input_specs_at_authoring_time() {
    let commodity = CommodityKey::new(MATERIAL_COPPER, FORM_INGOT);
    let result = std::panic::catch_unwind(|| {
        MaterialAssemblyProfile::new(vec![MaterialInputSpec::new(
            commodity,
            Mass::from_milligrams(1),
        )])
    });

    assert!(result.is_err());
}
