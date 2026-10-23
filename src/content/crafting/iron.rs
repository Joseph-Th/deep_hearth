//! Timed abrasive finishing of reclaimed and cast iron into reusable tool heads.

use crate::core::quantity::{Energy, Mass, Volume};
use crate::core::time::TickSpan;
use crate::crafting::{ManualCraftDefinition, ManualCraftEquipmentProfile, ManualCraftOutput};
use crate::material::{CommodityKey, FormId};
use crate::production::ProcessId;
use crate::survival::SurvivalExertion;

use crate::content::capabilities::CAPABILITY_METAL_ABRASIVE_FINISHING_FLOW;
use crate::content::materials::{FORM_CHIP, FORM_INGOT, FORM_SCRAP, FORM_TOOL, MATERIAL_IRON};
use crate::content::processes::{
    PROCESS_FINISH_IRON_PICK_HEAD_FROM_INGOT, PROCESS_FINISH_IRON_PICK_HEAD_FROM_SCRAP,
};

pub(super) fn definitions() -> [ManualCraftDefinition; 2] {
    [
        finish_head(PROCESS_FINISH_IRON_PICK_HEAD_FROM_SCRAP, FORM_SCRAP, 260),
        finish_head(PROCESS_FINISH_IRON_PICK_HEAD_FROM_INGOT, FORM_INGOT, 180),
    ]
}

fn finish_head(process: ProcessId, input_form: FormId, work_ticks: u64) -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        process,
        CommodityKey::new(MATERIAL_IRON, input_form),
        Mass::from_milligrams(1_000_000),
        TickSpan::new(work_ticks),
        SurvivalExertion::new(
            Energy::from_nanojoules(1_000_000_000_000),
            Volume::from_microliters(300),
        ),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_IRON, FORM_TOOL),
                Mass::from_milligrams(800_000),
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_IRON, FORM_CHIP),
                Mass::from_milligrams(200_000),
            ),
        ],
    )
    .with_equipment_profile(ManualCraftEquipmentProfile::new_required(
        CAPABILITY_METAL_ABRASIVE_FINISHING_FLOW,
        600,
    ))
}
