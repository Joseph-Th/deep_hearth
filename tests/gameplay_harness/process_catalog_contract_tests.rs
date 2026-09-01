//! Cross-scope authored process-topology contracts that do not require gameplay simulation.

use deep_hearth::content::build_registries;

use super::catalog::{ProcessResolverKind, process_catalog_entries};

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
        if matches!(
            entry.resolver,
            ProcessResolverKind::ManualCraft
                | ProcessResolverKind::ManualComminution
                | ProcessResolverKind::ManualSeparation
        ) {
            assert_eq!(
                (
                    entry.nominal_provider_count,
                    entry.compatible_energy_store_count
                ),
                (0, 0),
                "manual process {} ({}) must not invent machine providers or energy stores",
                entry.process.value(),
                entry.name
            );
        } else {
            assert!(
                entry.nominal_provider_count > 0,
                "machine process {} ({}) has no nominal equipment provider",
                entry.process.value(),
                entry.name
            );
            assert!(
                entry.compatible_energy_store_count > 0,
                "machine process {} ({}) has no compatible energy store",
                entry.process.value(),
                entry.name
            );
        }
    }
}
