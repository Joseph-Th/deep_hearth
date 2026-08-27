//! Guards for fixture-only industrial capabilities that are not yet player-acquirable.

use deep_hearth::content::gameplay_fixture::{seed_energy_store, seed_equipment};
use deep_hearth::core::quantity::Energy;
use deep_hearth::core::state::AppState;
use deep_hearth::energy::{EnergyStoreDefinitionId, EnergyStoreId};
use deep_hearth::equipment::{EquipmentDefinitionId, EquipmentId};
use deep_hearth::maintenance::Condition;
use deep_hearth::registry::Registries;

fn assert_capability_only_equipment(registries: &Registries, equipment: EquipmentDefinitionId) {
    let definition = registries
        .equipment()
        .get_equipment(equipment)
        .unwrap_or_else(|| panic!("gameplay harness equipment definition disappeared"));
    assert!(
        !definition.has_runtime_acquisition_route(),
        "capability fixture directly injects equipment {} ({}) even though it now has a runtime acquisition route; update the harness to acquire it through gameplay",
        equipment.value(),
        definition.name(),
    );
}

fn assert_capability_only_energy_store(registries: &Registries, store: EnergyStoreDefinitionId) {
    let definition = registries
        .energy()
        .get_store(store)
        .unwrap_or_else(|| panic!("gameplay harness energy-store definition disappeared"));
    assert!(
        !definition.has_runtime_assembly_route(),
        "capability fixture directly injects energy store {} ({}) even though it now has a runtime assembly route; update the harness to construct it through gameplay",
        store.value(),
        definition.name(),
    );
}

pub(super) fn seed_capability_only_equipment(
    registries: &Registries,
    state: &mut AppState,
    equipment: EquipmentDefinitionId,
    condition: Condition,
) -> EquipmentId {
    assert_capability_only_equipment(registries, equipment);
    seed_equipment(registries, state, equipment, condition)
}

pub(super) fn seed_capability_only_energy_store(
    registries: &Registries,
    state: &mut AppState,
    store: EnergyStoreDefinitionId,
    amount: Energy,
) -> EnergyStoreId {
    assert_capability_only_energy_store(registries, store);
    seed_energy_store(registries, state, store, amount)
}
