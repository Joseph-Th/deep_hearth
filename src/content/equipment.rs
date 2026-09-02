//! Built-in workshop equipment definitions.

use crate::equipment::{EquipmentDefinitionId, EquipmentRegistry};

mod authoring;
mod industrial;
mod primitive;

pub const EQUIPMENT_JAW_CRUSHER: EquipmentDefinitionId = EquipmentDefinitionId::new(1);
pub const EQUIPMENT_ELECTRIC_FURNACE: EquipmentDefinitionId = EquipmentDefinitionId::new(2);
pub const EQUIPMENT_CASTING_MOLD: EquipmentDefinitionId = EquipmentDefinitionId::new(3);
pub const EQUIPMENT_DRY_SCREEN: EquipmentDefinitionId = EquipmentDefinitionId::new(4);
pub const EQUIPMENT_GRINDING_MILL: EquipmentDefinitionId = EquipmentDefinitionId::new(5);
pub const EQUIPMENT_STONE_PICK: EquipmentDefinitionId = EquipmentDefinitionId::new(6);
pub const EQUIPMENT_STONE_HAND_CRANK: EquipmentDefinitionId = EquipmentDefinitionId::new(7);
pub const EQUIPMENT_COPPER_REINFORCED_PICK: EquipmentDefinitionId = EquipmentDefinitionId::new(8);
pub const EQUIPMENT_COPPER_REINFORCED_HAND_CRANK: EquipmentDefinitionId =
    EquipmentDefinitionId::new(9);
pub const EQUIPMENT_STONE_CRUSHER: EquipmentDefinitionId = EquipmentDefinitionId::new(10);
pub const EQUIPMENT_STONE_SEPARATOR: EquipmentDefinitionId = EquipmentDefinitionId::new(11);
pub const EQUIPMENT_GRAVITY_SEPARATOR: EquipmentDefinitionId = EquipmentDefinitionId::new(12);
pub const EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER: EquipmentDefinitionId =
    EquipmentDefinitionId::new(13);
pub const EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR: EquipmentDefinitionId =
    EquipmentDefinitionId::new(14);
pub const EQUIPMENT_STONE_QUARRY_PICK: EquipmentDefinitionId = EquipmentDefinitionId::new(15);
pub const EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK: EquipmentDefinitionId =
    EquipmentDefinitionId::new(16);
pub const EQUIPMENT_TIMBER_TREADLE_DRIVE: EquipmentDefinitionId = EquipmentDefinitionId::new(17);
pub const EQUIPMENT_STONE_ROTARY_QUERN: EquipmentDefinitionId = EquipmentDefinitionId::new(18);
pub const EQUIPMENT_COPPER_REINFORCED_STONE_ROTARY_QUERN: EquipmentDefinitionId =
    EquipmentDefinitionId::new(19);
pub const EQUIPMENT_COPPER_PLATE_SIZING_SCREEN: EquipmentDefinitionId =
    EquipmentDefinitionId::new(20);
pub const EQUIPMENT_STONE_GEOLOGICAL_HAMMER: EquipmentDefinitionId = EquipmentDefinitionId::new(21);
pub const EQUIPMENT_COPPER_REINFORCED_GEOLOGICAL_HAMMER: EquipmentDefinitionId =
    EquipmentDefinitionId::new(22);
pub const EQUIPMENT_STONE_WOODWORKING_ADZE: EquipmentDefinitionId = EquipmentDefinitionId::new(23);
pub const EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE: EquipmentDefinitionId =
    EquipmentDefinitionId::new(24);
pub const EQUIPMENT_TIMBER_FRAME_SAW_BENCH: EquipmentDefinitionId = EquipmentDefinitionId::new(25);

pub(crate) fn build_equipment_registry() -> EquipmentRegistry {
    EquipmentRegistry::new(
        industrial::definitions()
            .into_iter()
            .chain(primitive::definitions()),
    )
}

#[cfg(test)]
#[path = "equipment_tests.rs"]
mod tests;
