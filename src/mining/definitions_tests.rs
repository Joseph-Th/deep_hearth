//! Contract tests for mining-definition construction invariants.

use super::*;
use crate::survival::SurvivalExertion;

#[test]
fn mining_method_definition_rejects_zero_exertion() {
    let result = std::panic::catch_unwind(|| {
        MiningMethodDefinition::new(
            MiningMethodId::new(1),
            "free mining fixture",
            CapabilityId::new(1),
            CapabilityId::new(2),
            CapabilityId::new(3),
            1,
            SurvivalExertion::REST,
        )
    });

    assert!(result.is_err());
}
