//! Small shared fixtures used by the workshop/report harness and focused probe targets.

use deep_hearth::capability::{CapabilityId, CapabilityValue};
use deep_hearth::content::gameplay_fixture::seed_stockpile;
use deep_hearth::core::quantity::{Mass, Temperature};
use deep_hearth::core::state::AppState;
use deep_hearth::equipment::EquipmentDefinitionId;
use deep_hearth::inventory::{StockpileId, StockpileStorageProfile};
use deep_hearth::registry::Registries;

pub(super) const ROOM_TEMPERATURE: Temperature = Temperature::from_millikelvin(293_150);

pub(super) fn nominal_equipment_mass_capability(
    registries: &Registries,
    equipment: EquipmentDefinitionId,
    capability: CapabilityId,
) -> Mass {
    let definition = registries
        .equipment()
        .get_equipment(equipment)
        .unwrap_or_else(|| panic!("gameplay harness equipment definition disappeared"));
    match definition.capabilities().get_capability(capability) {
        Some(CapabilityValue::Mass(mass)) => mass,
        Some(value) => panic!(
            "gameplay harness expected mass capability {} on equipment {} but found {:?}",
            capability.value(),
            equipment.value(),
            value.kind()
        ),
        None => panic!(
            "gameplay harness equipment {} is missing authored mass capability {}",
            equipment.value(),
            capability.value()
        ),
    }
}

pub(super) fn add_solid_stockpile(state: &mut AppState, capacity: Mass) -> StockpileId {
    seed_stockpile(state, capacity, StockpileStorageProfile::solid_only())
}
