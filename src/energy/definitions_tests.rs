//! Tests for the sibling definitions module; isolated so test-only edits do not invalidate production builds.

use super::*;
use crate::content::{FORM_FLYWHEEL, MATERIAL_STONE};
use crate::core::quantity::Mass;
use crate::material::{CommodityKey, MaterialInputSpec};

fn assembly_profile() -> MaterialAssemblyProfile {
    MaterialAssemblyProfile::new(vec![MaterialInputSpec::pure(
        CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
        Mass::from_milligrams(1),
    )])
}

fn basic_definition(id: EnergyStoreDefinitionId) -> EnergyStoreDefinition {
    EnergyStoreDefinition::new(
        id,
        "energy definition fixture",
        EnergyCarrier::Mechanical,
        Energy::from_nanojoules(1),
        Power::from_microwatts(1),
    )
}

#[test]
fn runtime_assembly_classification_follows_the_authored_route() {
    let unavailable = basic_definition(EnergyStoreDefinitionId::new(930_002));
    let available = basic_definition(EnergyStoreDefinitionId::new(930_003))
        .with_assembly_profile(assembly_profile());

    assert!(!unavailable.has_runtime_assembly_route());
    assert!(available.has_runtime_assembly_route());
}

#[test]
fn energy_store_definition_rejects_duplicate_assembly_profiles() {
    let result = std::panic::catch_unwind(|| {
        basic_definition(EnergyStoreDefinitionId::new(930_001))
            .with_assembly_profile(assembly_profile())
            .with_assembly_profile(assembly_profile())
    });

    assert!(result.is_err());
}
