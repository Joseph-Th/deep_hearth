//! Manual preparation and reclamation of clay binder for settlement infrastructure.

use crate::content::crafted_parts::{
    CLAY_CERAMIC_CROCK_BODY_MASS, CLAY_CERAMIC_INGOT_MOLD_BODY_MASS,
};
use crate::content::materials::{
    FORM_CHIP, FORM_FIRED_CROCK, FORM_FIRED_INGOT_MOLD, FORM_GREENWARE_CROCK,
    FORM_GREENWARE_INGOT_MOLD, FORM_LUMP, FORM_PACKED_CLAY_BINDER, FORM_SCRAP, MATERIAL_CLAY,
};
use crate::content::processes::{
    PROCESS_CRUSH_BROKEN_CERAMIC_CROCK, PROCESS_CRUSH_CERAMIC_INGOT_MOLD, PROCESS_PACK_CLAY_BINDER,
    PROCESS_RECONDITION_CLAY_BINDER, PROCESS_SHAPE_CLAY_GREENWARE_CROCK,
    PROCESS_SHAPE_CLAY_INGOT_MOLD,
};
use crate::core::quantity::{Energy, Mass, Volume};
use crate::core::time::TickSpan;
use crate::crafting::{ManualCraftDefinition, ManualCraftOutput};
use crate::material::CommodityKey;
use crate::survival::SurvivalExertion;

pub(super) fn definitions() -> [ManualCraftDefinition; 6] {
    [
        pack_clay_binder(),
        recondition_clay_binder(),
        shape_greenware_crock(),
        crush_fired_crock(),
        shape_ingot_mold(),
        crush_fired_ingot_mold(),
    ]
}

/// Shape dry mineral clay into fragile greenware before any heat treatment.
/// Water of hydration and drying shrinkage are not currently represented.
fn shape_greenware_crock() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_SHAPE_CLAY_GREENWARE_CROCK,
        CommodityKey::new(MATERIAL_CLAY, FORM_LUMP),
        Mass::from_milligrams(2_000_000),
        TickSpan::new(70),
        clay_exertion(),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_CLAY, FORM_GREENWARE_CROCK),
                CLAY_CERAMIC_CROCK_BODY_MASS,
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_CLAY, FORM_CHIP),
                Mass::from_milligrams(400_000),
            ),
        ],
    )
}

/// A small ingot-casting cavity shaped from one kilogram of local clay.
/// The 200 g of trimming grit remains in inventory.
fn shape_ingot_mold() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_SHAPE_CLAY_INGOT_MOLD,
        CommodityKey::new(MATERIAL_CLAY, FORM_LUMP),
        Mass::from_milligrams(1_000_000),
        TickSpan::new(60),
        clay_exertion(),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_CLAY, FORM_GREENWARE_INGOT_MOLD),
                CLAY_CERAMIC_INGOT_MOLD_BODY_MASS,
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_CLAY, FORM_CHIP),
                Mass::from_milligrams(200_000),
            ),
        ],
    )
}

fn crush_fired_ingot_mold() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_CRUSH_CERAMIC_INGOT_MOLD,
        CommodityKey::new(MATERIAL_CLAY, FORM_FIRED_INGOT_MOLD),
        CLAY_CERAMIC_INGOT_MOLD_BODY_MASS,
        TickSpan::new(30),
        clay_exertion(),
        vec![ManualCraftOutput::new(
            CommodityKey::new(MATERIAL_CLAY, FORM_CHIP),
            CLAY_CERAMIC_INGOT_MOLD_BODY_MASS,
        )],
    )
}

/// Recover fired ceramic as reusable mineral grit, not unfired raw clay.
fn crush_fired_crock() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_CRUSH_BROKEN_CERAMIC_CROCK,
        CommodityKey::new(MATERIAL_CLAY, FORM_FIRED_CROCK),
        CLAY_CERAMIC_CROCK_BODY_MASS,
        TickSpan::new(40),
        clay_exertion(),
        vec![ManualCraftOutput::new(
            CommodityKey::new(MATERIAL_CLAY, FORM_CHIP),
            CLAY_CERAMIC_CROCK_BODY_MASS,
        )],
    )
}

fn clay_exertion() -> SurvivalExertion {
    SurvivalExertion::new(
        Energy::from_nanojoules(750_000_000_000),
        Volume::from_microliters(200),
    )
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
