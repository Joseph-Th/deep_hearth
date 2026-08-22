//! Tests for the sibling mod module; isolated so test-only edits do not invalidate production builds.

use super::*;
use crate::content::{MATERIAL_COPPER, MATERIAL_SLAG, build_registries};
use crate::material::{CompositionComponent, MaterialComposition};

#[test]
fn pure_copper_sensible_heat_is_exact_at_integer_scales() {
    let registries = build_registries();
    let composition = MaterialComposition::pure(MATERIAL_COPPER);

    let heat = match calculate_sensible_heat(
        registries.materials(),
        Mass::from_milligrams(10_000),
        &composition,
        Temperature::from_millikelvin(300_000),
        Temperature::from_millikelvin(301_000),
    ) {
        Ok(heat) => heat,
        Err(error) => panic!("thermal calculation failed: {error}"),
    };

    assert_eq!(heat.direction(), HeatDirection::IntoMaterial);
    assert_eq!(heat.energy(), Energy::from_nanojoules(3_850_000_000));
}

#[test]
fn mixed_composition_weights_specific_heat_by_normalized_fraction() {
    let registries = build_registries();
    let composition = match MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, 500_000),
        CompositionComponent::new(MATERIAL_SLAG, 500_000),
    ]) {
        Ok(composition) => composition,
        Err(error) => panic!("composition fixture failed: {error}"),
    };
    let copper_cp = match registries.materials().get_material(MATERIAL_COPPER) {
        Some(material) => material.properties().thermal().specific_heat_j_per_kg_k(),
        None => panic!("built-in copper disappeared"),
    };
    let slag_cp = match registries.materials().get_material(MATERIAL_SLAG) {
        Some(material) => material.properties().thermal().specific_heat_j_per_kg_k(),
        None => panic!("built-in slag disappeared"),
    };
    let expected_energy =
        1_000_u128 * 1_000_u128 * u128::from(copper_cp + slag_cp) * 500_000_u128 / 1_000_000_u128;

    let heat = match calculate_sensible_heat(
        registries.materials(),
        Mass::from_milligrams(1_000),
        &composition,
        Temperature::from_millikelvin(300_000),
        Temperature::from_millikelvin(301_000),
    ) {
        Ok(heat) => heat,
        Err(error) => panic!("mixed thermal calculation failed: {error}"),
    };

    assert_eq!(heat.energy().nanojoules(), expected_energy);
}

#[test]
fn sensible_heat_reaches_but_does_not_cross_a_melting_point() {
    let registries = build_registries();
    let composition = MaterialComposition::pure(MATERIAL_COPPER);
    let copper = match registries.materials().get_material(MATERIAL_COPPER) {
        Some(material) => material,
        None => panic!("built-in copper disappeared"),
    };
    let melting_point = match copper.properties().thermal().melting_point() {
        Some(value) => value,
        None => panic!("built-in copper has no melting point"),
    };

    let to_boundary = calculate_sensible_heat(
        registries.materials(),
        Mass::from_milligrams(1_000),
        &composition,
        Temperature::from_millikelvin(melting_point.millikelvin() - 1_000),
        melting_point,
    )
    .unwrap_or_else(|error| panic!("heating exactly to the phase boundary failed: {error}"));
    assert_eq!(to_boundary.direction(), HeatDirection::IntoMaterial);
    assert!(!to_boundary.energy().is_zero());

    let result = calculate_sensible_heat(
        registries.materials(),
        Mass::from_milligrams(1_000),
        &composition,
        Temperature::from_millikelvin(melting_point.millikelvin() - 1_000),
        Temperature::from_millikelvin(melting_point.millikelvin() + 1_000),
    );

    assert_eq!(
        result,
        Err(SensibleHeatError::PhaseBoundaryCrossed {
            material: MATERIAL_COPPER,
            melting_point,
        })
    );
}

#[test]
fn copper_fusion_heat_uses_authored_latent_energy_exactly() {
    let registries = build_registries();
    let fusion = match calculate_fusion_heat(
        registries.materials(),
        Mass::from_milligrams(1_000),
        MATERIAL_COPPER,
    ) {
        Ok(fusion) => fusion,
        Err(error) => panic!("fusion calculation failed: {error}"),
    };

    assert_eq!(fusion.energy(), Energy::from_nanojoules(205_000_000_000));
    assert_eq!(
        fusion.melting_point(),
        Temperature::from_millikelvin(1_357_770)
    );
}

#[test]
fn liquid_internal_energy_adds_latent_heat_at_the_phase_boundary() {
    let registries = build_registries();
    let composition = MaterialComposition::pure(MATERIAL_COPPER);
    let melting_point = Temperature::from_millikelvin(1_357_770);
    let mass = Mass::from_milligrams(1_000);
    let solid = match calculate_material_thermal_energy(
        registries.materials(),
        mass,
        crate::material::CommodityKey::new(MATERIAL_COPPER, crate::content::FORM_INGOT),
        &composition,
        melting_point,
    ) {
        Ok(energy) => energy,
        Err(error) => panic!("solid internal-energy calculation failed: {error}"),
    };
    let liquid = match calculate_material_thermal_energy(
        registries.materials(),
        mass,
        crate::material::CommodityKey::new(MATERIAL_COPPER, crate::content::FORM_MOLTEN),
        &composition,
        melting_point,
    ) {
        Ok(energy) => energy,
        Err(error) => panic!("liquid internal-energy calculation failed: {error}"),
    };

    assert_eq!(
        liquid.checked_sub(solid),
        Some(Energy::from_nanojoules(205_000_000_000))
    );
}

#[test]
fn phase_sensible_heat_allows_liquid_to_heat_up_from_fusion_boundary() {
    let registries = build_registries();
    let composition = MaterialComposition::pure(MATERIAL_COPPER);
    let current = Temperature::from_millikelvin(1_357_770);
    let target = Temperature::from_millikelvin(1_400_000);

    let heat = match calculate_phase_sensible_heat(
        registries.materials(),
        Mass::from_milligrams(1_000),
        CommodityKey::new(MATERIAL_COPPER, crate::content::FORM_MOLTEN),
        &composition,
        current,
        target,
    ) {
        Ok(heat) => heat,
        Err(error) => panic!("liquid sensible heating failed: {error}"),
    };

    assert_eq!(heat.direction(), HeatDirection::IntoMaterial);
    assert_eq!(heat.energy(), Energy::from_nanojoules(16_258_550_000));
}

#[test]
fn phase_sensible_heat_rejects_solid_target_above_fusion_boundary() {
    let registries = build_registries();
    let composition = MaterialComposition::pure(MATERIAL_COPPER);
    let target = Temperature::from_millikelvin(1_357_771);

    assert!(matches!(
        calculate_phase_sensible_heat(
            registries.materials(),
            Mass::from_milligrams(1_000),
            CommodityKey::new(MATERIAL_COPPER, crate::content::FORM_INGOT),
            &composition,
            Temperature::from_millikelvin(1_357_770),
            target,
        ),
        Err(PhaseSensibleHeatError::InvalidTargetState(
            MaterialPhaseStateError::SolidAboveMeltingPoint {
                material: _material,
                temperature: _temperature,
                melting_point: _melting_point,
            }
        ))
    ));
}
