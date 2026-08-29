//! Contract tests for ore-processing registry ownership and routing.

use crate::content::{
    FORM_CONCENTRATE, FORM_CRUSHED, FORM_NATIVE_METAL, FORM_REINFORCEMENT, MATERIAL_COPPER,
    PROCESS_HAND_BREAK_ORE, PROCESS_SCREEN_CRUSHED_ORE, PROCESS_SEPARATE_NATIVE_COPPER,
    build_registries,
};
use crate::core::quantity::{Mass, MassFlow};
use crate::core::time::{PhysicalTickDuration, TickSpan};
use crate::production::{ProcessDefinition, ProductionRegistry};

use super::{
    ConstituentSeparationProcessDefinition, ManualComminutionProcessDefinition,
    ManualOreProcessProfile, MassFlowDurationError, OreProcessingRegistry,
    PoweredOreProcessProfile, ScreeningProcessDefinition, calculate_mass_flow_duration_ceiling,
};

#[test]
fn registry_rejects_manual_comminution_and_screening_process_collision() {
    let registries = build_registries();
    let manual = registries
        .ore_processing()
        .get_manual_comminution(PROCESS_HAND_BREAK_ORE)
        .unwrap_or_else(|| panic!("built-in manual comminution definition disappeared"));
    let screening = registries
        .ore_processing()
        .get_screening(PROCESS_SCREEN_CRUSHED_ORE)
        .unwrap_or_else(|| panic!("built-in screening definition disappeared"));
    let manual = ManualComminutionProcessDefinition::new(
        manual.process(),
        manual.input_form(),
        manual.output_form(),
        manual.output_particle_size_distribution().clone(),
        ManualOreProcessProfile::new(
            manual.processing_rate(),
            manual.max_batch_mass(),
            manual.exertion(),
        ),
    );
    let screening = ScreeningProcessDefinition::new(
        manual.process(),
        screening.input_form(),
        screening.output_form(),
        screening.aperture(),
        PoweredOreProcessProfile::new(
            screening.mass_flow_capability(),
            screening.max_batch_mass_capability(),
            screening.energy_carrier(),
            screening.specific_energy(),
            screening.condition_wear_ppm_per_active_tick(),
        ),
    );

    assert!(
        std::panic::catch_unwind(|| {
            OreProcessingRegistry::new_with_manual_processes(
                std::iter::empty(),
                [screening],
                std::iter::empty(),
                [manual],
                std::iter::empty(),
            )
        })
        .is_err(),
        "one process cannot own both manual comminution and powered screening semantics"
    );
}

#[test]
fn powered_ore_registry_requires_resolver_capabilities_in_process_requirements() {
    let registries = build_registries();
    let authored = registries
        .ore_processing()
        .get_screening(PROCESS_SCREEN_CRUSHED_ORE)
        .unwrap_or_else(|| panic!("built-in screening definition disappeared"));
    let authored_process = registries
        .production()
        .get_process(authored.process())
        .unwrap_or_else(|| panic!("built-in screening process disappeared"));
    let requirements = authored_process
        .capability_requirements()
        .iter()
        .copied()
        .filter(|requirement| requirement.capability() != authored.mass_flow_capability())
        .collect();
    let mut production = ProductionRegistry::new();
    production.register_process(ProcessDefinition::new_selected_batch(
        authored.process(),
        "invalid screening capability contract",
        requirements,
    ));
    let registry = OreProcessingRegistry::new_with_processes(
        std::iter::empty(),
        [authored],
        std::iter::empty(),
    );

    let result = std::panic::catch_unwind(|| {
        registry.validate_references(
            &production,
            registries.capabilities(),
            registries.materials(),
        );
    });

    assert!(
        result.is_err(),
        "powered ore resolver capabilities must also participate in generic process provider matching"
    );
}

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
        [ConstituentSeparationProcessDefinition::new_sorting(
            authored.process(),
            FORM_CRUSHED,
            MATERIAL_COPPER,
            FORM_REINFORCEMENT,
            FORM_CRUSHED,
            authored.target_recovery_ppm(),
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
fn separation_registry_requires_a_usable_non_target_residue_commodity() {
    let registries = build_registries();
    let authored = registries
        .ore_processing()
        .get_constituent_separation(PROCESS_SEPARATE_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("built-in native-copper separation definition disappeared"));
    let registry = OreProcessingRegistry::new_with_processes(
        std::iter::empty(),
        std::iter::empty(),
        [ConstituentSeparationProcessDefinition::new_sorting(
            authored.process(),
            FORM_CRUSHED,
            MATERIAL_COPPER,
            FORM_NATIVE_METAL,
            FORM_CONCENTRATE,
            authored.target_recovery_ppm(),
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

    assert!(
        result.is_err(),
        "registry assembly must reject a separation residue form usable only by the target material"
    );
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
