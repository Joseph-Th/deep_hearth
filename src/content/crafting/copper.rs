//! Primitive copper cold-working definitions.

use crate::core::quantity::{Energy, Mass, Volume};
use crate::core::time::TickSpan;
use crate::crafting::{ManualCraftDefinition, ManualCraftEquipmentProfile, ManualCraftOutput};
use crate::material::CommodityKey;
use crate::survival::SurvivalExertion;

use crate::content::capabilities::{
    CAPABILITY_COPPER_HAMMERING_FLOW, CAPABILITY_COPPER_PIERCING_FLOW,
    CAPABILITY_COPPER_WIRE_DRAWING_FLOW,
};
use crate::content::crafted_parts::{
    COPPER_ELECTRICAL_WINDING_MASS, COPPER_REINFORCEMENT_MASS, COPPER_SAW_BLADE_MASS,
    COPPER_SCREEN_PLATE_MASS,
};
use crate::content::materials::{
    FORM_CHIP, FORM_ELECTRICAL_WINDING, FORM_INGOT, FORM_NATIVE_METAL, FORM_REINFORCEMENT,
    FORM_SAW_BLADE, FORM_SCRAP, FORM_SCREEN_PLATE, MATERIAL_COPPER,
};
use crate::content::processes::{
    PROCESS_COLD_WORK_COPPER_ELECTRICAL_WINDING, PROCESS_COLD_WORK_COPPER_INGOT_REINFORCEMENT,
    PROCESS_COLD_WORK_COPPER_REINFORCEMENT, PROCESS_COLD_WORK_COPPER_SAW_BLADE,
    PROCESS_COLD_WORK_COPPER_SCRAP_REINFORCEMENT, PROCESS_DRAW_COPPER_ELECTRICAL_WINDING,
    PROCESS_PIERCE_COPPER_SCREEN_PLATE,
};

pub(super) fn definitions() -> [ManualCraftDefinition; 7] {
    [
        cold_work_native_copper(),
        cold_work_cast_copper(),
        cold_work_copper_scrap(),
        pierce_copper_screen_plate(),
        cold_work_copper_saw_blade(),
        cold_work_copper_electrical_winding(),
        draw_copper_electrical_winding(),
    ]
}

fn cold_work_cast_copper() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_COLD_WORK_COPPER_INGOT_REINFORCEMENT,
        CommodityKey::new(MATERIAL_COPPER, FORM_INGOT),
        COPPER_REINFORCEMENT_MASS,
        // Uniform cast stock is already consolidated and sized for working. Forming it should be
        // distinctly easier than consolidating irregular native metal or scrap; otherwise the
        // foundry pays energy and setup cost only to increase the player's active-work burden.
        TickSpan::new(15),
        copper_work_exertion(),
        vec![ManualCraftOutput::new(
            CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
            COPPER_REINFORCEMENT_MASS,
        )],
    )
    .with_equipment_profile(treadle_hammer_profile())
}

/// Draws prepared copper stock through a reusable stone die instead of trimming a hammered bundle.
///
/// This is a specialist settlement route, not a replacement for the first electrical craft. The
/// general hammer route remains available before the drawbench exists and can later run unattended
/// on stored mechanical work. The manual drawbench initially pays player attention to preserve all
/// 60 g of worked copper as useful conductor; its later flywheel upgrade delegates that same
/// lossless transform rather than introducing a new yield rule.
fn draw_copper_electrical_winding() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_DRAW_COPPER_ELECTRICAL_WINDING,
        CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
        Mass::from_milligrams(60_000),
        TickSpan::new(90),
        copper_work_exertion(),
        vec![ManualCraftOutput::new(
            CommodityKey::new(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING),
            Mass::from_milligrams(60_000),
        )],
    )
    .with_equipment_profile(ManualCraftEquipmentProfile::new_required(
        CAPABILITY_COPPER_WIRE_DRAWING_FLOW,
        120,
    ))
}

/// Draws and folds broad reinforcement stock into a compact conductor winding bundle.
///
/// This stage is intentionally introduced only when the player wants higher-current electrical
/// infrastructure. The first dynamo still accepts ordinary reinforcement, so early copper does not
/// acquire another mandatory microcrafting step. Later settlement machinery can delegate the same
/// learned transform to the helve hammer.
fn cold_work_copper_electrical_winding() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_COLD_WORK_COPPER_ELECTRICAL_WINDING,
        CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
        Mass::from_milligrams(60_000),
        TickSpan::new(90),
        copper_work_exertion(),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING),
                COPPER_ELECTRICAL_WINDING_MASS,
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP),
                Mass::from_milligrams(6_000),
            ),
        ],
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
