//! Contract tests for immutable player-labor authoring invariants.

use super::*;
use crate::capability::{
    CapabilityDefinition, CapabilityId, CapabilityRegistry, CapabilityValueKind,
};
use crate::content::{EQUIPMENT_JAW_CRUSHER, EQUIPMENT_STONE_GEOLOGICAL_HAMMER, build_registries};
use crate::core::quantity::{Energy, Pressure, Volume};
use crate::core::time::TickSpan;
use crate::energy::{EnergyCarrier, EnergyRegistry};
use crate::equipment::{EquipmentDefinitionId, EquipmentRegistry};
use crate::geology::GeologicalEvidenceKind;
use crate::survival::SurvivalExertion;

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

#[test]
fn prospecting_definition_rejects_hardness_on_nonphysical_evidence() {
    let definition = ProspectingDefinition::new_with_equipment(
        ProspectingMethodId::new(55_005),
        GeologicalEvidenceKind::MagneticSurvey,
        TickSpan::new(1),
        1,
        1,
        active_exertion(),
        ProspectingEquipmentProfile::new(EquipmentDefinitionId::new(55_005), None, 1),
    );

    let result = std::panic::catch_unwind(|| {
        definition.with_excavation_hardness_resolution(Pressure::from_pascals(1))
    });

    assert!(result.is_err());
}

#[test]
fn manual_power_authoring_rejects_methods_without_a_physical_provider() {
    let power_capability = CapabilityId::new(55_002);
    let method = ManualPowerDefinition::new(
        ManualPowerMethodId::new(55_002),
        power_capability,
        EnergyCarrier::Mechanical,
        200_000,
        1,
        active_exertion(),
    );
    let registry = LaborRegistry::new([method], std::iter::empty());
    let mut capabilities = CapabilityRegistry::new();
    capabilities.register_capability(CapabilityDefinition::new(
        power_capability,
        "test manual power",
        CapabilityValueKind::Power,
    ));
    let equipment = EquipmentRegistry::new(std::iter::empty());

    let result = std::panic::catch_unwind(|| {
        registry.validate_references(
            &capabilities,
            &equipment,
            &EnergyRegistry::new(std::iter::empty()),
        );
    });

    assert!(result.is_err());
}

#[test]
fn manual_power_authoring_rejects_methods_without_a_compatible_energy_sink() {
    let registries = build_registries();
    let labor = registries.labor().clone();
    let no_energy = EnergyRegistry::new(std::iter::empty());

    let result = std::panic::catch_unwind(|| {
        labor.validate_references(
            registries.capabilities(),
            registries.equipment(),
            &no_energy,
        );
    });

    assert!(result.is_err());
}

#[test]
fn prospecting_authoring_rejects_structurally_installed_instruments() {
    let registries = build_registries();
    let prospecting = ProspectingDefinition::new_with_equipment(
        ProspectingMethodId::new(55_003),
        GeologicalEvidenceKind::ExcavationSample,
        TickSpan::new(1),
        1,
        1,
        active_exertion(),
        ProspectingEquipmentProfile::new(EQUIPMENT_JAW_CRUSHER, None, 1),
    );
    let labor = LaborRegistry::new(std::iter::empty(), [prospecting]);

    let result = std::panic::catch_unwind(|| {
        labor.validate_references(
            registries.capabilities(),
            registries.equipment(),
            registries.energy(),
        );
    });

    assert!(result.is_err());
}

#[test]
fn prospecting_authoring_rejects_duration_beyond_pristine_tool_lifetime() {
    let registries = build_registries();
    let prospecting = ProspectingDefinition::new_with_equipment(
        ProspectingMethodId::new(55_004),
        GeologicalEvidenceKind::ExcavationSample,
        TickSpan::new(2),
        1,
        1,
        active_exertion(),
        ProspectingEquipmentProfile::new(EQUIPMENT_STONE_GEOLOGICAL_HAMMER, None, 1_000_000),
    );
    let labor = LaborRegistry::new(std::iter::empty(), [prospecting]);

    let result = std::panic::catch_unwind(|| {
        labor.validate_references(
            registries.capabilities(),
            registries.equipment(),
            registries.energy(),
        );
    });

    assert!(result.is_err());
}
