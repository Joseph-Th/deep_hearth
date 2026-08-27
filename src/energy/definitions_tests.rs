//! Tests for the sibling definitions module; isolated so test-only edits do not invalidate production builds.

use super::*;
use crate::content::{FORM_FLYWHEEL, MATERIAL_STONE};
use crate::core::quantity::Mass;
use crate::material::{CommodityKey, MaterialInputSpec};

fn assembly_profile() -> MaterialAssemblyProfile {
    MaterialAssemblyProfile::new(vec![MaterialInputSpec::new(
        CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
        Mass::from_milligrams(1),
    )])
}

#[test]
fn energy_store_definition_rejects_duplicate_assembly_profiles() {
    let result = std::panic::catch_unwind(|| {
        EnergyStoreDefinition::new(
            EnergyStoreDefinitionId::new(930_001),
            "duplicate assembly fixture",
            EnergyCarrier::Mechanical,
            Energy::from_nanojoules(1),
            Power::from_microwatts(1),
        )
        .with_assembly_profile(assembly_profile())
        .with_assembly_profile(assembly_profile())
    });

    assert!(result.is_err());
}
