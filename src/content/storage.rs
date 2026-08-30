//! Built-in low-tech material storage and preservation infrastructure.

use crate::core::quantity::{Mass, Temperature};
use crate::inventory::{
    StockpileStorageProfile, StorageDefinition, StorageDefinitionId, StorageRegistry,
};
use crate::material::{CommodityKey, MaterialAssemblyProfile, MaterialInputSpec};

use super::{FORM_CHEST_BODY, FORM_DOUBLE_WALL_CHEST_BODY, MATERIAL_WOOD};

pub const STORAGE_TIMBER_PROVISIONS_CHEST: StorageDefinitionId = StorageDefinitionId::new(1);
pub const STORAGE_DOUBLE_WALL_TIMBER_PROVISIONS_CHEST: StorageDefinitionId =
    StorageDefinitionId::new(2);
const TIMBER_PROVISIONS_CHEST_MAXIMUM_TEMPERATURE: Temperature =
    Temperature::from_millikelvin(333_150);

pub(crate) fn build_storage_registry() -> StorageRegistry {
    let lidded_preservation = StockpileStorageProfile::with_preservation(
        true,
        false,
        TIMBER_PROVISIONS_CHEST_MAXIMUM_TEMPERATURE,
        2_000_000,
    )
    .unwrap_or_else(|error| panic!("timber provisions chest storage profile failed: {error}"));
    let double_wall_preservation = StockpileStorageProfile::with_preservation(
        true,
        false,
        TIMBER_PROVISIONS_CHEST_MAXIMUM_TEMPERATURE,
        3_000_000,
    )
    .unwrap_or_else(|error| {
        panic!("double-wall timber provisions chest storage profile failed: {error}")
    });
    StorageRegistry::new([
        StorageDefinition::new(
            STORAGE_TIMBER_PROVISIONS_CHEST,
            "lidded timber provisions chest",
            Mass::from_milligrams(20_000_000),
            lidded_preservation,
            MaterialAssemblyProfile::new(vec![MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_CHEST_BODY),
                Mass::from_milligrams(2_400_000),
            )]),
        ),
        StorageDefinition::new(
            STORAGE_DOUBLE_WALL_TIMBER_PROVISIONS_CHEST,
            "double-wall timber provisions chest",
            Mass::from_milligrams(20_000_000),
            double_wall_preservation,
            MaterialAssemblyProfile::new(vec![MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_DOUBLE_WALL_CHEST_BODY),
                Mass::from_milligrams(4_000_000),
            )]),
        ),
    ])
}
