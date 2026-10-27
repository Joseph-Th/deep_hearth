//! Contract tests for ore-processing registry ownership and routing.

use crate::content::{
    FORM_CRUSHED, FORM_NATIVE_METAL, FORM_ORE, FORM_REINFORCEMENT, MATERIAL_COPPER,
    PROCESS_HAND_BREAK_ORE, PROCESS_HAND_SORT_NATIVE_COPPER, PROCESS_SCREEN_CRUSHED_ORE,
    PROCESS_SEPARATE_NATIVE_COPPER, build_registries,
};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::AppState;
use crate::inventory::{
    MaterialStackProjectionError, add_solid_stockpile_for_test, deposit_lot_for_test,
    project_material_stacks,
};
use crate::material::CommodityKey;
use crate::production::{ProcessDefinition, ProductionRegistry};

use super::{
    ConstituentRecoveryProfile, ConstituentSeparationProcessDefinition,
    ManualComminutionProcessDefinition, ManualComminutionResolutionError,
    ManualConstituentSeparationResolutionError, ManualOrePhysicsError, ManualOreProcessProfile,
    OreProcessingRegistry, PoweredOreProcessProfile, ScreeningProcessDefinition,
    project_manual_ore_duration, resolve_manual_comminution_from_stack,
    resolve_manual_constituent_separation_from_stack,
};

#[test]
fn player_selected_manual_ore_stacks_enforce_exact_amounts_and_revision_freshness() {
    let registries = build_registries();
    let mut state = AppState::new();
    let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(300_000))
        .unwrap_or_else(|error| panic!("manual ore stack stockpile failed: {error}"));
    let ore = CommodityKey::new(MATERIAL_COPPER, FORM_ORE);
    let feed = Mass::from_milligrams(100_000);
    let deposit = |state: &mut AppState| {
        deposit_lot_for_test(
            &registries,
            state,
            stockpile,
            ore,
            feed,
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("manual ore stack lot setup failed: {error}"));
    };
    deposit(&mut state);
    let stack = project_material_stacks(&registries, &state, stockpile)
        .unwrap()
        .into_iter()
        .find(|stack| stack.commodity() == ore)
        .expect("player-visible ore stack missing");
    let snapshot = state.clone();
    let resolved = resolve_manual_comminution_from_stack(
        &registries,
        &state,
        PROCESS_HAND_BREAK_ORE,
        &stack,
        feed,
    )
    .unwrap_or_else(|error| panic!("valid stack ore breaking rejected: {error}"));
    assert!(resolved.duration().value() > 0);
    assert!(matches!(
        resolve_manual_comminution_from_stack(
            &registries,
            &state,
            PROCESS_HAND_BREAK_ORE,
            &stack,
            Mass::ZERO,
        ),
        Err(ManualComminutionResolutionError::Stack(
            MaterialStackProjectionError::ZeroMass
        ))
    ));
    assert!(matches!(
        resolve_manual_constituent_separation_from_stack(
            &registries,
            &state,
            PROCESS_HAND_SORT_NATIVE_COPPER,
            &stack,
            Mass::from_milligrams(100_001),
        ),
        Err(ManualConstituentSeparationResolutionError::Stack(
            MaterialStackProjectionError::MassExceedsStack { .. }
        ))
    ));
    assert_eq!(
        state, snapshot,
        "player stack resolution must not mutate custody"
    );
    deposit(&mut state);
    assert!(matches!(
        resolve_manual_comminution_from_stack(
            &registries,
            &state,
            PROCESS_HAND_BREAK_ORE,
            &stack,
            feed,
        ),
        Err(ManualComminutionResolutionError::Stack(
            MaterialStackProjectionError::StaleInventoryRevision { .. }
        ))
    ));
    assert!(matches!(
        resolve_manual_constituent_separation_from_stack(
            &registries,
            &state,
            PROCESS_HAND_SORT_NATIVE_COPPER,
            &stack,
            feed,
        ),
        Err(ManualConstituentSeparationResolutionError::Stack(
            MaterialStackProjectionError::StaleInventoryRevision { .. }
        ))
    ));
}

#[test]
fn manual_ore_projection_rejects_zero_mass_non_batch() {
    let registries = build_registries();
    let definition = registries
        .ore_processing()
        .get_manual_comminution(PROCESS_HAND_BREAK_ORE)
        .unwrap_or_else(|| panic!("built-in manual comminution definition disappeared"));

    assert_eq!(
        project_manual_ore_duration(
            registries.core().physical_tick_duration(),
            ManualOreProcessProfile::new(
                definition.processing_rate(),
                definition.max_batch_mass(),
                definition.exertion(),
            ),
            Mass::ZERO,
        ),
        Err(ManualOrePhysicsError::ZeroBatchMass)
    );
}

#[test]
fn recovery_profile_requires_rounding_safe_selectivity_margin() {
    let boundary = ConstituentRecoveryProfile::new(900_000, 450_000);
    assert_eq!(boundary.target_ppm(), 900_000);
    assert_eq!(boundary.non_target_ppm(), 450_000);

    assert!(
        std::panic::catch_unwind(|| ConstituentRecoveryProfile::new(900_000, 450_001)).is_err(),
        "a weaker selectivity margin can fail to enrich small whole-milligram batches"
    );
}

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
    production.register_process(ProcessDefinition::new(
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
            FORM_NATIVE_METAL,
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
