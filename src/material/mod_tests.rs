//! Tests for the sibling mod module; isolated so test-only edits do not invalidate production builds.

use super::*;
use crate::core::quantity::Length;

fn make_test_properties() -> MaterialProperties {
    MaterialProperties::new(
        1_000,
        ThermalProperties::new(1_000, None),
        Some(StructuralProperties::new(10, 10)),
    )
}

fn make_fusible_properties(melting_point: Temperature) -> MaterialProperties {
    MaterialProperties::new(
        1_000,
        ThermalProperties::new(1_000, Some(FusionProperties::new(melting_point, 200_000))),
        Some(StructuralProperties::new(10, 10)),
    )
}

#[test]
fn liquid_commodity_requires_fusion_properties() {
    let mut registry = MaterialRegistry::new();
    let material = MaterialId::new(3);
    let liquid = FormId::new(8);
    registry.register_material(MaterialDefinition::new(
        material,
        "non-melting test material",
        make_test_properties(),
    ));
    registry.register_form(FormDefinition::new(
        liquid,
        "test liquid",
        MaterialPhase::Liquid,
        ParticleSizeStatePolicy::Untracked,
        MaterialFormCohesion::Loose,
    ));

    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            registry.register_commodity(CommodityKey::new(material, liquid));
        }))
        .is_err()
    );
    assert!(!registry.has_commodity(CommodityKey::new(material, liquid)));
}

#[test]
fn solid_phase_validation_checks_every_constituent_against_its_own_fusion_boundary() {
    let low_melting = MaterialId::new(3);
    let high_melting = MaterialId::new(4);
    let solid = FormId::new(7);
    let low_boundary = Temperature::from_millikelvin(400_000);
    let high_boundary = Temperature::from_millikelvin(800_000);
    let temperature = Temperature::from_millikelvin(500_000);
    let mut registry = MaterialRegistry::new();
    registry.register_material(MaterialDefinition::new(
        low_melting,
        "low-melting fixture",
        make_fusible_properties(low_boundary),
    ));
    registry.register_material(MaterialDefinition::new(
        high_melting,
        "high-melting fixture",
        make_fusible_properties(high_boundary),
    ));
    registry.register_form(FormDefinition::new(
        solid,
        "solid fixture",
        MaterialPhase::Solid,
        ParticleSizeStatePolicy::Untracked,
        MaterialFormCohesion::Consolidated,
    ));
    let composition = MaterialComposition::new(vec![
        CompositionComponent::new(low_melting, 500_000),
        CompositionComponent::new(high_melting, 500_000),
    ])
    .unwrap_or_else(|error| panic!("phase composition fixture failed: {error}"));

    assert_eq!(
        validate_material_phase_state(
            &registry,
            CommodityKey::new(high_melting, solid),
            &composition,
            temperature,
        ),
        Err(MaterialPhaseStateError::SolidAboveMeltingPoint {
            material: low_melting,
            temperature,
            melting_point: low_boundary,
        })
    );
}

#[test]
fn liquid_phase_validation_enforces_purity_host_identity_and_fusion_boundary() {
    let material = MaterialId::new(3);
    let other = MaterialId::new(4);
    let liquid = FormId::new(8);
    let melting_point = Temperature::from_millikelvin(700_000);
    let mut registry = MaterialRegistry::new();
    registry.register_material(MaterialDefinition::new(
        material,
        "liquid fixture",
        make_fusible_properties(melting_point),
    ));
    registry.register_material(MaterialDefinition::new(
        other,
        "other fixture",
        make_fusible_properties(melting_point),
    ));
    registry.register_form(FormDefinition::new(
        liquid,
        "liquid fixture form",
        MaterialPhase::Liquid,
        ParticleSizeStatePolicy::Untracked,
        MaterialFormCohesion::Loose,
    ));
    let impure = MaterialComposition::new(vec![
        CompositionComponent::new(material, 900_000),
        CompositionComponent::new(other, 100_000),
    ])
    .unwrap_or_else(|error| panic!("liquid composition fixture failed: {error}"));
    let pure = MaterialComposition::pure(material);
    let hot = Temperature::from_millikelvin(800_000);

    assert_eq!(
        validate_material_phase_state(&registry, CommodityKey::new(material, liquid), &impure, hot,),
        Err(MaterialPhaseStateError::LiquidRequiresPureComposition)
    );
    assert_eq!(
        validate_material_phase_state(&registry, CommodityKey::new(other, liquid), &pure, hot,),
        Err(MaterialPhaseStateError::LiquidHostMismatch {
            host: other,
            pure: material,
        })
    );
    assert_eq!(
        validate_material_phase_state(
            &registry,
            CommodityKey::new(material, liquid),
            &pure,
            Temperature::from_millikelvin(699_999),
        ),
        Err(MaterialPhaseStateError::LiquidBelowMeltingPoint {
            material,
            temperature: Temperature::from_millikelvin(699_999),
            melting_point,
        })
    );
    assert_eq!(
        validate_material_phase_state(
            &registry,
            CommodityKey::new(material, liquid),
            &pure,
            melting_point,
        ),
        Ok(())
    );
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
