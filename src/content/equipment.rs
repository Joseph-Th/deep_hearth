//! Built-in equipment definitions; registrations remain empty until concrete physical providers are authored.

use crate::equipment::{EquipmentDefinition, EquipmentRegistry};

pub(crate) fn build_equipment_registry() -> EquipmentRegistry {
    EquipmentRegistry::new(std::iter::empty::<EquipmentDefinition>())
}
