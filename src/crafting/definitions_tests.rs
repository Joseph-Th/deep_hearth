//! Contract tests for immutable manual-crafting authoring invariants.

use super::*;
use crate::core::quantity::{Energy, Volume};
use crate::material::{FormId, MaterialId};

fn definition() -> ManualCraftDefinition {
    let material = MaterialId::new(54_001);
    let input = CommodityKey::new(material, FormId::new(54_001));
    let output = CommodityKey::new(material, FormId::new(54_002));
    ManualCraftDefinition::new(
        ProcessId::new(54_001),
        input,
        Mass::from_milligrams(1),
        TickSpan::new(1),
        SurvivalExertion::new(Energy::from_nanojoules(1), Volume::ZERO),
        vec![ManualCraftOutput::new(output, Mass::from_milligrams(1))],
    )
}

#[test]
fn manual_craft_definition_rejects_duplicate_equipment_profiles() {
    let profile = ManualCraftEquipmentProfile::new(CapabilityId::new(54_001), 1);
    let result = std::panic::catch_unwind(|| {
        definition()
            .with_equipment_profile(profile)
            .with_equipment_profile(profile)
    });

    assert!(result.is_err());
}
