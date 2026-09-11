//! Cross-scope authored process-topology contracts that do not require gameplay simulation.

use deep_hearth::content::build_registries;

use deep_hearth::registry::{ProcessEnergyRole, ProcessEquipmentRole};

use super::catalog::process_catalog_entries;

#[test]
fn every_authored_process_has_legible_physical_execution_topology() {
    let registries = build_registries();
    let catalog = process_catalog_entries(&registries);
    assert_eq!(
        catalog.len(),
        registries.production().definitions().count(),
        "gameplay catalog discovery must classify every authored process"
    );

    for entry in catalog {
        match entry.equipment_role {
            ProcessEquipmentRole::None => assert_eq!(
                entry.nominal_provider_count,
                0,
                "equipment-free process {} ({}) cannot expose equipment providers",
                entry.process.value(),
                entry.name
            ),
            ProcessEquipmentRole::Optional | ProcessEquipmentRole::Required => assert!(
                entry.nominal_provider_count > 0,
                "equipment-bearing process {} ({}) has no nominal equipment provider",
                entry.process.value(),
                entry.name
            ),
        }
        match entry.energy_role {
            ProcessEnergyRole::None => assert_eq!(
                entry.compatible_energy_store_count,
                0,
                "energy-free process {} ({}) cannot expose energy stores",
                entry.process.value(),
                entry.name
            ),
            ProcessEnergyRole::Supply(_) | ProcessEnergyRole::Sink(_) => assert!(
                entry.compatible_energy_store_count > 0,
                "energy-bearing process {} ({}) has no compatible energy store",
                entry.process.value(),
                entry.name
            ),
        }
    }
}
