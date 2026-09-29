//! Built-in low-tech material storage and preservation infrastructure.

use crate::core::quantity::{Energy, Mass, Temperature, Volume};
use crate::core::time::TickSpan;
use crate::inventory::{
    StockpileStorageProfile, StorageDefinition, StorageDefinitionId, StorageRegistry,
};
use crate::material::{CommodityKey, MaterialAssemblyProfile, MaterialInputSpec};
use crate::survival::SurvivalExertion;

use super::crafted_parts::{
    BULK_TIMBER_PROVISIONS_CRATE_BODY_MASS, DOUBLE_WALL_TIMBER_PROVISIONS_CHEST_BODY_MASS,
    INSULATED_TIMBER_PANTRY_BODY_MASS, ROUGH_TIMBER_FIELD_BOX_BODY_MASS,
    STONE_PROVISIONS_CROCK_BODY_MASS, TIMBER_PROVISIONS_CHEST_BODY_MASS,
};
use super::{
    FORM_BULK_CRATE_BODY, FORM_CHEST_BODY, FORM_DOUBLE_WALL_CHEST_BODY, FORM_INSULATED_PANTRY_BODY,
    FORM_PACKED_CLAY_BINDER, FORM_REINFORCEMENT, FORM_ROUGH_BOX_BODY, FORM_STONE_CROCK_BODY,
    MATERIAL_CLAY, MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD,
};

pub const STORAGE_TIMBER_PROVISIONS_CHEST: StorageDefinitionId = StorageDefinitionId::new(1);
pub const STORAGE_DOUBLE_WALL_TIMBER_PROVISIONS_CHEST: StorageDefinitionId =
    StorageDefinitionId::new(2);
pub const STORAGE_BULK_TIMBER_PROVISIONS_CRATE: StorageDefinitionId = StorageDefinitionId::new(3);
pub const STORAGE_INSULATED_TIMBER_PANTRY: StorageDefinitionId = StorageDefinitionId::new(4);
pub const STORAGE_ROUGH_TIMBER_FIELD_BOX: StorageDefinitionId = StorageDefinitionId::new(5);
pub const STORAGE_CARVED_STONE_PROVISIONS_CROCK: StorageDefinitionId = StorageDefinitionId::new(6);
pub const STORAGE_COPPER_BANDED_STONE_PROVISIONS_CROCK: StorageDefinitionId =
    StorageDefinitionId::new(7);
pub const STORAGE_CLAY_DAUBED_TIMBER_PROVISIONS_BIN: StorageDefinitionId =
    StorageDefinitionId::new(8);
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

fn storage_definition(
    id: StorageDefinitionId,
    name: &'static str,
    maximum_stockpile_capacity: Mass,
    storage_profile: StockpileStorageProfile,
    assembly_profile: MaterialAssemblyProfile,
) -> StorageDefinition {
    let duration = dismantle_duration(assembly_profile.input_mass());
    StorageDefinition::new(
        id,
        name,
        maximum_stockpile_capacity,
        storage_profile,
        assembly_profile,
        duration,
        storage_dismantle_exertion(),
    )
}

