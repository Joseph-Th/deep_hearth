//! Tests for the sibling volume module; isolated so test-only edits do not invalidate production builds.

use super::*;
use crate::content::{MATERIAL_COPPER, MATERIAL_SLAG, build_registries};
use crate::material::{CompositionComponent, MaterialComposition};

#[test]
fn pure_material_volume_uses_authored_density_and_rounds_up() {
    let registries = build_registries();
    let composition = MaterialComposition::pure(MATERIAL_COPPER);

    let volume = match calculate_volume_ceiling(
        registries.materials(),
        Mass::from_milligrams(1_000),
        &composition,
    ) {
        Ok(volume) => volume,
        Err(error) => panic!("volume calculation failed: {error}"),
    };

    assert_eq!(volume, Volume::from_microliters(112));
}

#[test]
fn mixed_material_volume_is_additive_and_conservatively_rounded() {
    let registries = build_registries();
    let composition = match MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, 500_000),
        CompositionComponent::new(MATERIAL_SLAG, 500_000),
    ]) {
        Ok(composition) => composition,
        Err(error) => panic!("composition fixture failed: {error}"),
    };

    let mixed = match calculate_volume_ceiling(
        registries.materials(),
        Mass::from_milligrams(1_000),
        &composition,
    ) {
        Ok(volume) => volume,
        Err(error) => panic!("mixed volume calculation failed: {error}"),
    };
    let copper_half = match calculate_volume_ceiling(
        registries.materials(),
        Mass::from_milligrams(500),
        &MaterialComposition::pure(MATERIAL_COPPER),
    ) {
        Ok(volume) => volume,
        Err(error) => panic!("copper volume calculation failed: {error}"),
    };
    let slag_half = match calculate_volume_ceiling(
        registries.materials(),
        Mass::from_milligrams(500),
        &MaterialComposition::pure(MATERIAL_SLAG),
    ) {
        Ok(volume) => volume,
        Err(error) => panic!("slag volume calculation failed: {error}"),
    };

    assert_eq!(
        mixed,
        Volume::from_microliters(copper_half.microliters() + slag_half.microliters())
    );
}
