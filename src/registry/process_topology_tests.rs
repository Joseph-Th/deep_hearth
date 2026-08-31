//! Exact built-in process-topology coverage for resolver families, providers, and energy roles.

use crate::content::{
    ENERGY_ELECTRICAL_BUFFER, ENERGY_THERMAL_SINK, EQUIPMENT_CASTING_MOLD,
    EQUIPMENT_ELECTRIC_FURNACE, PROCESS_CAST_PURE_COPPER, PROCESS_HAND_SORT_NATIVE_COPPER,
    PROCESS_MELT_PURE_COPPER, build_registries,
};
use crate::energy::EnergyCarrier;

use super::{ProcessEnergyRole, ProcessExecutionFamily};

#[test]
fn every_builtin_process_has_one_derived_execution_topology() {
    let registries = build_registries();
    for process in registries.production().definitions() {
        assert!(
            registries.process_topology(process.id()).is_some(),
            "built-in process {} has no physical execution topology",
            process.id().value()
        );
    }
}

#[test]
fn process_topology_preserves_manual_machine_and_energy_direction_semantics() {
    let registries = build_registries();

    let manual = registries
        .process_topology(PROCESS_HAND_SORT_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("hand-sort process topology disappeared"));
    assert_eq!(
        manual.execution_family(),
        ProcessExecutionFamily::ManualSeparation
    );
    assert_eq!(manual.energy_role(), ProcessEnergyRole::None);
    assert!(manual.nominal_providers().is_empty());
    assert!(manual.compatible_energy_stores().is_empty());

    let melting = registries
        .process_topology(PROCESS_MELT_PURE_COPPER)
        .unwrap_or_else(|| panic!("copper-melting process topology disappeared"));
    assert_eq!(melting.execution_family(), ProcessExecutionFamily::Melting);
    assert_eq!(
        melting.energy_role(),
        ProcessEnergyRole::Supply(EnergyCarrier::Electrical)
    );
    assert_eq!(melting.nominal_providers(), &[EQUIPMENT_ELECTRIC_FURNACE]);
    assert_eq!(
        melting.compatible_energy_stores(),
        &[ENERGY_ELECTRICAL_BUFFER]
    );

    let casting = registries
        .process_topology(PROCESS_CAST_PURE_COPPER)
        .unwrap_or_else(|| panic!("copper-casting process topology disappeared"));
    assert_eq!(casting.execution_family(), ProcessExecutionFamily::Casting);
    assert_eq!(
        casting.energy_role(),
        ProcessEnergyRole::Sink(EnergyCarrier::Thermal)
    );
    assert_eq!(casting.nominal_providers(), &[EQUIPMENT_CASTING_MOLD]);
    assert_eq!(casting.compatible_energy_stores(), &[ENERGY_THERMAL_SINK]);
}
