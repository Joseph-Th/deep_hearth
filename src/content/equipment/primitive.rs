//! Portable primitive equipment and additive copper upgrade definitions.

use crate::equipment::{EquipmentDefinition, EquipmentDefinitionId, EquipmentUpgradeProfile};
use crate::material::{CommodityKey, MaterialAssemblyProfile, MaterialInputSpec};

use crate::content::crafted_parts::COPPER_REINFORCEMENT_MASS;
use crate::content::materials::{FORM_REINFORCEMENT, MATERIAL_COPPER};

mod mining;
mod power;
mod processing;
mod woodworking;

pub(super) fn definitions() -> Vec<EquipmentDefinition> {
    vec![
        mining::stone_pick(),
        power::stone_hand_crank(),
        mining::copper_reinforced_pick(),
        power::copper_reinforced_hand_crank(),
        mining::stone_quarry_pick(),
        mining::copper_reinforced_stone_quarry_pick(),
        mining::stone_geological_hammer(),
        mining::copper_reinforced_geological_hammer(),
        power::timber_treadle_drive(),
        processing::stone_crusher(),
        processing::stone_separator(),
        processing::stone_rotary_quern(),
        processing::timber_riddle_sizing_screen(),
        processing::copper_plate_sizing_screen(),
        processing::copper_reinforced_stone_crusher(),
        processing::copper_reinforced_stone_separator(),
        processing::copper_reinforced_stone_rotary_quern(),
        woodworking::stone_woodworking_adze(),
        woodworking::copper_reinforced_woodworking_adze(),
        woodworking::timber_frame_saw_bench(),
    ]
}

fn copper_reinforcement_input() -> MaterialInputSpec {
    MaterialInputSpec::pure(
        CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
        COPPER_REINFORCEMENT_MASS,
    )
}

fn copper_upgrade(from: EquipmentDefinitionId) -> EquipmentUpgradeProfile {
    EquipmentUpgradeProfile::new(
        from,
        MaterialAssemblyProfile::new(vec![copper_reinforcement_input()]),
    )
}
