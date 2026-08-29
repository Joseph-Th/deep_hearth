//! Contract tests for energy-store definitions.

use super::*;
use crate::content::{FORM_FLYWHEEL, MATERIAL_STONE, build_registries};
use crate::core::quantity::Mass;
use crate::material::{CommodityKey, MaterialInputSpec};

fn assembly_profile() -> MaterialAssemblyProfile {
    MaterialAssemblyProfile::new(vec![MaterialInputSpec::pure(
        CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
        Mass::from_milligrams(1),
    )])
}

fn basic_definition(id: EnergyStoreDefinitionId) -> EnergyStoreDefinition {
    EnergyStoreDefinition::new_with_transfer_limits(
        id,
        "energy definition fixture",
        EnergyCarrier::Mechanical,
        Energy::from_nanojoules(1),
        Power::ZERO,
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

#[test]
fn energy_store_definition_rejects_duplicate_passive_dissipation() {
    let result = std::panic::catch_unwind(|| {
        basic_definition(EnergyStoreDefinitionId::new(930_004))
            .with_passive_dissipation_power(Power::from_microwatts(1))
            .with_passive_dissipation_power(Power::from_microwatts(1))
    });

    assert!(result.is_err());
}

#[test]
fn registry_rejects_passive_dissipation_that_would_discard_fractional_energy() {
    let registries = build_registries();
    let invalid = EnergyRegistry::new([basic_definition(EnergyStoreDefinitionId::new(930_005))
        .with_passive_dissipation_power(Power::from_picowatts(1))]);

    let result = std::panic::catch_unwind(|| {
        invalid.validate_references(
            registries.materials(),
            registries.core().physical_tick_duration(),
        )
    });

    assert!(result.is_err());
}
