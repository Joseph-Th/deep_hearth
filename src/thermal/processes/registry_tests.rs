//! Tests for thermal registry authoring invariants that must fail before runtime resolution.

use super::*;
use crate::content::{
    FORM_CRUSHED, FORM_MOLTEN, MATERIAL_COPPER, PROCESS_CAST_PURE_COPPER, build_registries,
};
use crate::core::quantity::Temperature;
use crate::thermal::{CastingPhaseChange, PhaseChangeForms};

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
