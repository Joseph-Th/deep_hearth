//! Stone hand-processing definitions.

use crate::core::quantity::{Energy, Mass, Volume};
use crate::core::time::TickSpan;
use crate::crafting::{ManualCraftDefinition, ManualCraftEquipmentProfile, ManualCraftOutput};
use crate::material::CommodityKey;
use crate::survival::SurvivalExertion;

use crate::content::capabilities::CAPABILITY_STONE_GRINDING_FLOW;
use crate::content::crafted_parts::{
    STONE_DRAWPLATE_MASS, STONE_DRILL_BIT_MASS, STONE_FLYWHEEL_MASS, STONE_GRINDSTONE_WHEEL_MASS,
    STONE_PROVISIONS_CROCK_BODY_MASS,
};
use crate::content::materials::{
    FORM_CHIP, FORM_DRAWPLATE, FORM_DRILL_BIT, FORM_FLYWHEEL, FORM_GRINDSTONE_WHEEL, FORM_LUMP,
    FORM_SCRAP, FORM_STONE_CROCK_BODY, FORM_TOOL, MATERIAL_STONE,
};
use crate::content::processes::{
    PROCESS_DRESS_STONE_CHIP_DRILL_BIT, PROCESS_GRIND_STONE_SCRAP_DRILL_BIT,
    PROCESS_GRIND_STONE_SCRAP_TOOL, PROCESS_KNAP_STONE_DRILL_BIT, PROCESS_KNAP_STONE_TOOL,
    PROCESS_REKNAP_STONE_SCRAP_TOOL, PROCESS_SALVAGE_STONE_PROVISIONS_CROCK_BODY,
    PROCESS_SHAPE_STONE_DRAWPLATE, PROCESS_SHAPE_STONE_FLYWHEEL,
    PROCESS_SHAPE_STONE_GRINDSTONE_WHEEL, PROCESS_SHAPE_STONE_PROVISIONS_CROCK,
};

pub(super) fn definitions() -> [ManualCraftDefinition; 11] {
    [
        knap_stone_tool(),
        knap_stone_drill_bit(),
        dress_stone_chip_drill_bit(),
        reknap_stone_scrap_tool(),
        grind_stone_scrap_tool(),
        grind_stone_scrap_drill_bit(),
        shape_stone_flywheel(),
        shape_stone_grindstone_wheel(),
        shape_stone_drawplate(),
        shape_stone_provisions_crock(),
        salvage_stone_provisions_crock_body(),
    ]
}

fn grinding_profile() -> ManualCraftEquipmentProfile {
    ManualCraftEquipmentProfile::new_required(CAPABILITY_STONE_GRINDING_FLOW, 1_000)
}

/// Polishes and perforates a hard stone plate for repeated cold drawing of copper strip and rod.
///
/// The die is deliberately a durable service part rather than a consumed recipe catalyst. Building
/// it asks for real stone-shaping attention up front, while the drawbench later repays that cost by
/// eliminating the trimming loss of the general-purpose hammer route.
fn shape_stone_drawplate() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_SHAPE_STONE_DRAWPLATE,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(500_000),
        TickSpan::new(70),
        stone_exertion(),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_STONE, FORM_DRAWPLATE),
                STONE_DRAWPLATE_MASS,
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
                Mass::from_milligrams(100_000),
            ),
        ],
    )
}

/// Abrasively reworks worn service stone with less mass loss than percussion reknapping.
fn grind_stone_scrap_tool() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_GRIND_STONE_SCRAP_TOOL,
        CommodityKey::new(MATERIAL_STONE, FORM_SCRAP),
        Mass::from_milligrams(900_000),
        TickSpan::new(60),
        stone_exertion(),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
                Mass::from_milligrams(800_000),
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
                Mass::from_milligrams(100_000),
            ),
        ],
    )
    .with_equipment_profile(grinding_profile())
}

/// Recovers precision drill-bit stock from accumulated stone-service fragments.
fn grind_stone_scrap_drill_bit() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_GRIND_STONE_SCRAP_DRILL_BIT,
        CommodityKey::new(MATERIAL_STONE, FORM_SCRAP),
        Mass::from_milligrams(120_000),
        TickSpan::new(20),
        stone_exertion(),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_STONE, FORM_DRILL_BIT),
                STONE_DRILL_BIT_MASS,
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
                Mass::from_milligrams(20_000),
            ),
        ],
    )
    .with_equipment_profile(grinding_profile())
}

fn shape_stone_grindstone_wheel() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_SHAPE_STONE_GRINDSTONE_WHEEL,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(1_600_000),
        TickSpan::new(100),
        stone_exertion(),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_STONE, FORM_GRINDSTONE_WHEEL),
                STONE_GRINDSTONE_WHEEL_MASS,
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
                Mass::from_milligrams(200_000),
            ),
        ],
    )
}

fn dress_stone_chip_drill_bit() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_DRESS_STONE_CHIP_DRILL_BIT,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        STONE_DRILL_BIT_MASS,
        TickSpan::new(12),
        stone_exertion(),
        vec![ManualCraftOutput::new(
            CommodityKey::new(MATERIAL_STONE, FORM_DRILL_BIT),
            STONE_DRILL_BIT_MASS,
        )],
    )
}

fn knap_stone_drill_bit() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_KNAP_STONE_DRILL_BIT,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(200_000),
        TickSpan::new(20),
        stone_exertion(),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_STONE, FORM_DRILL_BIT),
                STONE_DRILL_BIT_MASS,
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
                Mass::from_milligrams(100_000),
            ),
        ],
    )
}

fn shape_stone_provisions_crock() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_SHAPE_STONE_PROVISIONS_CROCK,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(3_000_000),
        TickSpan::new(180),
        stone_exertion(),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_STONE, FORM_STONE_CROCK_BODY),
                STONE_PROVISIONS_CROCK_BODY_MASS,
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
                Mass::from_milligrams(600_000),
            ),
        ],
    )
}

fn salvage_stone_provisions_crock_body() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_SALVAGE_STONE_PROVISIONS_CROCK_BODY,
        CommodityKey::new(MATERIAL_STONE, FORM_STONE_CROCK_BODY),
        STONE_PROVISIONS_CROCK_BODY_MASS,
        TickSpan::new(70),
        stone_exertion(),
        vec![ManualCraftOutput::new(
            CommodityKey::new(MATERIAL_STONE, FORM_SCRAP),
            STONE_PROVISIONS_CROCK_BODY_MASS,
        )],
    )
}

fn stone_exertion() -> SurvivalExertion {
    SurvivalExertion::new(
        Energy::from_nanojoules(1_000_000_000_000),
        Volume::from_microliters(250),
    )
}

fn knap_stone_tool() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_KNAP_STONE_TOOL,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(1_000_000),
        TickSpan::new(40),
        stone_exertion(),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
                Mass::from_milligrams(800_000),
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
                Mass::from_milligrams(200_000),
            ),
        ],
    )
}

fn reknap_stone_scrap_tool() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_REKNAP_STONE_SCRAP_TOOL,
        CommodityKey::new(MATERIAL_STONE, FORM_SCRAP),
        Mass::from_milligrams(1_000_000),
        TickSpan::new(60),
        stone_exertion(),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
                Mass::from_milligrams(800_000),
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
                Mass::from_milligrams(200_000),
            ),
        ],
    )
}

fn shape_stone_flywheel() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_SHAPE_STONE_FLYWHEEL,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(1_000_000),
        TickSpan::new(60),
        stone_exertion(),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
                STONE_FLYWHEEL_MASS,
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
                Mass::from_milligrams(100_000),
            ),
        ],
    )
}
