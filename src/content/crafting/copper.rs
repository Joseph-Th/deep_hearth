//! Primitive copper cold-working definitions.

use crate::core::quantity::{Energy, Mass, Volume};
use crate::core::time::TickSpan;
use crate::crafting::{ManualCraftDefinition, ManualCraftOutput};
use crate::material::CommodityKey;
use crate::survival::SurvivalExertion;

use crate::content::materials::{
    FORM_NATIVE_METAL, FORM_REINFORCEMENT, FORM_SCRAP, MATERIAL_COPPER,
};
use crate::content::processes::{
    PROCESS_COLD_WORK_COPPER_REINFORCEMENT, PROCESS_COLD_WORK_COPPER_SCRAP_REINFORCEMENT,
};

const REINFORCEMENT_MASS: Mass = Mass::from_milligrams(20_000);

pub(super) fn definitions() -> [ManualCraftDefinition; 2] {
    [cold_work_native_copper(), cold_work_copper_scrap()]
}

fn copper_work_exertion() -> SurvivalExertion {
    SurvivalExertion::new(
        Energy::from_nanojoules(1_000_000_000_000),
        Volume::from_microliters(250),
    )
}

fn cold_work_native_copper() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
        CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
        REINFORCEMENT_MASS,
        TickSpan::new(40),
        copper_work_exertion(),
        vec![ManualCraftOutput::new(
            CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
            REINFORCEMENT_MASS,
        )],
    )
}

fn cold_work_copper_scrap() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_COLD_WORK_COPPER_SCRAP_REINFORCEMENT,
        CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP),
        REINFORCEMENT_MASS,
        TickSpan::new(50),
        copper_work_exertion(),
        vec![ManualCraftOutput::new(
            CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
            REINFORCEMENT_MASS,
        )],
    )
}
