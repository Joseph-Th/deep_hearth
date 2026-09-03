//! Contract tests for authored storage-enclosure definitions.

use super::*;
use crate::core::quantity::{Energy, Temperature, Volume};
use crate::core::time::TickSpan;
use crate::material::{CommodityKey, FormId, MaterialId, MaterialInputSpec};
use crate::survival::SurvivalExertion;

const TEST_STORAGE: StorageDefinitionId = StorageDefinitionId::new(99);
const TEST_MATERIAL: MaterialId = MaterialId::new(99);
const TEST_FORM: FormId = FormId::new(99);

fn preservation_profile() -> StockpileStorageProfile {
    StockpileStorageProfile::with_preservation(
        true,
        false,
        Temperature::from_millikelvin(333_150),
        2_000_000,
    )
    .unwrap_or_else(|error| panic!("storage definition test profile failed: {error}"))
}

fn active_exertion() -> SurvivalExertion {
    SurvivalExertion::new(Energy::from_nanojoules(1), Volume::ZERO)
}

fn assembly_profile() -> MaterialAssemblyProfile {
    MaterialAssemblyProfile::new(vec![MaterialInputSpec::pure(
        CommodityKey::new(TEST_MATERIAL, TEST_FORM),
        Mass::from_milligrams(1),
    )])
}

#[test]
fn storage_definition_requires_nonempty_name() {
    assert!(
        std::panic::catch_unwind(|| {
            StorageDefinition::new(
                TEST_STORAGE,
                "   ",
                Mass::from_milligrams(1),
                preservation_profile(),
                assembly_profile(),
                TickSpan::new(1),
                active_exertion(),
            )
        })
        .is_err()
    );
}

#[test]
fn storage_definition_requires_active_dismantling_exertion() {
    assert!(
        std::panic::catch_unwind(|| {
            StorageDefinition::new(
                TEST_STORAGE,
                "test storage",
                Mass::from_milligrams(1),
                preservation_profile(),
                assembly_profile(),
                TickSpan::new(1),
                SurvivalExertion::REST,
            )
        })
        .is_err()
    );
}
