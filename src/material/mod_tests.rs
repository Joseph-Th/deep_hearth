//! Tests for the sibling mod module; isolated so test-only edits do not invalidate production builds.

use super::*;
use crate::core::quantity::Length;

fn make_test_properties() -> MaterialProperties {
    MaterialProperties::new(
        1_000,
        ThermalProperties::new(1_000, None, 100),
        MechanicalProperties::new(10, 10, 10),
        ElectricalProperties::new(None),
    )
}

#[test]
fn commodity_requires_explicit_material_form_authoring() {
    let mut registry = MaterialRegistry::new();
    let material = MaterialId::new(3);
    let form = FormId::new(7);
    registry.register_material(MaterialDefinition::new(
        material,
        "test material",
        make_test_properties(),
    ));

    assert!(!registry.has_commodity(CommodityKey::new(material, form)));

    registry.register_form(FormDefinition::new(
        form,
        "test form",
        MaterialPhase::Solid,
        ParticleSizeStatePolicy::Untracked,
        MaterialFormCohesion::Consolidated,
    ));
    assert!(!registry.has_commodity(CommodityKey::new(material, form)));

    registry.register_commodity(CommodityKey::new(material, form));
    assert!(registry.has_commodity(CommodityKey::new(material, form)));
}

#[test]
fn particle_size_range_and_form_policy_reject_ambiguous_runtime_state() {
    assert_eq!(
        ParticleSizeRange::new(Length::ZERO, Length::from_micrometers(10)),
        Err(ParticleSizeRangeError::ZeroMinimumDiameter)
    );
    assert_eq!(
        ParticleSizeRange::new(Length::from_micrometers(11), Length::from_micrometers(10),),
        Err(ParticleSizeRangeError::MinimumExceedsMaximum {
            minimum: Length::from_micrometers(11),
            maximum: Length::from_micrometers(10),
        })
    );

    let mut registry = MaterialRegistry::new();
    let form = FormId::new(9);
    registry.register_form(FormDefinition::new(
        form,
        "particulate fixture",
        MaterialPhase::Solid,
        ParticleSizeStatePolicy::Required,
        MaterialFormCohesion::Loose,
    ));
    let commodity = CommodityKey::new(MaterialId::new(1), form);
    assert_eq!(
        validate_material_particle_size_state(&registry, commodity, None),
        Err(ParticleSizeStateError::MissingRequired { form })
    );
    let range =
        match ParticleSizeRange::new(Length::from_micrometers(1), Length::from_micrometers(10)) {
            Ok(range) => range,
            Err(error) => panic!("particle-size fixture failed: {error}"),
        };
    let distribution = ParticleSizeDistribution::from(range);
    assert_eq!(
        validate_material_particle_size_state(&registry, commodity, Some(&distribution)),
        Ok(())
    );
}
