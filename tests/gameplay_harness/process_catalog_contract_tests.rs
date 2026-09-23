//! Cross-scope authored process-topology contracts that do not require gameplay simulation.

use std::collections::BTreeSet;

use deep_hearth::content::{
    PROCESS_CAST_PURE_COPPER, PROCESS_HEAT_MATERIAL_BATCH, PROCESS_MELT_PURE_COPPER,
    build_registries,
};

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

#[test]
fn ordinary_process_reachability_can_only_stop_at_the_declared_foundry_frontier() {
    let registries = build_registries();
    let catalog = process_catalog_entries(&registries);
    let declared_frontier = BTreeSet::from([
        PROCESS_MELT_PURE_COPPER,
        PROCESS_CAST_PURE_COPPER,
        PROCESS_HEAT_MATERIAL_BATCH,
    ]);

    let equipment_frontier = catalog
        .iter()
        .filter(|entry| {
            !matches!(entry.equipment_role, ProcessEquipmentRole::None)
                && entry.authored_acquisition_provider_count == 0
        })
        .map(|entry| entry.process)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        equipment_frontier, declared_frontier,
        "only the declared industrial foundry frontier may lack a player-acquirable equipment provider"
    );

    let energy_frontier = catalog
        .iter()
        .filter(|entry| {
            !matches!(entry.energy_role, ProcessEnergyRole::None)
                && entry.authored_assembly_energy_store_count == 0
        })
        .map(|entry| entry.process)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        energy_frontier, declared_frontier,
        "only the declared industrial foundry frontier may lack a player-assembleable compatible energy store"
    );
}
