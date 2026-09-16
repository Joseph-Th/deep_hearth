//! Timber shaping plus aggregation of wood-specific manual-craft families.

use crate::core::quantity::{Energy, Mass, Volume};
use crate::core::time::TickSpan;
use crate::crafting::{ManualCraftDefinition, ManualCraftEquipmentProfile, ManualCraftOutput};
use crate::material::CommodityKey;
use crate::survival::SurvivalExertion;

use crate::content::capabilities::{CAPABILITY_SAWING_FLOW, CAPABILITY_WOODWORKING_FLOW};
use crate::content::crafted_parts::{TIMBER_FLYWHEEL_MASS, TIMBER_RIDDLE_PANEL_MASS};
use crate::content::materials::{
    FORM_BOARD, FORM_CHIP, FORM_FLYWHEEL, FORM_HANDLE, FORM_LOG, FORM_TIMBER_RIDDLE_PANEL,
    MATERIAL_WOOD,
};
use crate::content::processes::{
    PROCESS_SAW_WOOD_BOARDS, PROCESS_SHAPE_TIMBER_FLYWHEEL, PROCESS_SHAPE_TIMBER_RIDDLE_PANEL,
    PROCESS_SHAPE_WOOD_BOARDS, PROCESS_SHAPE_WOOD_HANDLE,
};

mod recovery;
mod storage;

pub(super) fn definitions() -> Vec<ManualCraftDefinition> {
    storage::definitions()
        .into_iter()
        .chain([
            shape_wood_boards(),
            saw_wood_boards(),
            shape_wood_handle(),
            shape_timber_flywheel(),
            shape_timber_riddle_panel(),
        ])
        .chain(recovery::definitions())
        .collect()
}

/// Hews a broad, single-piece timber wheel whose lighter rim trades material bulk for lower
/// rotational energy density. Hand shaping remains possible; a woodworking tool accelerates it.
fn shape_timber_flywheel() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_SHAPE_TIMBER_FLYWHEEL,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(2_400_000),
        TickSpan::new(120),
        wood_exertion(),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_FLYWHEEL),
                TIMBER_FLYWHEEL_MASS,
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
                Mass::from_milligrams(400_000),
            ),
        ],
    )
    .with_equipment_profile(ManualCraftEquipmentProfile::new(
        CAPABILITY_WOODWORKING_FLOW,
        1_000,
    ))
}

/// Shapes closely spaced timber slats into a replaceable coarse sizing panel. The chip stream is
/// the material removed to establish a repeatable aperture instead of treating precision as free.
fn shape_timber_riddle_panel() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_SHAPE_TIMBER_RIDDLE_PANEL,
        CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
        Mass::from_milligrams(1_600_000),
        TickSpan::new(90),
        wood_exertion(),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_TIMBER_RIDDLE_PANEL),
                TIMBER_RIDDLE_PANEL_MASS,
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
                Mass::from_milligrams(200_000),
            ),
        ],
    )
    .with_equipment_profile(ManualCraftEquipmentProfile::new_required(
        CAPABILITY_WOODWORKING_FLOW,
        1_000,
    ))
}

fn saw_wood_boards() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_SAW_WOOD_BOARDS,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(1_000_000),
        TickSpan::new(50),
        wood_exertion(),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(900_000),
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
                Mass::from_milligrams(100_000),
            ),
        ],
    )
    .with_equipment_profile(ManualCraftEquipmentProfile::new_required(
        CAPABILITY_SAWING_FLOW,
        1_500,
    ))
}

fn wood_exertion() -> SurvivalExertion {
    SurvivalExertion::new(
        Energy::from_nanojoules(750_000_000_000),
        Volume::from_microliters(200),
    )
}

fn shape_wood_boards() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_SHAPE_WOOD_BOARDS,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(1_000_000),
        TickSpan::new(50),
        wood_exertion(),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(800_000),
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
                Mass::from_milligrams(200_000),
            ),
        ],
    )
    .with_equipment_profile(ManualCraftEquipmentProfile::new(
        CAPABILITY_WOODWORKING_FLOW,
        1_000,
    ))
}

fn shape_wood_handle() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_SHAPE_WOOD_HANDLE,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(1_000_000),
        TickSpan::new(40),
        wood_exertion(),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(200_000),
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
                Mass::from_milligrams(800_000),
            ),
        ],
    )
    .with_equipment_profile(ManualCraftEquipmentProfile::new(
        CAPABILITY_WOODWORKING_FLOW,
        1_000,
    ))
}
