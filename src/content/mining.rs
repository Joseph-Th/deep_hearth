//! Built-in hand-mining method definitions.

use crate::core::quantity::{Energy, Volume};
use crate::mining::{MiningMethodDefinition, MiningMethodId, MiningRegistry};
use crate::survival::SurvivalExertion;

use super::capabilities::{
    CAPABILITY_EARTHWORK_FLOW, CAPABILITY_EARTHWORK_MAX_BATCH, CAPABILITY_EARTHWORK_MAX_HARDNESS,
    CAPABILITY_MINING_FLOW, CAPABILITY_MINING_MAX_BATCH, CAPABILITY_MINING_MAX_HARDNESS,
};

pub const MINING_METHOD_HAND_PICK: MiningMethodId = MiningMethodId::new(1);
pub const MINING_METHOD_HAND_DIGGING: MiningMethodId = MiningMethodId::new(2);

pub(crate) fn build_mining_registry() -> MiningRegistry {
    MiningRegistry::new([
        MiningMethodDefinition::new(
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
        ),
        MiningMethodDefinition::new(
            MINING_METHOD_HAND_DIGGING,
            "hand shovel digging",
            CAPABILITY_EARTHWORK_FLOW,
            CAPABILITY_EARTHWORK_MAX_BATCH,
            CAPABILITY_EARTHWORK_MAX_HARDNESS,
            200,
            SurvivalExertion::new(
                Energy::from_nanojoules(1_000_000_000_000),
                Volume::from_microliters(500),
            ),
        ),
    ])
}
