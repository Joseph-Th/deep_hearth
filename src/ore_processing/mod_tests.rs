//! Tests for the sibling mod module; isolated so test-only edits do not invalidate production builds.

use crate::content::{
    FORM_CRUSHED, FORM_REINFORCEMENT, MATERIAL_COPPER, MATERIAL_STONE,
    PROCESS_SEPARATE_NATIVE_COPPER, build_registries,
};
use crate::core::quantity::{Mass, MassFlow};
use crate::core::time::{PhysicalTickDuration, TickSpan};

use super::{
    ConstituentSeparationProcessDefinition, MassFlowDurationError, OreProcessingRegistry,
    PoweredOreProcessProfile, calculate_mass_flow_duration_ceiling,
};

#[test]
fn separation_registry_rejects_free_consolidation_of_particulate_feed() {
    let registries = build_registries();
    let authored = registries
        .ore_processing()
        .get_constituent_separation(PROCESS_SEPARATE_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("built-in native-copper separation definition disappeared"));
    let registry = OreProcessingRegistry::new_with_processes(
        std::iter::empty(),
        std::iter::empty(),
        [ConstituentSeparationProcessDefinition::new_binary(
            authored.process(),
            FORM_CRUSHED,
            MATERIAL_COPPER,
            FORM_REINFORCEMENT,
            MATERIAL_STONE,
            FORM_CRUSHED,
            PoweredOreProcessProfile::new(
                authored.mass_flow_capability(),
                authored.max_batch_mass_capability(),
                authored.energy_carrier(),
                authored.specific_energy(),
                authored.condition_wear_ppm_per_active_tick(),
            ),
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
fn mass_flow_duration_returns_first_tick_that_can_finish_batch() {
    let tick_duration = PhysicalTickDuration::from_microseconds(50_000);
    assert_eq!(
        calculate_mass_flow_duration_ceiling(
            MassFlow::from_milligrams_per_second(30),
            Mass::from_milligrams(3),
            tick_duration,
        ),
        Ok(TickSpan::new(2))
    );
    assert_eq!(
        calculate_mass_flow_duration_ceiling(
            MassFlow::from_milligrams_per_second(60),
            Mass::from_milligrams(3),
            tick_duration,
        ),
        Ok(TickSpan::new(1))
    );
}

#[test]
fn mass_flow_duration_rejects_zero_rate_and_preserves_zero_mass() {
    let tick_duration = PhysicalTickDuration::from_microseconds(50_000);
    assert_eq!(
        calculate_mass_flow_duration_ceiling(
            MassFlow::ZERO,
            Mass::from_milligrams(1),
            tick_duration,
        ),
        Err(MassFlowDurationError::ZeroRate)
    );
    assert_eq!(
        calculate_mass_flow_duration_ceiling(
            MassFlow::from_milligrams_per_second(1),
            Mass::ZERO,
            tick_duration,
        ),
        Ok(TickSpan::ZERO)
    );
}
