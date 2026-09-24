//! Portable primitive equipment and additive copper upgrade definitions.

use crate::equipment::{EquipmentDefinition, EquipmentDefinitionId, EquipmentUpgradeProfile};
use crate::material::{CommodityKey, MaterialAssemblyProfile, MaterialInputSpec};

use crate::content::crafted_parts::COPPER_REINFORCEMENT_MASS;
use crate::content::materials::{FORM_REINFORCEMENT, MATERIAL_COPPER};

mod dressing;
mod foundry;
mod metalworking;
mod mining;
mod power;
mod precision;
mod processing;
mod toolroom;
mod woodworking;

pub(super) fn definitions() -> Vec<EquipmentDefinition> {
    vec![
        dressing::stone_cobbing_hammer(),
        dressing::timber_dressing_bench(),
        metalworking::timber_treadle_hammer(),
        metalworking::timber_helve_hammer(),
        mining::stone_pick(),
        power::stone_hand_crank(),
        mining::copper_reinforced_pick(),
        power::copper_reinforced_hand_crank(),
        mining::stone_quarry_pick(),
        mining::copper_reinforced_stone_quarry_pick(),
        mining::stone_geological_hammer(),
        mining::copper_reinforced_geological_hammer(),
        precision::stone_flywheel_pump_drill(),
        precision::timber_spindle_drill(),
        power::timber_treadle_drive(),
        power::timber_walking_wheel_drive(),
        processing::stone_crusher(),
        processing::stone_separator(),
        processing::stone_rotary_quern(),
        processing::timber_riddle_sizing_screen(),
        processing::copper_plate_sizing_screen(),
        processing::copper_reinforced_stone_crusher(),
        processing::copper_reinforced_stone_separator(),
        processing::copper_reinforced_stone_rotary_quern(),
        processing::timber_frame_comminution_mill(),
        processing::timber_ore_dressing_table(),
        woodworking::stone_woodworking_adze(),
        woodworking::copper_reinforced_woodworking_adze(),
        woodworking::timber_frame_saw_bench(),
        woodworking::timber_sash_sawmill(),
        woodworking::timber_spring_pole_lathe(),
        woodworking::timber_flywheel_lathe(),
        toolroom::timber_treadle_grindstone(),
        toolroom::timber_flywheel_grinding_bench(),
        foundry::timber_treadle_dynamo(),
        foundry::stone_arc_crucible_furnace(),
        foundry::stone_ingot_mold(),
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
