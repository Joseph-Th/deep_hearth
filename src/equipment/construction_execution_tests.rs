//! Contract tests for material-backed equipment construction.

use super::*;
use crate::content::{
    EQUIPMENT_STONE_PICK, FORM_HANDLE, FORM_TOOL, MATERIAL_STONE, MATERIAL_WOOD, build_registries,
};
use crate::core::quantity::Temperature;
use crate::core::state::StateValidationError;
use crate::core::time::WorldSeed;
use crate::energy::calculate_explicit_energy_accounting;
use crate::equipment::EquipmentValidationError;
use crate::inventory::{
    add_solid_stockpile_for_test, deposit_composed_lot_for_test, deposit_lot_for_test,
};
use crate::material::{CommodityKey, CompositionComponent, MaterialComposition};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};

fn unassembled_pick_fixture() -> (Registries, AppState, crate::inventory::StockpileId) {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA55E_00E0));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("assembly exhaustion source fixture failed: {error}"));
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(800_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(200_000),
        ),
    ] {
        deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            commodity,
            mass,
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("assembly exhaustion material fixture failed: {error}"));
    }
    (registries, state, source)
}

#[test]
fn assembly_rejects_exhausted_equipment_id_without_consuming_material() {
    let (registries, state, source) = unassembled_pick_fixture();
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("equipment id exhaustion serialization failed: {error}"));
    encoded["state"]["systems"]["equipment"]["next_equipment_id"] = serde_json::json!(u32::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("equipment id exhaustion decode failed: {error}"));
    let loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("equipment id exhaustion fixture should load: {error}"));
    let before = loaded.clone();

    assert_eq!(
        validate_assemble_equipment(&registries, &loaded, EQUIPMENT_STONE_PICK, source).err(),
        Some(EquipmentAssemblyError::EquipmentIdExhausted)
    );
    assert_eq!(loaded, before);
}

#[test]
fn assembly_rejects_exhausted_equipment_revision_without_consuming_material() {
    let (registries, state, source) = unassembled_pick_fixture();
    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("equipment revision exhaustion serialization failed: {error}")
        });
    encoded["state"]["systems"]["equipment"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("equipment revision exhaustion decode failed: {error}"));
    let loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("equipment revision exhaustion fixture should load: {error}")
    });
    let before = loaded.clone();

    assert_eq!(
        validate_assemble_equipment(&registries, &loaded, EQUIPMENT_STONE_PICK, source).err(),
        Some(EquipmentAssemblyError::EquipmentRevisionExhausted)
    );
    assert_eq!(loaded, before);
}

#[test]
fn composite_pick_requires_both_authored_inputs_and_rejects_forged_embodiment() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA55E_0001));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("assembly source fixture failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
        Mass::from_milligrams(800_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("assembly stone fixture failed: {error}"));

    assert_eq!(
        validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_PICK, source).err(),
        Some(EquipmentAssemblyError::InsufficientMaterial {
            stockpile: source,
            available: Mass::ZERO,
            required: Mass::from_milligrams(200_000),
        })
    );
    deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
        Mass::from_milligrams(200_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("assembly handle fixture failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("assembly matter-before audit failed: {error}"))
        .total();
    let energy_before = calculate_explicit_energy_accounting(&registries, &state)
        .unwrap_or_else(|error| panic!("assembly energy-before audit failed: {error}"))
        .total()
        .unwrap_or_else(|| panic!("assembly energy-before total overflowed"));
    let equipment = validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_PICK, source)
        .unwrap_or_else(|error| panic!("composite pick validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("composite pick commit failed: {error}"));
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("assembly matter-after audit failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(
        calculate_explicit_energy_accounting(&registries, &state)
            .unwrap_or_else(|error| panic!("assembly energy-after audit failed: {error}"))
            .total(),
        Some(energy_before),
        "equipment assembly must transfer exact material thermal energy into embodiment"
    );

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("composite pick serialization failed: {error}"));
    encoded["state"]["systems"]["equipment"]["records"][equipment.value().to_string()]["embodied_material"]
        [0]["mass"] = serde_json::json!(200_001_u64);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("composite pick tamper decode failed: {error}"));

    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Equipment(
            EquipmentValidationError::EmbodiedTraceMassMismatch {
                equipment,
                stored: Mass::from_milligrams(1_000_000),
                traced: Mass::from_milligrams(1_000_001),
            }
        )))
    );
}

#[test]
fn equipment_assembly_skips_older_contaminated_stock_when_pure_material_exists() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA55E_0002));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_800_000))
        .unwrap_or_else(|error| panic!("mixed assembly source fixture failed: {error}"));
    let mixed = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_STONE, 900_000),
        CompositionComponent::new(MATERIAL_WOOD, 100_000),
    ])
    .unwrap_or_else(|error| panic!("mixed assembly composition fixture failed: {error}"));
    let contaminated = deposit_composed_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
        Mass::from_milligrams(800_000),
        Temperature::from_millikelvin(293_150),
        mixed,
    )
    .unwrap_or_else(|error| panic!("contaminated assembly stock fixture failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
        Mass::from_milligrams(800_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("pure assembly stone fixture failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
        Mass::from_milligrams(200_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("pure assembly handle fixture failed: {error}"));

    validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_PICK, source)
        .unwrap_or_else(|error| panic!("mixed-stock assembly validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("mixed-stock assembly commit failed: {error}"));

    assert_eq!(
        state
            .inventory()
            .get_lot(contaminated)
            .map(|lot| lot.mass()),
        Some(Mass::from_milligrams(800_000))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::from_milligrams(800_000))
    );
}
