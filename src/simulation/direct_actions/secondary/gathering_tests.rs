//! Local physical-resource and method ambiguity is owned by gathering selection.

use super::*;
use crate::content::{FORM_FOOD, FORM_LUMP, MATERIAL_BERRIES, MATERIAL_STONE, build_registries};
use crate::core::quantity::{Mass, Temperature};
use crate::material::{CommodityKey, MaterialComposition};
use crate::player::validate_admit_player;
use crate::spatial::VoxelCoord;
use crate::surface::{SurfaceResourceKind, SurfaceResourceSpec, validate_create_surface_resources};

#[test]
fn contextual_gathering_counts_physical_sources_before_method_choices() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let specs = [
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
    ]
    .map(|commodity| {
        SurfaceResourceSpec::new(
            SurfaceResourceKind::Opportunity,
            position,
            commodity,
            Mass::from_milligrams(500_000),
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(commodity.material()),
        )
        .unwrap()
    });
    validate_create_surface_resources(&registries, &state, specs)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_admit_player(&registries, &state, position)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let options = player_surface_gathering_options(&registries, &state).unwrap();
    assert_eq!(options.len(), 2);
    assert_ne!(options[0].resource(), options[1].resource());
    let options = options
        .into_iter()
        .map(|option| (option, option.suggested_mass_at(&state)))
        .collect::<Vec<_>>();

    assert!(matches!(
        classify_direct_surface_gathering_options(&[]),
        DirectSurfaceGatheringResolution::None
    ));
    assert!(matches!(
        classify_direct_surface_gathering_options(&options[..1]),
        DirectSurfaceGatheringResolution::One(..)
    ));
    // An additional method for the same observed source must not be
    // counted as another physical resource or hide the second source.
    assert!(matches!(
        classify_direct_surface_gathering_options(&[options[0], options[0]]),
        DirectSurfaceGatheringResolution::AmbiguousMethods { count: 2 }
    ));
    assert!(matches!(
        classify_direct_surface_gathering_options(&[options[0], options[0], options[1]]),
        DirectSurfaceGatheringResolution::AmbiguousResources { count: 2 }
    ));
}

