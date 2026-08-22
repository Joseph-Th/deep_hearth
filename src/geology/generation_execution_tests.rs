//! Tests for the sibling generation execution module; isolated so test-only edits do not invalidate production builds.

use super::*;
use crate::content::{FORM_CRUSHED, FORM_MOLTEN, FORM_ORE, MATERIAL_COPPER, build_registries};
use crate::core::quantity::{Mass, Pressure, Temperature};
use crate::core::time::WorldSeed;
use crate::material::{
    CommodityKey, CompositionComponent, MaterialComposition, MaterialId, MaterialPhase,
};
use crate::spatial::{VoxelBounds, VoxelCoord};

fn bounds(x: i64) -> VoxelBounds {
    VoxelBounds::new(VoxelCoord::new(x, -12, 0), VoxelCoord::new(x + 4, -8, 4))
        .unwrap_or_else(|error| panic!("geological generation bounds failed: {error}"))
}

#[test]
fn generated_geological_owner_rejects_liquid_material_form_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x6E00_0011));
    let spec = GeneratedDepositSpec::new(
        bounds(0),
        CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN),
        Mass::from_milligrams(100),
        Temperature::from_millikelvin(1_357_770),
        Pressure::from_pascals(350_000_000),
        MaterialComposition::pure(MATERIAL_COPPER),
    )
    .unwrap_or_else(|error| panic!("liquid geology specification fixture failed: {error}"));
    let before = state.clone();

    assert_eq!(
        insert_generated_deposit(&registries, &mut state, spec),
        Err(InsertGeneratedDepositError::UnsupportedPhase {
            form: FORM_MOLTEN,
            phase: MaterialPhase::Liquid,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn generated_geological_owner_rejects_processed_particulate_form_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x6E00_0012));
    let spec = GeneratedDepositSpec::new(
        bounds(0),
        CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED),
        Mass::from_milligrams(100),
        Temperature::from_millikelvin(300_000),
        Pressure::from_pascals(350_000_000),
        MaterialComposition::pure(MATERIAL_COPPER),
    )
    .unwrap_or_else(|error| panic!("particulate geology specification fixture failed: {error}"));
    let before = state.clone();

    assert_eq!(
        insert_generated_deposit(&registries, &mut state, spec),
        Err(InsertGeneratedDepositError::UnsupportedParticulateForm { form: FORM_CRUSHED })
    );
    assert_eq!(state, before);
}

#[test]
fn generated_deposit_insertion_resolves_all_material_references_before_mutation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x6E00_0000));
    let unknown = MaterialId::new(999_999);
    let unknown_host = GeneratedDepositSpec::new(
        bounds(0),
        CommodityKey::new(unknown, FORM_ORE),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(300_000),
        Pressure::from_pascals(350_000_000),
        MaterialComposition::pure(unknown),
    )
    .unwrap_or_else(|error| panic!("unknown-host deposit specification failed locally: {error}"));
    let before = state.clone();
    assert_eq!(
        insert_generated_deposit(&registries, &mut state, unknown_host),
        Err(InsertGeneratedDepositError::UnknownMaterial { material: unknown })
    );
    assert_eq!(state, before);

    let mixed = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, 500_000),
        CompositionComponent::new(unknown, 500_000),
    ])
    .unwrap_or_else(|error| panic!("unknown-constituent composition fixture failed: {error}"));
    let unknown_constituent = GeneratedDepositSpec::new(
        bounds(0),
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(300_000),
        Pressure::from_pascals(350_000_000),
        mixed,
    )
    .unwrap_or_else(|error| {
        panic!("unknown-constituent deposit specification failed locally: {error}")
    });
    assert_eq!(
        insert_generated_deposit(&registries, &mut state, unknown_constituent),
        Err(InsertGeneratedDepositError::UnknownCompositionMaterial { material: unknown })
    );
    assert_eq!(state, before);
}
