//! Contract tests for authored storage-enclosure definitions.

use super::*;
use crate::core::quantity::Temperature;
use crate::material::{CommodityKey, FormId, MaterialId, MaterialInputSpec};

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
            )
        })
        .is_err()
    );
}
