//! Contract tests for thermal-registry authoring invariants enforced before runtime resolution.

use super::*;
use crate::content::{
    FORM_CRUSHED, FORM_MOLTEN, MATERIAL_COPPER, PROCESS_CAST_PURE_COPPER, build_registries,
};
use crate::core::quantity::Temperature;
use crate::production::{ProcessDefinition, ProductionRegistry};
use crate::thermal::{CastingPhaseChange, PhaseChangeForms};

#[test]
fn thermal_registry_requires_resolver_capabilities_in_process_requirements() {
    let registries = build_registries();
    let authored = registries
        .thermal()
        .get_casting(PROCESS_CAST_PURE_COPPER)
        .unwrap_or_else(|| panic!("built-in copper casting definition disappeared"));
    let authored_process = registries
        .production()
        .get_process(authored.process())
        .unwrap_or_else(|| panic!("built-in copper casting process disappeared"));
    let requirements = authored_process
        .capability_requirements()
        .iter()
        .copied()
        .filter(|requirement| requirement.capability() != authored.cooling_power_capability())
        .collect();
    let mut production = ProductionRegistry::new();
    production.register_process(ProcessDefinition::new_selected_batch(
        authored.process(),
        "invalid casting capability contract",
        requirements,
    ));
    let registry = ThermalRegistry::new(std::iter::empty(), std::iter::empty(), [authored]);

    let result = std::panic::catch_unwind(|| {
        registry.validate_references(
            &production,
            registries.capabilities(),
            registries.materials(),
        );
    });

    assert!(
        result.is_err(),
        "thermal resolver capabilities must also participate in generic process provider matching"
    );
}

#[test]
fn casting_registry_rejects_particulate_solid_output_without_distribution() {
    let registries = build_registries();
    let authored = registries
        .thermal()
        .get_casting(PROCESS_CAST_PURE_COPPER)
        .unwrap_or_else(|| panic!("built-in copper casting definition disappeared"));
    let registry = ThermalRegistry::new(
        std::iter::empty(),
        std::iter::empty(),
        [CastingProcessDefinition::new(
            authored.process(),
            authored.cooling_power_capability(),
            authored.max_temperature_capability(),
            authored.max_batch_mass_capability(),
            authored.energy_carrier(),
            CastingPhaseChange::new(
                PhaseChangeForms::new(FORM_MOLTEN, FORM_CRUSHED),
                authored.output_temperature(),
            ),
            authored.condition_wear_ppm_per_active_tick(),
        )],
    );

    let result = std::panic::catch_unwind(|| {
        registry.validate_references(
            registries.production(),
            registries.capabilities(),
            registries.materials(),
        );
    });

    assert!(result.is_err());
}

#[test]
fn casting_registry_rejects_output_temperature_above_material_melting_point() {
    let registries = build_registries();
    let authored = registries
        .thermal()
        .get_casting(PROCESS_CAST_PURE_COPPER)
        .unwrap_or_else(|| panic!("built-in copper casting definition disappeared"));
    let melting_point = registries
        .materials()
        .get_material(MATERIAL_COPPER)
        .and_then(|material| material.properties().thermal().melting_point())
        .unwrap_or_else(|| panic!("built-in copper fusion properties disappeared"));
    let invalid_output_temperature =
        Temperature::from_millikelvin(melting_point.millikelvin().checked_add(1).unwrap_or_else(
            || panic!("copper melting point cannot produce an invalid test target"),
        ));
    let registry = ThermalRegistry::new(
        std::iter::empty(),
        std::iter::empty(),
        [CastingProcessDefinition::new(
            authored.process(),
            authored.cooling_power_capability(),
            authored.max_temperature_capability(),
            authored.max_batch_mass_capability(),
            authored.energy_carrier(),
            CastingPhaseChange::new(
                PhaseChangeForms::new(authored.liquid_form(), authored.solid_form()),
                invalid_output_temperature,
            ),
            authored.condition_wear_ppm_per_active_tick(),
        )],
    );

    let result = std::panic::catch_unwind(|| {
        registry.validate_references(
            registries.production(),
            registries.capabilities(),
            registries.materials(),
        );
    });

    assert!(result.is_err());
}
