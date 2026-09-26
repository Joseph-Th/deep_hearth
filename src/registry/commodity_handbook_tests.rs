//! Definition-derived commodity handbook contracts.

use super::*;
use crate::content::{
    EQUIPMENT_STONE_QUARRY_PICK, FORM_CHEST_BODY, FORM_CRUSHED, FORM_HANDLE, FORM_INGOT,
    FORM_MOLTEN, FORM_NATIVE_METAL, FORM_ORE, MATERIAL_COPPER, MATERIAL_WOOD,
    PROCESS_CAST_PURE_COPPER, PROCESS_CRUSH_ORE, PROCESS_HAND_BREAK_ORE,
    PROCESS_HAND_SORT_NATIVE_COPPER, PROCESS_MELT_PURE_COPPER, PROCESS_SHAPE_WOOD_HANDLE,
    STORAGE_TIMBER_PROVISIONS_CHEST, build_registries,
};
use crate::material::CommodityKey;

#[test]
fn handle_handbook_links_crafting_and_physical_equipment_uses() {
    let registries = build_registries();
    let handle = CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE);
    let entry = registries
        .commodity_handbook_entry(handle)
        .unwrap_or_else(|| panic!("wood handle handbook entry disappeared"));

    assert_eq!(entry.name(), "wood handle");
    assert_eq!(entry.material().name(), "wood");
    assert_eq!(entry.form().name(), "handle");
    assert!(entry.sources().iter().any(|source| matches!(
        source,
        CommoditySource::ManualCraft { process, .. } if *process == PROCESS_SHAPE_WOOD_HANDLE
    )));
    assert!(entry.uses().iter().any(|usage| matches!(
        usage,
        CommodityUse::EquipmentAssembly { equipment, .. }
            if *equipment == EQUIPMENT_STONE_QUARRY_PICK
    )));
}

#[test]
fn copper_handbook_exposes_ore_dressing_without_claiming_current_reachability() {
    let registries = build_registries();
    let ore = registries
        .commodity_handbook_entry(CommodityKey::new(MATERIAL_COPPER, FORM_ORE))
        .unwrap_or_else(|| panic!("copper ore handbook entry disappeared"));
    assert!(ore.uses().iter().any(|usage| matches!(
        usage,
        CommodityUse::OreProcessing { process }
            if *process == PROCESS_HAND_BREAK_ORE || *process == PROCESS_CRUSH_ORE
    )));

    let crushed = registries
        .commodity_handbook_entry(CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED))
        .unwrap_or_else(|| panic!("crushed copper handbook entry disappeared"));
    assert!(crushed.sources().iter().any(|source| matches!(
        source,
        CommoditySource::OreProcessing { process }
            if *process == PROCESS_HAND_BREAK_ORE || *process == PROCESS_CRUSH_ORE
    )));

    let native = registries
        .commodity_handbook_entry(CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL))
        .unwrap_or_else(|| panic!("native copper handbook entry disappeared"));
    assert_eq!(native.name(), "native copper");
    assert!(native.sources().iter().any(|source| matches!(
        source,
        CommoditySource::OreProcessing { process } if *process == PROCESS_HAND_SORT_NATIVE_COPPER
    )));
}

#[test]
fn copper_handbook_exposes_melting_and_casting_as_one_material_loop() {
    let registries = build_registries();
    let ingot = registries
        .commodity_handbook_entry(CommodityKey::new(MATERIAL_COPPER, FORM_INGOT))
        .unwrap_or_else(|| panic!("copper ingot handbook entry disappeared"));
    assert!(ingot.uses().iter().any(|usage| matches!(
        usage,
        CommodityUse::ThermalPhaseChange { process } if *process == PROCESS_MELT_PURE_COPPER
    )));
    assert!(ingot.sources().iter().any(|source| matches!(
        source,
        CommoditySource::ThermalPhaseChange { process } if *process == PROCESS_CAST_PURE_COPPER
    )));

    let molten = registries
        .commodity_handbook_entry(CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN))
        .unwrap_or_else(|| panic!("molten copper handbook entry disappeared"));
    assert!(molten.sources().iter().any(|source| matches!(
        source,
        CommoditySource::ThermalPhaseChange { process } if *process == PROCESS_MELT_PURE_COPPER
    )));
    assert!(molten.uses().iter().any(|usage| matches!(
        usage,
        CommodityUse::ThermalPhaseChange { process } if *process == PROCESS_CAST_PURE_COPPER
    )));
}

#[test]
fn chest_body_handbook_links_storage_construction_and_recovery() {
    let registries = build_registries();
    let body = CommodityKey::new(MATERIAL_WOOD, FORM_CHEST_BODY);
    let entry = registries
        .commodity_handbook_entry(body)
        .unwrap_or_else(|| panic!("timber chest body handbook entry disappeared"));

    assert!(entry.uses().iter().any(|usage| matches!(
        usage,
        CommodityUse::StorageConstruction { storage, .. }
            if *storage == STORAGE_TIMBER_PROVISIONS_CHEST
    )));
    assert!(entry.sources().iter().any(|source| matches!(
        source,
        CommoditySource::StorageDismantling { storage, .. }
            if *storage == STORAGE_TIMBER_PROVISIONS_CHEST
    )));
}

#[test]
fn unknown_commodity_has_no_handbook_entry() {
    let registries = build_registries();
    assert!(
        registries
            .commodity_handbook_entry(CommodityKey::new(
                crate::material::MaterialId::new(999_999),
                crate::material::FormId::new(999),
            ))
            .is_none()
    );
}
