//! Built-in hand-mining method definitions.

use crate::core::quantity::{Energy, Volume};
use crate::mining::{MiningMethodDefinition, MiningMethodId, MiningRegistry};
use crate::survival::SurvivalExertion;

use super::capabilities::{
    CAPABILITY_MINING_FLOW, CAPABILITY_MINING_MAX_BATCH, CAPABILITY_MINING_MAX_HARDNESS,
};

pub const MINING_METHOD_HAND_PICK: MiningMethodId = MiningMethodId::new(1);

pub(crate) fn build_mining_registry() -> MiningRegistry {
    MiningRegistry::new([MiningMethodDefinition::new(
        MINING_METHOD_HAND_PICK,
        "hand pick mining",
        CAPABILITY_MINING_FLOW,
        CAPABILITY_MINING_MAX_BATCH,
        CAPABILITY_MINING_MAX_HARDNESS,
        250,
        SurvivalExertion::new(
            // Hard sustained hand mining: 1.5 kJ of incremental metabolic work per 3.6 s tick
            // (about 417 W above basal metabolism).
            Energy::from_nanojoules(1_500_000_000_000),
            Volume::from_microliters(750),
        ),
    )])
}
