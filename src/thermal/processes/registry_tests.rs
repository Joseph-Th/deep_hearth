//! Tests for thermal registry authoring invariants that must fail before runtime resolution.

use super::*;
use crate::content::{FORM_CRUSHED, FORM_MOLTEN, PROCESS_CAST_PURE_COPPER, build_registries};
use crate::thermal::PhaseChangeForms;

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
            PhaseChangeForms::new(FORM_MOLTEN, FORM_CRUSHED),
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
