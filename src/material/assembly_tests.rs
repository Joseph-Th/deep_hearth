//! Contract tests for material assembly requirements.

use std::collections::BTreeMap;

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

#[test]
fn additive_extension_combines_shared_commodities_without_parallel_rules() {
    let copper = CommodityKey::new(MATERIAL_COPPER, FORM_INGOT);
    let scrap = CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP);
    let base = MaterialAssemblyProfile::new(vec![
        MaterialInputSpec::pure(copper, Mass::from_milligrams(10)),
        MaterialInputSpec::pure(scrap, Mass::from_milligrams(20)),
    ]);
    let additions = MaterialAssemblyProfile::new(vec![MaterialInputSpec::pure(
        copper,
        Mass::from_milligrams(5),
    )]);
    let exact = MaterialAssemblyProfile::new(vec![
        MaterialInputSpec::pure(copper, Mass::from_milligrams(15)),
        MaterialInputSpec::pure(scrap, Mass::from_milligrams(20)),
    ]);
    let wrong = MaterialAssemblyProfile::new(vec![
        MaterialInputSpec::pure(copper, Mass::from_milligrams(14)),
        MaterialInputSpec::pure(scrap, Mass::from_milligrams(20)),
    ]);

    assert!(exact.is_exact_additive_extension_of(&base, &additions));
    assert!(!wrong.is_exact_additive_extension_of(&base, &additions));
}

#[test]
fn assembly_mass_comparison_reports_missing_changed_and_extra_commodities() {
    let copper = CommodityKey::new(MATERIAL_COPPER, FORM_INGOT);
    let scrap = CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP);
    let assembly = MaterialAssemblyProfile::new(vec![
        MaterialInputSpec::pure(copper, Mass::from_milligrams(10)),
        MaterialInputSpec::pure(scrap, Mass::from_milligrams(20)),
    ]);
    let exact = BTreeMap::from([
        (copper, Mass::from_milligrams(10)),
        (scrap, Mass::from_milligrams(20)),
    ]);
    assert_eq!(assembly.first_mass_mismatch(&exact), None);

    let missing = BTreeMap::from([(scrap, Mass::from_milligrams(20))]);
    assert_eq!(
        assembly.first_mass_mismatch(&missing),
        Some((copper, Mass::ZERO, Mass::from_milligrams(10)))
    );

    let changed = BTreeMap::from([
        (copper, Mass::from_milligrams(9)),
        (scrap, Mass::from_milligrams(20)),
    ]);
    assert_eq!(
        assembly.first_mass_mismatch(&changed),
        Some((copper, Mass::from_milligrams(9), Mass::from_milligrams(10)))
    );

    let extra_commodity = CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED);
    let extra = BTreeMap::from([
        (copper, Mass::from_milligrams(10)),
        (scrap, Mass::from_milligrams(20)),
        (extra_commodity, Mass::from_milligrams(1)),
    ]);
    assert_eq!(
        assembly.first_mass_mismatch(&extra),
        Some((extra_commodity, Mass::from_milligrams(1), Mass::ZERO))
    );
}