pub(crate) fn build_storage_registry() -> StorageRegistry {
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
        1_750_000,
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
    let copper_banded_crock_preservation = StockpileStorageProfile::with_preservation(
        true,
        false,
        PROVISIONS_STORAGE_MAXIMUM_TEMPERATURE,
        5_000_000,
    )
    .unwrap_or_else(|error| {
        panic!("copper-banded stone provisions crock storage profile failed: {error}")
    });
    let clay_daubed_bin_preservation = StockpileStorageProfile::with_preservation(
        true,
        false,
        PROVISIONS_STORAGE_MAXIMUM_TEMPERATURE,
        2_500_000,
    )
    .unwrap_or_else(|error| panic!("clay-daubed provisions bin storage profile failed: {error}"));
    StorageRegistry::new([
        storage_definition(
            STORAGE_ROUGH_TIMBER_FIELD_BOX,
            "rough timber field box",
            Mass::from_milligrams(10_000_000),
            rough_box_preservation,
            MaterialAssemblyProfile::new(vec![MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_ROUGH_BOX_BODY),
                ROUGH_TIMBER_FIELD_BOX_BODY_MASS,
            )]),
        ),
        storage_definition(
            STORAGE_TIMBER_PROVISIONS_CHEST,
            "lidded timber provisions chest",
            Mass::from_milligrams(20_000_000),
            lidded_preservation,
            MaterialAssemblyProfile::new(vec![MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_CHEST_BODY),
                TIMBER_PROVISIONS_CHEST_BODY_MASS,
            )]),
        ),
        storage_definition(
            STORAGE_DOUBLE_WALL_TIMBER_PROVISIONS_CHEST,
            "double-wall timber provisions chest",
            Mass::from_milligrams(20_000_000),
            double_wall_preservation,
            MaterialAssemblyProfile::new(vec![MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_DOUBLE_WALL_CHEST_BODY),
                DOUBLE_WALL_TIMBER_PROVISIONS_CHEST_BODY_MASS,
            )]),
        ),
        storage_definition(
            STORAGE_BULK_TIMBER_PROVISIONS_CRATE,
            "slatted timber bulk provisions crate",
            Mass::from_milligrams(50_000_000),
            bulk_crate_preservation,
            MaterialAssemblyProfile::new(vec![MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_BULK_CRATE_BODY),
                BULK_TIMBER_PROVISIONS_CRATE_BODY_MASS,
            )]),
        ),
        // Clay daub seals the gaps of the slatted bulk crate without pretending unfired clay is
        // ceramic. It gives staple-scale stores a middle preservation option: less capacity than
        // the open crate and less shelf-life extension than a double-wall chest, but it converts a
        // locally dug earth material into settlement infrastructure instead of demanding more
        // finished timber or scarce copper.
        storage_definition(
            STORAGE_CLAY_DAUBED_TIMBER_PROVISIONS_BIN,
            "clay-daubed timber provisions bin",
            Mass::from_milligrams(40_000_000),
            clay_daubed_bin_preservation,
            MaterialAssemblyProfile::new(vec![
                MaterialInputSpec::pure(
                    CommodityKey::new(MATERIAL_WOOD, FORM_BULK_CRATE_BODY),
                    BULK_TIMBER_PROVISIONS_CRATE_BODY_MASS,
                ),
                MaterialInputSpec::pure(
                    CommodityKey::new(MATERIAL_CLAY, FORM_PACKED_CLAY_BINDER),
                    Mass::from_milligrams(4_000_000),
                ),
            ]),
        ),
        storage_definition(
            STORAGE_INSULATED_TIMBER_PANTRY,
            "compact insulated timber pantry",
            Mass::from_milligrams(8_000_000),
            insulated_pantry_preservation,
            MaterialAssemblyProfile::new(vec![MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_INSULATED_PANTRY_BODY),
                INSULATED_TIMBER_PANTRY_BODY_MASS,
            )]),
        ),
        storage_definition(
            STORAGE_CARVED_STONE_PROVISIONS_CROCK,
            "carved stone provisions crock",
            Mass::from_milligrams(6_000_000),
            stone_crock_preservation,
            MaterialAssemblyProfile::new(vec![MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_STONE_CROCK_BODY),
                STONE_PROVISIONS_CROCK_BODY_MASS,
            )]),
        ),
        // Copper banding turns the small carved crock into the best long-horizon provisions store,
        // but does not increase its capacity. The insulated timber pantry remains better for larger
        // batches, while the crock asks the player to spend scarce worked copper on perishability.
        storage_definition(
            STORAGE_COPPER_BANDED_STONE_PROVISIONS_CROCK,
            "copper-banded sealed stone provisions crock",
            Mass::from_milligrams(6_000_000),
            copper_banded_crock_preservation,
            MaterialAssemblyProfile::new(vec![
                MaterialInputSpec::pure(
                    CommodityKey::new(MATERIAL_STONE, FORM_STONE_CROCK_BODY),
                    STONE_PROVISIONS_CROCK_BODY_MASS,
                ),
                MaterialInputSpec::pure(
                    CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
                    Mass::from_milligrams(40_000),
                ),
            ]),
        ),
    ])
}

#[cfg(all(
    test,
    any(not(feature = "test-unit-shard"), feature = "test-unit-content")
))]
#[path = "storage_tests.rs"]
mod tests;
