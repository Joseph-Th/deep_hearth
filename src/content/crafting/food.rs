//! Primitive winter staple preparation with explicit inedible material losses.

use crate::content::materials::{
    FORM_CHIP, FORM_FOOD, FORM_WINTER_PROVISIONS, MATERIAL_WILD_NUTS, MATERIAL_WILD_TUBERS,
};
use crate::content::processes::{PROCESS_CURE_WILD_TUBERS, PROCESS_SORT_WILD_NUTS_FOR_WINTER};
use crate::core::quantity::{Energy, Mass, Volume};
use crate::core::time::TickSpan;
use crate::crafting::{ManualCraftDefinition, ManualCraftOutput};
use crate::material::CommodityKey;
use crate::survival::SurvivalExertion;

pub(super) fn definitions() -> [ManualCraftDefinition; 2] {
    [sort_nuts(), cure_tubers()]
}

fn preparation_exertion() -> SurvivalExertion {
    SurvivalExertion::new(
        Energy::from_nanojoules(800_000_000_000),
        Volume::from_microliters(200),
    )
}

/// Remove cracked and damaged nuts before setting aside shell-on winter stock.
/// Rejected material remains in the world as inedible residuals.
fn sort_nuts() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_SORT_WILD_NUTS_FOR_WINTER,
        CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD),
        Mass::from_milligrams(500_000),
        TickSpan::new(240),
        preparation_exertion(),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_WILD_NUTS, FORM_WINTER_PROVISIONS),
                Mass::from_milligrams(450_000),
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_WILD_NUTS, FORM_CHIP),
                Mass::from_milligrams(50_000),
            ),
        ],
    )
}

/// Select and cure intact roots while cutting away damaged sections.
/// Preparation costs time and discards real inedible matter.
fn cure_tubers() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_CURE_WILD_TUBERS,
        CommodityKey::new(MATERIAL_WILD_TUBERS, FORM_FOOD),
        Mass::from_milligrams(500_000),
        TickSpan::new(900),
        preparation_exertion(),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_WILD_TUBERS, FORM_WINTER_PROVISIONS),
                Mass::from_milligrams(400_000),
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_WILD_TUBERS, FORM_CHIP),
                Mass::from_milligrams(100_000),
            ),
        ],
    )
}
