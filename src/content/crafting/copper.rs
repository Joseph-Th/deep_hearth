//! Primitive copper cold-working definitions.

use crate::core::quantity::{Energy, Mass, Volume};
use crate::core::time::TickSpan;
use crate::crafting::{ManualCraftDefinition, ManualCraftEquipmentProfile, ManualCraftOutput};
use crate::material::CommodityKey;
use crate::survival::SurvivalExertion;

use crate::content::capabilities::{
    CAPABILITY_COPPER_HAMMERING_FLOW, CAPABILITY_COPPER_PIERCING_FLOW,
};
use crate::content::crafted_parts::{
    COPPER_REINFORCEMENT_MASS, COPPER_SAW_BLADE_MASS, COPPER_SCREEN_PLATE_MASS,
};
use crate::content::materials::{
    FORM_CHIP, FORM_INGOT, FORM_NATIVE_METAL, FORM_REINFORCEMENT, FORM_SAW_BLADE, FORM_SCRAP,
    FORM_SCREEN_PLATE, MATERIAL_COPPER,
};
use crate::content::processes::{
    PROCESS_COLD_WORK_COPPER_INGOT_REINFORCEMENT, PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
    PROCESS_COLD_WORK_COPPER_SAW_BLADE, PROCESS_COLD_WORK_COPPER_SCRAP_REINFORCEMENT,
    PROCESS_PIERCE_COPPER_SCREEN_PLATE,
};

pub(super) fn definitions() -> [ManualCraftDefinition; 5] {
    [
        cold_work_native_copper(),
        cold_work_cast_copper(),
        cold_work_copper_scrap(),
        pierce_copper_screen_plate(),
        cold_work_copper_saw_blade(),
    ]
}

fn cold_work_cast_copper() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_COLD_WORK_COPPER_INGOT_REINFORCEMENT,
        CommodityKey::new(MATERIAL_COPPER, FORM_INGOT),
        COPPER_REINFORCEMENT_MASS,
        TickSpan::new(45),
        copper_work_exertion(),
        vec![ManualCraftOutput::new(
            CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
            COPPER_REINFORCEMENT_MASS,
        )],
    )
    .with_equipment_profile(treadle_hammer_profile())
}

fn treadle_hammer_profile() -> ManualCraftEquipmentProfile {
    ManualCraftEquipmentProfile::new(CAPABILITY_COPPER_HAMMERING_FLOW, 100)
}

fn copper_work_exertion() -> SurvivalExertion {
    SurvivalExertion::new(
        Energy::from_nanojoules(1_000_000_000_000),
        Volume::from_microliters(250),
    )
}

fn cold_work_copper_saw_blade() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_COLD_WORK_COPPER_SAW_BLADE,
        CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
        Mass::from_milligrams(60_000),
        TickSpan::new(120),
        copper_work_exertion(),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_COPPER, FORM_SAW_BLADE),
                COPPER_SAW_BLADE_MASS,
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP),
                Mass::from_milligrams(6_000),
            ),
        ],
    )
    .with_equipment_profile(treadle_hammer_profile())
}

fn cold_work_native_copper() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
        CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
        COPPER_REINFORCEMENT_MASS,
        TickSpan::new(40),
        copper_work_exertion(),
        vec![ManualCraftOutput::new(
            CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
            COPPER_REINFORCEMENT_MASS,
        )],
    )
    .with_equipment_profile(treadle_hammer_profile())
}

fn pierce_copper_screen_plate() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_PIERCE_COPPER_SCREEN_PLATE,
        CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
        COPPER_REINFORCEMENT_MASS,
        TickSpan::new(50),
        copper_work_exertion(),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_COPPER, FORM_SCREEN_PLATE),
                COPPER_SCREEN_PLATE_MASS,
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP),
                Mass::from_milligrams(2_000),
            ),
        ],
    )
    .with_equipment_profile(ManualCraftEquipmentProfile::new_required(
        CAPABILITY_COPPER_PIERCING_FLOW,
        500,
    ))
}

fn cold_work_copper_scrap() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_COLD_WORK_COPPER_SCRAP_REINFORCEMENT,
        CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP),
        COPPER_REINFORCEMENT_MASS,
        TickSpan::new(50),
        copper_work_exertion(),
        // Cold consolidation can recover the large, workable pieces, but fine offcuts are no
        // longer workable as coarse scrap. Keeping ten percent as copper chips gives remelting a
        // material-recovery purpose without destroying matter, while players who value time over
        // recovery can still take the direct route.
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
                Mass::from_milligrams(18_000),
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_COPPER, FORM_CHIP),
                Mass::from_milligrams(2_000),
            ),
        ],
    )
    .with_equipment_profile(treadle_hammer_profile())
}
