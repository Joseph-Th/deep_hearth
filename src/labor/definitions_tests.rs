//! Contract tests for immutable player-labor authoring invariants.

use super::*;
use crate::core::quantity::{Energy, Volume};

fn active_exertion() -> SurvivalExertion {
    SurvivalExertion::new(Energy::from_nanojoules(1), Volume::ZERO)
}

#[test]
fn prospecting_definition_rejects_duplicate_hardness_resolution() {
    let definition = ProspectingDefinition::new_with_equipment(
        ProspectingMethodId::new(55_001),
        GeologicalEvidenceKind::ExcavationSample,
        TickSpan::new(1),
        1,
        1,
        active_exertion(),
        ProspectingEquipmentProfile::new(EquipmentDefinitionId::new(55_001), None, 1),
    );
    let resolution = Pressure::from_pascals(1);
    let result = std::panic::catch_unwind(|| {
        definition
            .with_excavation_hardness_resolution(resolution)
            .with_excavation_hardness_resolution(resolution)
    });

    assert!(result.is_err());
}
