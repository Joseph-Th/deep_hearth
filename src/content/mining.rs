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
            // Hand mining is sustained high-exertion labor; the method owns its incremental
            // physiological cost rather than hiding it in extraction throughput.
            Energy::from_nanojoules(1_500_000_000_000),
            Volume::from_microliters(750),
        ),
    )])
}
