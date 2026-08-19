//! Guards for fixture-only industrial capabilities that are not yet player-acquirable.

use deep_hearth::energy::EnergyStoreDefinitionId;
use deep_hearth::equipment::EquipmentDefinitionId;
use deep_hearth::registry::Registries;

pub(super) fn assert_capability_only_equipment(
    registries: &Registries,
    equipment: EquipmentDefinitionId,
) {
    let definition = registries
        .equipment()
        .get_equipment(equipment)
        .unwrap_or_else(|| panic!("gameplay harness equipment definition disappeared"));
    assert!(
        definition.assembly_profile().is_none() && definition.upgrade_profile().is_none(),
        "capability fixture directly injects equipment {} ({}) even though it now has a runtime acquisition route; update the harness to acquire it through gameplay",
        equipment.value(),
        definition.name(),
    );
}

pub(super) fn assert_capability_only_energy_store(
    registries: &Registries,
    store: EnergyStoreDefinitionId,
) {
    let definition = registries
        .energy()
        .get_store(store)
        .unwrap_or_else(|| panic!("gameplay harness energy-store definition disappeared"));
    assert!(
        definition.assembly_profile().is_none(),
        "capability fixture directly injects energy store {} ({}) even though it now has a runtime assembly route; update the harness to construct it through gameplay",
        store.value(),
        definition.name(),
    );
}
