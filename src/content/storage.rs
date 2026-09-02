//! Built-in low-tech material storage and preservation infrastructure.

use crate::core::quantity::{Energy, Mass, Temperature, Volume};
use crate::core::time::TickSpan;
use crate::inventory::{
    StockpileStorageProfile, StorageDefinition, StorageDefinitionId, StorageRegistry,
};
use crate::material::{CommodityKey, MaterialAssemblyProfile, MaterialInputSpec};
use crate::survival::SurvivalExertion;

use super::{FORM_CHEST_BODY, FORM_DOUBLE_WALL_CHEST_BODY, MATERIAL_WOOD};

pub const STORAGE_TIMBER_PROVISIONS_CHEST: StorageDefinitionId = StorageDefinitionId::new(1);
pub const STORAGE_DOUBLE_WALL_TIMBER_PROVISIONS_CHEST: StorageDefinitionId =
    StorageDefinitionId::new(2);
const TIMBER_PROVISIONS_CHEST_MAXIMUM_TEMPERATURE: Temperature =
    Temperature::from_millikelvin(333_150);
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
    ])
}
