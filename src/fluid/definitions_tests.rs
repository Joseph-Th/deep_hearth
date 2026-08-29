//! Contract tests for fluid-definition identity invariants enforced at registry assembly.

use super::*;
use crate::content::MATERIAL_WATER;

#[test]
fn fluid_registry_rejects_multiple_identities_for_one_material() {
    let result = std::panic::catch_unwind(|| {
        FluidRegistry::new([
            FluidDefinition::new(FluidDefinitionId::new(940_001), "water a", MATERIAL_WATER),
            FluidDefinition::new(FluidDefinitionId::new(940_002), "water b", MATERIAL_WATER),
        ])
    });

    assert!(result.is_err());
}
