//! Contract tests for energy-store definitions.

use super::*;
use crate::content::{
    FORM_FLYWHEEL, FORM_REINFORCEMENT, MATERIAL_COPPER, MATERIAL_STONE, build_registries,
};
use crate::core::quantity::Mass;
use crate::material::{CommodityKey, MaterialInputSpec};

fn assembly_profile() -> MaterialAssemblyProfile {
    MaterialAssemblyProfile::new(vec![MaterialInputSpec::pure(
        CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
        Mass::from_milligrams(1),
    )])
}

fn copper_additions() -> MaterialAssemblyProfile {
    MaterialAssemblyProfile::new(vec![MaterialInputSpec::pure(
        CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
        Mass::from_milligrams(1),
    )])
}

fn upgraded_assembly_profile() -> MaterialAssemblyProfile {
    MaterialAssemblyProfile::new(vec![
        MaterialInputSpec::pure(
            CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
            Mass::from_milligrams(1),
        ),
        MaterialInputSpec::pure(
            CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
            Mass::from_milligrams(1),
        ),
    ])
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
fn energy_store_definition_rejects_duplicate_upgrade_profiles() {
    let base = EnergyStoreDefinitionId::new(930_006);
    let result = std::panic::catch_unwind(|| {
        basic_definition(EnergyStoreDefinitionId::new(930_007))
            .with_upgrade_profile(EnergyStoreUpgradeProfile::new(base, copper_additions()))
            .with_upgrade_profile(EnergyStoreUpgradeProfile::new(base, copper_additions()))
    });

    assert!(result.is_err());
}

#[test]
fn registry_rejects_cyclic_energy_upgrade_ancestry() {
    let registries = build_registries();
    let first_id = EnergyStoreDefinitionId::new(930_016);
    let second_id = EnergyStoreDefinitionId::new(930_017);
    let first = basic_definition(first_id).with_upgrade_profile(EnergyStoreUpgradeProfile::new(
        second_id,
        copper_additions(),
    ));
    let second = basic_definition(second_id)
        .with_upgrade_profile(EnergyStoreUpgradeProfile::new(first_id, copper_additions()));
    let invalid = EnergyRegistry::new([first, second]);

    let result = std::panic::catch_unwind(|| {
        invalid.validate_references(
            registries.materials(),
            registries.core().physical_tick_duration(),
        )
    });

    assert!(result.is_err());
}

#[test]
fn registry_rejects_energy_upgrade_with_missing_base_definition() {
    let registries = build_registries();
    let target = basic_definition(EnergyStoreDefinitionId::new(930_008))
        .with_assembly_profile(upgraded_assembly_profile())
        .with_upgrade_profile(EnergyStoreUpgradeProfile::new(
            EnergyStoreDefinitionId::new(930_009),
            copper_additions(),
        ));
    let invalid = EnergyRegistry::new([target]);

    let result = std::panic::catch_unwind(|| {
        invalid.validate_references(
            registries.materials(),
            registries.core().physical_tick_duration(),
        )
    });

    assert!(result.is_err());
}

#[test]
fn registry_rejects_energy_upgrade_target_that_is_not_exact_base_plus_additions() {
    let registries = build_registries();
    let base_id = EnergyStoreDefinitionId::new(930_010);
    let target_id = EnergyStoreDefinitionId::new(930_011);
    let base = basic_definition(base_id).with_assembly_profile(assembly_profile());
    let target = basic_definition(target_id)
        .with_assembly_profile(MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
                Mass::from_milligrams(2),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
                Mass::from_milligrams(1),
            ),
        ]))
        .with_upgrade_profile(EnergyStoreUpgradeProfile::new(base_id, copper_additions()));
    let invalid = EnergyRegistry::new([base, target]);

    let result = std::panic::catch_unwind(|| {
        invalid.validate_references(
            registries.materials(),
            registries.core().physical_tick_duration(),
        )
    });

    assert!(result.is_err());
}

#[test]
fn registry_rejects_energy_upgrade_that_changes_carrier_or_reduces_capacity() {
    let registries = build_registries();
    let base_id = EnergyStoreDefinitionId::new(930_012);
    let target_id = EnergyStoreDefinitionId::new(930_013);
    let base = EnergyStoreDefinition::new_with_transfer_limits(
        base_id,
        "upgrade base",
        EnergyCarrier::Mechanical,
        Energy::from_nanojoules(2),
        Power::from_microwatts(1),
        Power::from_microwatts(1),
    )
    .with_assembly_profile(assembly_profile());
    let target = EnergyStoreDefinition::new_with_transfer_limits(
        target_id,
        "invalid upgrade target",
        EnergyCarrier::Electrical,
        Energy::from_nanojoules(1),
        Power::from_microwatts(1),
        Power::from_microwatts(1),
    )
    .with_assembly_profile(upgraded_assembly_profile())
    .with_upgrade_profile(EnergyStoreUpgradeProfile::new(base_id, copper_additions()));
    let invalid = EnergyRegistry::new([base, target]);

    let result = std::panic::catch_unwind(|| {
        invalid.validate_references(
            registries.materials(),
            registries.core().physical_tick_duration(),
        )
    });

    assert!(result.is_err());
}

#[test]
fn registry_rejects_energy_upgrade_that_increases_passive_loss() {
    let registries = build_registries();
    let base_id = EnergyStoreDefinitionId::new(930_014);
    let target_id = EnergyStoreDefinitionId::new(930_015);
    let base = EnergyStoreDefinition::new_with_transfer_limits(
        base_id,
        "loss upgrade base",
        EnergyCarrier::Mechanical,
        Energy::from_nanojoules(10_000),
        Power::from_microwatts(1),
        Power::from_microwatts(1),
    )
    .with_assembly_profile(assembly_profile());
    let target = EnergyStoreDefinition::new_with_transfer_limits(
        target_id,
        "lossy upgrade target",
        EnergyCarrier::Mechanical,
        Energy::from_nanojoules(10_000),
        Power::from_microwatts(1),
        Power::from_microwatts(1),
    )
    .with_passive_dissipation_power(Power::from_microwatts(1))
    .with_assembly_profile(upgraded_assembly_profile())
    .with_upgrade_profile(EnergyStoreUpgradeProfile::new(base_id, copper_additions()));
    let invalid = EnergyRegistry::new([base, target]);

    let result = std::panic::catch_unwind(|| {
        invalid.validate_references(
            registries.materials(),
            registries.core().physical_tick_duration(),
        )
    });

    assert!(result.is_err());
}

#[test]
fn authored_assembly_edge_classification_follows_the_assembly_field() {
    let unavailable = basic_definition(EnergyStoreDefinitionId::new(930_002));
    let available = basic_definition(EnergyStoreDefinitionId::new(930_003))
        .with_assembly_profile(assembly_profile());

    assert!(!unavailable.has_authored_assembly_edge());
    assert!(available.has_authored_assembly_edge());
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
