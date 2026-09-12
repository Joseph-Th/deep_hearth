//! Contract tests for mining-definition construction invariants.

use super::*;
use crate::capability::{CapabilityDefinition, CapabilityValueKind};
use crate::core::quantity::{Energy, Volume};
use crate::equipment::EquipmentRegistry;
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

#[test]
fn mining_registry_rejects_methods_without_a_usable_equipment_provider() {
    let flow = CapabilityId::new(11);
    let batch = CapabilityId::new(12);
    let hardness = CapabilityId::new(13);
    let method = MiningMethodDefinition::new(
        MiningMethodId::new(2),
        "unreachable mining fixture",
        flow,
        batch,
        hardness,
        1,
        SurvivalExertion::new(Energy::from_nanojoules(1), Volume::ZERO),
    );
    let registry = MiningRegistry::new([method]);
    let mut capabilities = CapabilityRegistry::new();
    for (id, kind) in [
        (flow, CapabilityValueKind::MassFlow),
        (batch, CapabilityValueKind::Mass),
        (hardness, CapabilityValueKind::Pressure),
    ] {
        capabilities.register_capability(CapabilityDefinition::new(id, "mining fixture", kind));
    }
    let equipment = EquipmentRegistry::new(std::iter::empty());

    let result = std::panic::catch_unwind(|| {
        registry.validate_references(&capabilities, &equipment);
    });

    assert!(result.is_err());
}
