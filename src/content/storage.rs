//! Built-in low-tech material storage and preservation infrastructure.

use crate::core::quantity::{Energy, Mass, Temperature, Volume};
use crate::core::time::TickSpan;
use crate::inventory::{
    StockpileStorageProfile, StorageDefinition, StorageDefinitionId, StorageRegistry,
};
use crate::material::{CommodityKey, MaterialAssemblyProfile, MaterialInputSpec};
use crate::survival::SurvivalExertion;

use super::{
    FORM_BULK_CRATE_BODY, FORM_CHEST_BODY, FORM_DOUBLE_WALL_CHEST_BODY, FORM_INSULATED_PANTRY_BODY,
    FORM_ROUGH_BOX_BODY, FORM_STONE_CROCK_BODY, MATERIAL_STONE, MATERIAL_WOOD,
};

pub const STORAGE_TIMBER_PROVISIONS_CHEST: StorageDefinitionId = StorageDefinitionId::new(1);
pub const STORAGE_DOUBLE_WALL_TIMBER_PROVISIONS_CHEST: StorageDefinitionId =
    StorageDefinitionId::new(2);
pub const STORAGE_BULK_TIMBER_PROVISIONS_CRATE: StorageDefinitionId = StorageDefinitionId::new(3);
pub const STORAGE_INSULATED_TIMBER_PANTRY: StorageDefinitionId = StorageDefinitionId::new(4);
pub const STORAGE_ROUGH_TIMBER_FIELD_BOX: StorageDefinitionId = StorageDefinitionId::new(5);
pub const STORAGE_CARVED_STONE_PROVISIONS_CROCK: StorageDefinitionId = StorageDefinitionId::new(6);
const PROVISIONS_STORAGE_MAXIMUM_TEMPERATURE: Temperature = Temperature::from_millikelvin(333_150);
const STORAGE_DISMANTLE_MILLIGRAMS_PER_TICK: u64 = 100_000;

const fn storage_dismantle_exertion() -> SurvivalExertion {
    SurvivalExertion::new(
        Energy::from_nanojoules(1_000_000_000_000),
        Volume::from_microliters(250),
    )
}

const fn dismantle_duration(body_mass: Mass) -> TickSpan {
    let ticks = body_mass
        .milligrams()
        .div_ceil(STORAGE_DISMANTLE_MILLIGRAMS_PER_TICK);
    TickSpan::new(if ticks == 0 { 1 } else { ticks })
}

pub(crate) fn build_storage_registry() -> StorageRegistry {
    let timber_body_mass = Mass::from_milligrams(2_400_000);
    let double_wall_body_mass = Mass::from_milligrams(4_000_000);
    let bulk_crate_body_mass = Mass::from_milligrams(3_200_000);
    let insulated_pantry_body_mass = Mass::from_milligrams(4_800_000);
    let rough_box_body_mass = Mass::from_milligrams(1_600_000);
    let stone_crock_body_mass = Mass::from_milligrams(2_400_000);
    let lidded_preservation = StockpileStorageProfile::with_preservation(
        true,
        false,
        PROVISIONS_STORAGE_MAXIMUM_TEMPERATURE,
        2_000_000,
    )
    .unwrap_or_else(|error| panic!("timber provisions chest storage profile failed: {error}"));
    let double_wall_preservation = StockpileStorageProfile::with_preservation(
        true,
        false,
        PROVISIONS_STORAGE_MAXIMUM_TEMPERATURE,
        3_000_000,
    )
    .unwrap_or_else(|error| {
        panic!("double-wall timber provisions chest storage profile failed: {error}")
    });
    let bulk_crate_preservation = StockpileStorageProfile::with_preservation(
        true,
        false,
        PROVISIONS_STORAGE_MAXIMUM_TEMPERATURE,
        1_500_000,
    )
    .unwrap_or_else(|error| panic!("bulk timber provisions crate storage profile failed: {error}"));
    let insulated_pantry_preservation = StockpileStorageProfile::with_preservation(
        true,
        false,
        PROVISIONS_STORAGE_MAXIMUM_TEMPERATURE,
        4_000_000,
    )
    .unwrap_or_else(|error| panic!("insulated timber pantry storage profile failed: {error}"));
    let rough_box_preservation = StockpileStorageProfile::with_preservation(
        true,
        false,
        PROVISIONS_STORAGE_MAXIMUM_TEMPERATURE,
        1_250_000,
    )
    .unwrap_or_else(|error| panic!("rough timber field box storage profile failed: {error}"));
    let stone_crock_preservation = StockpileStorageProfile::with_preservation(
        true,
        false,
        PROVISIONS_STORAGE_MAXIMUM_TEMPERATURE,
        2_500_000,
    )
    .unwrap_or_else(|error| {
        panic!("carved stone provisions crock storage profile failed: {error}")
    });
    StorageRegistry::new([
        StorageDefinition::new(
            STORAGE_ROUGH_TIMBER_FIELD_BOX,
            "rough timber field box",
            Mass::from_milligrams(10_000_000),
            rough_box_preservation,
            MaterialAssemblyProfile::new(vec![MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_ROUGH_BOX_BODY),
                rough_box_body_mass,
            )]),
            dismantle_duration(rough_box_body_mass),
            storage_dismantle_exertion(),
        ),
        StorageDefinition::new(
            STORAGE_TIMBER_PROVISIONS_CHEST,
            "lidded timber provisions chest",
            Mass::from_milligrams(20_000_000),
            lidded_preservation,
            MaterialAssemblyProfile::new(vec![MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_CHEST_BODY),
                timber_body_mass,
            )]),
            dismantle_duration(timber_body_mass),
            storage_dismantle_exertion(),
        ),
        StorageDefinition::new(
            STORAGE_DOUBLE_WALL_TIMBER_PROVISIONS_CHEST,
            "double-wall timber provisions chest",
            Mass::from_milligrams(20_000_000),
            double_wall_preservation,
            MaterialAssemblyProfile::new(vec![MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_DOUBLE_WALL_CHEST_BODY),
                double_wall_body_mass,
            )]),
            dismantle_duration(double_wall_body_mass),
            storage_dismantle_exertion(),
        ),
        StorageDefinition::new(
            STORAGE_BULK_TIMBER_PROVISIONS_CRATE,
            "slatted timber bulk provisions crate",
            Mass::from_milligrams(50_000_000),
            bulk_crate_preservation,
            MaterialAssemblyProfile::new(vec![MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_BULK_CRATE_BODY),
                bulk_crate_body_mass,
            )]),
            dismantle_duration(bulk_crate_body_mass),
            storage_dismantle_exertion(),
        ),
        StorageDefinition::new(
            STORAGE_INSULATED_TIMBER_PANTRY,
            "compact insulated timber pantry",
            Mass::from_milligrams(8_000_000),
            insulated_pantry_preservation,
            MaterialAssemblyProfile::new(vec![MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_INSULATED_PANTRY_BODY),
                insulated_pantry_body_mass,
            )]),
            dismantle_duration(insulated_pantry_body_mass),
            storage_dismantle_exertion(),
        ),
        StorageDefinition::new(
            STORAGE_CARVED_STONE_PROVISIONS_CROCK,
            "carved stone provisions crock",
            Mass::from_milligrams(6_000_000),
            stone_crock_preservation,
            MaterialAssemblyProfile::new(vec![MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_STONE_CROCK_BODY),
                stone_crock_body_mass,
            )]),
            dismantle_duration(stone_crock_body_mass),
            storage_dismantle_exertion(),
        ),
    ])
}