#[test]
fn contextual_stone_gathering_uses_affordable_portion_but_explicit_amount_is_not_reduced() {
    use crate::core::quantity::Energy;
    use crate::core::state::validate_loaded_state;
    use crate::labor::PlayerWorkStartError;
    use crate::matter::calculate_matter_accounting;
    use crate::survival::player_record_with_thermal_comfort;

    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let spec = SurfaceResourceSpec::new(
        SurfaceResourceKind::Opportunity,
        position,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap();
    validate_create_surface_resources(&registries, &state, [spec])
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_admit_player(&registries, &mut state, position)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let option = player_surface_gathering_options(&registries, &state).unwrap()[0];
    let ceiling = option.suggested_mass_at(&state);
    let player = *state.survival().player().unwrap();
    let set_energy = |state: &mut AppState, energy: Energy| {
        let revision = state.survival().revision();
        state.survival_state_mut().apply_player(
            revision,
            revision + 1,
            player_record_with_thermal_comfort(
                energy,
                player.hydration(),
                player.vitality(),
                player.nutrition(),
                player.vitality_recovery_remainder(),
                player.thermal_comfort_ppm(),
            ),
        );
    };
    set_energy(&mut state, Energy::ZERO);
    let Err(SurfaceGatheringError::PlayerWork(PlayerWorkStartError::InsufficientMetabolicEnergy {
        required,
        ..
    })) = assess_player_surface_gathering_option_admission(&registries, &state, option, ceiling)
    else {
        panic!("the ordinary batch must exceed an empty reserve");
    };
    set_energy(
        &mut state,
        Energy::from_nanojoules(required.nanojoules() / 2),
    );
    let affordable =
        assess_affordable_player_gathering_mass(&registries, &state, option, ceiling).unwrap();
    assert!(!affordable.is_zero() && affordable < ceiling);
    let before = state.clone();
    assert!(matches!(
        attempt_explicit_surface_gathering(&registries, &mut state, option.resource(), ceiling),
        SecondaryActionOutcome::SurfaceGatheringRejected(SurfaceGatheringError::PlayerWork(
            PlayerWorkStartError::InsufficientMetabolicEnergy { .. }
        ))
    ));
    assert_eq!(state, before);
    assert!(matches!(
        attempt_empty_hand_secondary_action(&registries, &mut state),
        SecondaryActionOutcome::SurfaceGatheringStarted(work)
            if work.gathered_mass() == affordable
    ));
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        calculate_matter_accounting(&before).unwrap().total()
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn emergency_forage_costs_vitality_and_enables_recovery_after_save_reload() {
    use crate::core::quantity::{Energy, Volume};
    use crate::core::state::validate_loaded_state;
    use crate::labor::PlayerWorkStartError;
    use crate::matter::calculate_matter_accounting;
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
    use crate::survival::{Vitality, player_record_with_thermal_comfort};

    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let created = validate_create_surface_resources(
        &registries,
        &state,
        [SurfaceResourceSpec::new(
            SurfaceResourceKind::Opportunity,
            position,
            CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
            Mass::from_milligrams(1_000_000),
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(MATERIAL_BERRIES),
        )
        .unwrap()],
    )
    .unwrap();
    let resource = created.first_resource_id();
    created.commit(&mut state).unwrap();
    let carried = validate_admit_player(&registries, &state, position)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    let original = *state.survival().player().unwrap();
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record_with_thermal_comfort(
            Energy::ZERO,
            Volume::ZERO,
            original.vitality(),
            original.nutrition(),
            original.vitality_recovery_remainder(),
            original.thermal_comfort_ppm(),
        ),
    );
    let option = player_surface_gathering_options(&registries, &state).unwrap()[0];
    let portion = option.suggested_mass_at(&state);
    let full = option.maximum_mass_at(&state);
    assert!(full > portion);
    assert!(matches!(
        assess_player_surface_gathering_option_admission(&registries, &state, option, full),
        Err(SurfaceGatheringError::PlayerWork(
            PlayerWorkStartError::InsufficientMetabolicEnergy { .. }
        ))
    ));
    assert_eq!(
        assess_affordable_player_gathering_mass(&registries, &state, option, full),
        Ok(portion)
    );
    let total = calculate_matter_accounting(&state).unwrap().total();
    // The same rescue attempt can fail if the player has too little vitality.
    let mut dying = state.clone();
    let revision = dying.survival().revision();
    dying.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record_with_thermal_comfort(
            Energy::ZERO,
            Volume::ZERO,
            Vitality::from_parts_per_million_unchecked(1),
            original.nutrition(),
            original.vitality_recovery_remainder(),
            original.thermal_comfort_ppm(),
        ),
    );
    assert!(matches!(
        attempt_empty_hand_secondary_action(&registries, &mut dying),
        SecondaryActionOutcome::SurfaceGatheringStarted(_)
    ));
    let _ = crate::simulation::advance_tick(&registries, &mut dying).unwrap();
    assert_eq!(
        dying.survival().player().unwrap().vitality(),
        Vitality::ZERO
    );
    assert!(dying.player_work().active().is_none());
    assert_eq!(
        dying.surface().get(resource).unwrap().remaining_mass(),
        full
    );
    assert_eq!(
        dying
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .reserved_inbound(),
        Mass::ZERO
    );
    assert_eq!(calculate_matter_accounting(&dying).unwrap().total(), total);
    assert_eq!(validate_loaded_state(&registries, &dying), Ok(()));
    let SecondaryActionOutcome::SurfaceGatheringStarted(work) =
        attempt_empty_hand_secondary_action(&registries, &mut state)
    else {
        panic!("living player must be able to forage a rescue portion");
    };
    assert_eq!(work.gathered_mass(), portion);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    // One exact active slow boundary at a time: no reserve or matter grant.
    while state.tick() < work.completes_at() {
        let a = crate::simulation::advance_tick(&registries, &mut state).unwrap();
        let b = crate::simulation::advance_tick(&registries, &mut replay).unwrap();
        assert_eq!(a, b);
        assert_eq!(state, replay);
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }
    let weakened = state.survival().player().unwrap();
    assert!(weakened.vitality() < original.vitality());
    assert!(weakened.vitality() > Vitality::ZERO);
    assert_eq!(weakened.metabolic_energy(), Energy::ZERO);
    assert_eq!(
        state
            .surface()
            .get(resource)
            .unwrap()
            .remaining_mass()
            .milligrams(),
        full.milligrams() - portion.milligrams()
    );
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), total);
    let stacks = crate::inventory::project_material_stacks(&registries, &state, carried).unwrap();
    let meal = &stacks[0];
    let eating = crate::survival::validate_eat_stack_to_full(&registries, &state, meal)
        .unwrap()
        .unwrap();
    let _ = eating.commit(&mut state).unwrap();
    while state.player_work().active().is_some() {
        let _ = crate::simulation::advance_tick(&registries, &mut state).unwrap();
    }
    let recovered = state.survival().player().unwrap();
    assert!(recovered.metabolic_energy() > Energy::ZERO);
    assert!(recovered.hydration() > Volume::ZERO);
    assert!(recovered.vitality() > Vitality::ZERO);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn explicit_gathering_selects_only_method_feasible_for_requested_mass() {
    use crate::content::make_test_registries_with_surface_gathering;
    use crate::core::quantity::{Energy, Volume};
    use crate::core::state::validate_loaded_state;
    use crate::core::time::TickSpan;
    use crate::labor::{SurfaceGatheringDefinition, SurfaceGatheringMethodId};
    use crate::matter::calculate_matter_accounting;
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
    use crate::survival::SurvivalExertion;

    let commodity = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
    let narrow_method = SurfaceGatheringMethodId::new(900_001);
    let broad_method = SurfaceGatheringMethodId::new(900_002);
    let exertion = SurvivalExertion::new(
        Energy::from_nanojoules(1_000_000_000),
        Volume::from_microliters(10),
    );
    let registries = make_test_registries_with_surface_gathering([
        SurfaceGatheringDefinition::new(
            narrow_method,
            commodity,
            Mass::from_milligrams(100_000),
            Mass::from_milligrams(50_000),
            TickSpan::new(5),
            exertion,
        ),
        SurfaceGatheringDefinition::new(
            broad_method,
            commodity,
            Mass::from_milligrams(500_000),
            Mass::from_milligrams(250_000),
            TickSpan::new(10),
            exertion,
        ),
    ]);
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let created = validate_create_surface_resources(
        &registries,
        &state,
        [SurfaceResourceSpec::new(
            SurfaceResourceKind::Opportunity,
            position,
            commodity,
            Mass::from_milligrams(1_000_000),
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(MATERIAL_STONE),
        )
        .unwrap()],
    )
    .unwrap();
    let resource = created.first_resource_id();
    created.commit(&mut state).unwrap();
    let carried = validate_admit_player(&registries, &state, position)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    assert_eq!(
        player_surface_gathering_options(&registries, &state)
            .unwrap()
            .len(),
        2
    );

    let before = state.clone();
    assert_eq!(
        attempt_explicit_surface_gathering(
            &registries,
            &mut state,
            resource,
            Mass::from_milligrams(50_000),
        ),
        SecondaryActionOutcome::AmbiguousSurfaceGatheringMethods { count: 2 }
    );
    assert_eq!(state, before);
    assert_eq!(
        attempt_explicit_surface_gathering(
            &registries,
            &mut state,
            resource,
            Mass::from_milligrams(600_000),
        ),
        SecondaryActionOutcome::SurfaceGatheringRejected(SurfaceGatheringError::BatchTooLarge {
            requested: Mass::from_milligrams(600_000),
            maximum: Mass::from_milligrams(500_000),
        })
    );
    assert_eq!(state, before);

    let options = player_surface_gathering_options(&registries, &state).unwrap();
    assert_eq!(
        options[0].maximum_batch_mass(),
        Mass::from_milligrams(100_000)
    );
    let chosen_option = options[0];
    let chosen_amount = Mass::from_milligrams(50_000);
    let mut selected = state.clone();
    let selected_result = crate::simulation::advance_control_tick_with_direct_input(
        &registries,
        &mut selected,
        crate::simulation::PlayerDirectControlInput::NEUTRAL.with_secondary(
            crate::simulation::SecondaryControlIntent::gather_surface_option(
                chosen_option,
                chosen_amount,
            ),
        ),
    )
    .unwrap();
    assert!(matches!(
        selected_result.secondary_action(),
        Some(SecondaryActionOutcome::SurfaceGatheringStarted(work))
            if work.method() == narrow_method && work.gathered_mass() == chosen_amount
    ));
    assert!(selected.logistics().pending_secondary_use().is_none());
    assert_eq!(
        selected
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .reserved_inbound(),
        chosen_amount,
    );
    let selected_save = serde_json::to_vec(&SaveEnvelope::new(&registries, &selected)).unwrap();
    let mut selected_replay = serde_json::from_slice::<LoadedSaveEnvelope>(&selected_save)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let completes_at = match selected.player_work().active() {
        Some(crate::labor::PlayerWork::SurfaceGathering { work }) => work.completes_at(),
        other => panic!("selected gathering did not schedule work: {other:?}"),
    };
    while selected.tick() < completes_at {
        let first = crate::simulation::advance_control_tick(&registries, &mut selected).unwrap();
        let replayed =
            crate::simulation::advance_control_tick(&registries, &mut selected_replay).unwrap();
        assert_eq!(first, replayed);
        assert_eq!(selected, selected_replay);
    }
    assert_eq!(selected.player_work().active(), None);
    assert_eq!(
        selected.surface().get(resource).unwrap().remaining_mass(),
        Mass::from_milligrams(950_000),
    );
    assert_eq!(
        calculate_matter_accounting(&selected).unwrap().total(),
        calculate_matter_accounting(&before).unwrap().total(),
    );
    assert_eq!(validate_loaded_state(&registries, &selected), Ok(()));
    let stale = crate::simulation::advance_control_tick_with_direct_input(
        &registries,
        &mut selected,
        crate::simulation::PlayerDirectControlInput::NEUTRAL.with_secondary(
            crate::simulation::SecondaryControlIntent::gather_surface_option(
                chosen_option,
                chosen_amount,
            ),
        ),
    )
    .unwrap();
    assert!(matches!(
        stale.secondary_action(),
        Some(SecondaryActionOutcome::SurfaceGatheringRejected(
            SurfaceGatheringError::StalePlayerOption { .. }
        ))
    ));
    assert_eq!(selected.player_work().active(), None);
    let refreshed = chosen_option
        .refresh(&registries, &selected)
        .unwrap()
        .unwrap();
    assert_eq!(refreshed.resource(), resource);
    assert_eq!(refreshed.source_mass(), Mass::from_milligrams(950_000));
    assert_eq!(
        refreshed.maximum_batch_mass(),
        chosen_option.maximum_batch_mass()
    );
    let refreshed_result = crate::simulation::advance_control_tick_with_direct_input(
        &registries,
        &mut selected,
        crate::simulation::PlayerDirectControlInput::NEUTRAL.with_secondary(
            crate::simulation::SecondaryControlIntent::gather_surface_option(
                refreshed,
                chosen_amount,
            ),
        ),
    )
    .unwrap();
    assert!(matches!(
        refreshed_result.secondary_action(),
        Some(SecondaryActionOutcome::SurfaceGatheringStarted(work))
            if work.method() == narrow_method && work.gathered_mass() == chosen_amount
    ));

    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let checkpoint = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&checkpoint)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for game in [&mut state, &mut replay] {
        let started = attempt_explicit_surface_gathering(
            &registries,
            game,
            resource,
            Mass::from_milligrams(200_000),
        );
        assert!(matches!(
            started,
            SecondaryActionOutcome::SurfaceGatheringStarted(work)
                if work.method() == broad_method
                    && work.gathered_mass() == Mass::from_milligrams(200_000)
        ));
    }
    assert_eq!(state, replay);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .reserved_inbound(),
        Mass::from_milligrams(200_000),
    );
    assert_eq!(
        state.surface().get(resource).unwrap().remaining_mass(),
        Mass::from_milligrams(1_000_000),
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
