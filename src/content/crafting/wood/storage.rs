//! Timber enclosure-body joinery and loss-bearing salvage definitions.

use crate::content::crafted_parts::{
    BULK_TIMBER_PROVISIONS_CRATE_BODY_MASS, DOUBLE_WALL_TIMBER_PROVISIONS_CHEST_BODY_MASS,
    INSULATED_TIMBER_PANTRY_BODY_MASS, ROUGH_TIMBER_FIELD_BOX_BODY_MASS,
    TIMBER_PROVISIONS_CHEST_BODY_MASS,
};
use crate::content::materials::{
    FORM_BOARD, FORM_BULK_CRATE_BODY, FORM_CHEST_BODY, FORM_CHIP, FORM_DOUBLE_WALL_CHEST_BODY,
    FORM_INSULATED_PANTRY_BODY, FORM_ROUGH_BOX_BODY, MATERIAL_WOOD,
};
use crate::content::processes::{
    PROCESS_ASSEMBLE_BULK_TIMBER_CRATE, PROCESS_ASSEMBLE_DOUBLE_WALL_TIMBER_CHEST,
    PROCESS_ASSEMBLE_INSULATED_TIMBER_PANTRY, PROCESS_ASSEMBLE_ROUGH_TIMBER_FIELD_BOX,
    PROCESS_ASSEMBLE_TIMBER_CHEST, PROCESS_SALVAGE_BULK_TIMBER_CRATE_BODY,
    PROCESS_SALVAGE_DOUBLE_WALL_TIMBER_CHEST_BODY, PROCESS_SALVAGE_INSULATED_TIMBER_PANTRY_BODY,
    PROCESS_SALVAGE_ROUGH_TIMBER_FIELD_BOX_BODY, PROCESS_SALVAGE_TIMBER_CHEST_BODY,
};
use crate::core::quantity::Mass;
use crate::core::time::TickSpan;
use crate::crafting::{ManualCraftDefinition, ManualCraftOutput};
use crate::material::{CommodityKey, FormId};
use crate::production::ProcessId;

use super::wood_exertion;

const STORAGE_BODY_SALVAGE_CHIP_MASS: Mass = Mass::from_milligrams(800_000);

pub(super) fn definitions() -> [ManualCraftDefinition; 10] {
    [
        assemble_storage_body(
            PROCESS_ASSEMBLE_ROUGH_TIMBER_FIELD_BOX,
            FORM_ROUGH_BOX_BODY,
            ROUGH_TIMBER_FIELD_BOX_BODY_MASS,
            TickSpan::new(50),
        ),
        assemble_storage_body(
            PROCESS_ASSEMBLE_TIMBER_CHEST,
            FORM_CHEST_BODY,
            TIMBER_PROVISIONS_CHEST_BODY_MASS,
            TickSpan::new(80),
        ),
        assemble_storage_body(
            PROCESS_ASSEMBLE_DOUBLE_WALL_TIMBER_CHEST,
            FORM_DOUBLE_WALL_CHEST_BODY,
            DOUBLE_WALL_TIMBER_PROVISIONS_CHEST_BODY_MASS,
            TickSpan::new(120),
        ),
        assemble_storage_body(
            PROCESS_ASSEMBLE_BULK_TIMBER_CRATE,
            FORM_BULK_CRATE_BODY,
            BULK_TIMBER_PROVISIONS_CRATE_BODY_MASS,
            TickSpan::new(90),
        ),
        assemble_storage_body(
            PROCESS_ASSEMBLE_INSULATED_TIMBER_PANTRY,
            FORM_INSULATED_PANTRY_BODY,
            INSULATED_TIMBER_PANTRY_BODY_MASS,
            TickSpan::new(140),
        ),
        salvage_storage_body(
            PROCESS_SALVAGE_ROUGH_TIMBER_FIELD_BOX_BODY,
            FORM_ROUGH_BOX_BODY,
            ROUGH_TIMBER_FIELD_BOX_BODY_MASS,
            TickSpan::new(50),
        ),
        salvage_storage_body(
            PROCESS_SALVAGE_TIMBER_CHEST_BODY,
            FORM_CHEST_BODY,
            TIMBER_PROVISIONS_CHEST_BODY_MASS,
            TickSpan::new(70),
        ),
        salvage_storage_body(
            PROCESS_SALVAGE_DOUBLE_WALL_TIMBER_CHEST_BODY,
            FORM_DOUBLE_WALL_CHEST_BODY,
            DOUBLE_WALL_TIMBER_PROVISIONS_CHEST_BODY_MASS,
            TickSpan::new(100),
        ),
        salvage_storage_body(
            PROCESS_SALVAGE_BULK_TIMBER_CRATE_BODY,
            FORM_BULK_CRATE_BODY,
            BULK_TIMBER_PROVISIONS_CRATE_BODY_MASS,
            TickSpan::new(80),
        ),
        salvage_storage_body(
            PROCESS_SALVAGE_INSULATED_TIMBER_PANTRY_BODY,
            FORM_INSULATED_PANTRY_BODY,
            INSULATED_TIMBER_PANTRY_BODY_MASS,
            TickSpan::new(120),
        ),
    ]
}

fn assemble_storage_body(
    process: ProcessId,
    body_form: FormId,
    body_mass: Mass,
    duration: TickSpan,
) -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        process,
        CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
        body_mass,
        duration,
        wood_exertion(),
        vec![ManualCraftOutput::new(
            CommodityKey::new(MATERIAL_WOOD, body_form),
            body_mass,
        )],
    )
}

fn salvage_storage_body(
    process: ProcessId,
    body_form: FormId,
    body_mass: Mass,
    duration: TickSpan,
) -> ManualCraftDefinition {
    let recovered_board_mass = body_mass
        .checked_sub(STORAGE_BODY_SALVAGE_CHIP_MASS)
        .unwrap_or_else(|| panic!("storage body must exceed its fixed salvage chip loss"));
    ManualCraftDefinition::new(
        process,
        CommodityKey::new(MATERIAL_WOOD, body_form),
        body_mass,
        duration,
        wood_exertion(),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                recovered_board_mass,
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
                STORAGE_BODY_SALVAGE_CHIP_MASS,
            ),
        ],
    )
}
