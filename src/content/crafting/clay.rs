//! Manual preparation and reclamation of clay binder for settlement infrastructure.

use crate::content::materials::{
    FORM_CHIP, FORM_LUMP, FORM_PACKED_CLAY_BINDER, FORM_SCRAP, MATERIAL_CLAY,
};
use crate::content::processes::{PROCESS_PACK_CLAY_BINDER, PROCESS_RECONDITION_CLAY_BINDER};
use crate::core::quantity::{Energy, Mass, Volume};
use crate::core::time::TickSpan;
use crate::crafting::{ManualCraftDefinition, ManualCraftOutput};
use crate::material::CommodityKey;
use crate::survival::SurvivalExertion;

pub(super) fn definitions() -> [ManualCraftDefinition; 2] {
    [pack_clay_binder(), recondition_clay_binder()]
}

/// Compacts locally dug clay-rich earth into portable binder cakes.
///
/// The operation changes cohesion and handling state without creating or destroying matter. Batch
/// execution lets the player prepare settlement-scale quantities without turning material prep into
/// repeated single-item interaction.
fn pack_clay_binder() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_PACK_CLAY_BINDER,
        CommodityKey::new(MATERIAL_CLAY, FORM_LUMP),
        Mass::from_milligrams(500_000),
        TickSpan::new(15),
        SurvivalExertion::new(
            Energy::from_nanojoules(750_000_000_000),
            Volume::from_microliters(200),
        ),
        vec![ManualCraftOutput::new(
            CommodityKey::new(MATERIAL_CLAY, FORM_PACKED_CLAY_BINDER),
            Mass::from_milligrams(500_000),
        )],
    )
}

/// Reclaims most of a worn casting-bed facing while retiring the fraction that has become spent.
fn recondition_clay_binder() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_RECONDITION_CLAY_BINDER,
        CommodityKey::new(MATERIAL_CLAY, FORM_SCRAP),
        Mass::from_milligrams(1_000_000),
        TickSpan::new(20),
        SurvivalExertion::new(
            Energy::from_nanojoules(750_000_000_000),
            Volume::from_microliters(200),
        ),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_CLAY, FORM_PACKED_CLAY_BINDER),
                Mass::from_milligrams(900_000),
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_CLAY, FORM_CHIP),
                Mass::from_milligrams(100_000),
            ),
        ],
    )
}
