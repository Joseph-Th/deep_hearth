//! Loss-bearing manual recovery of reusable timber stock from worn wood scrap.

use crate::content::capabilities::CAPABILITY_WOODWORKING_FLOW;
use crate::content::materials::{FORM_BOARD, FORM_CHIP, FORM_HANDLE, FORM_SCRAP, MATERIAL_WOOD};
use crate::content::processes::{
    PROCESS_RECOVER_WOOD_SCRAP_BOARDS, PROCESS_REWORK_WOOD_SCRAP_HANDLE,
};
use crate::core::quantity::Mass;
use crate::core::time::TickSpan;
use crate::crafting::{ManualCraftDefinition, ManualCraftEquipmentProfile, ManualCraftOutput};
use crate::material::CommodityKey;

use super::wood_exertion;

pub(super) fn definitions() -> [ManualCraftDefinition; 2] {
    [rework_wood_scrap_handle(), recover_wood_scrap_boards()]
}

/// Cuts sound short lengths from worn timber components into replacement handle stock. The
/// remainder becomes explicit chips, so maintenance residue can reduce but never erase fresh-
/// timber demand.
fn rework_wood_scrap_handle() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_REWORK_WOOD_SCRAP_HANDLE,
        CommodityKey::new(MATERIAL_WOOD, FORM_SCRAP),
        Mass::from_milligrams(250_000),
        TickSpan::new(30),
        wood_exertion(),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(200_000),
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
                Mass::from_milligrams(50_000),
            ),
        ],
    )
}

/// Selects the longer sound sections of damaged timber for secondary board stock. Recovery is
/// intentionally poorer than shaping a fresh log because cracks and worn joints become chips.
fn recover_wood_scrap_boards() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_RECOVER_WOOD_SCRAP_BOARDS,
        CommodityKey::new(MATERIAL_WOOD, FORM_SCRAP),
        Mass::from_milligrams(1_000_000),
        TickSpan::new(60),
        wood_exertion(),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(600_000),
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
