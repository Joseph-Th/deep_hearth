//! Built-in low-tech material storage and preservation infrastructure.

use crate::core::quantity::{Mass, Temperature};
use crate::inventory::{
    StockpileStorageProfile, StorageDefinition, StorageDefinitionId, StorageRegistry,
};
use crate::material::{CommodityKey, MaterialAssemblyProfile, MaterialInputSpec};

use super::{FORM_CHEST_BODY, MATERIAL_WOOD};

pub const STORAGE_TIMBER_PROVISIONS_CHEST: StorageDefinitionId = StorageDefinitionId::new(1);

pub(crate) fn build_storage_registry() -> StorageRegistry {
    let preservation = StockpileStorageProfile::with_preservation(
        true,
        false,
        Temperature::from_millikelvin(u32::MAX),
        2_000_000,
    )
    .unwrap_or_else(|error| panic!("timber provisions chest storage profile failed: {error}"));
    StorageRegistry::new([StorageDefinition::new(
        STORAGE_TIMBER_PROVISIONS_CHEST,
        "lidded timber provisions chest",
        Mass::from_milligrams(20_000_000),
        preservation,
        MaterialAssemblyProfile::new(vec![MaterialInputSpec::pure(
            CommodityKey::new(MATERIAL_WOOD, FORM_CHEST_BODY),
            Mass::from_milligrams(2_400_000),
        )]),
    )])
}
