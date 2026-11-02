//! Regression contracts for physical-source and gathering-method contextual selection.

use super::*;
use crate::content::{FORM_FOOD, FORM_LUMP, MATERIAL_BERRIES, MATERIAL_STONE, build_registries};
use crate::core::quantity::Temperature;
use crate::material::MaterialComposition;
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

    assert!(matches!(
        classify_direct_surface_gathering_options(&[]),
        DirectSurfaceGatheringResolution::None
    ));
    assert!(matches!(
        classify_direct_surface_gathering_options(&options[..1]),
        DirectSurfaceGatheringResolution::One(_)
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
fn unusable_gathering_does_not_obstruct_a_real_local_drink() {
    use crate::core::state::validate_loaded_state;
    use crate::inventory::deposit_lot_for_test;
    use crate::matter::calculate_matter_accounting;
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};

    let minimum = build_registries()
        .survival()
        .physiology()
        .direct_consumption()
        .minimum_drink_volume();
    for hot in [true, false] {
        let (registries, mut state, stores) = water_and_gathering_context(&[minimum], false, true);
        let position = state.logistics().player().unwrap().position();
        let commodity = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
        let temperature = Temperature::from_millikelvin(if hot { 423_150 } else { 293_150 });
        let source = validate_create_surface_resources(
            &registries,
            &state,
            [SurfaceResourceSpec::new(
                SurfaceResourceKind::Opportunity,
                position,
                commodity,
                Mass::from_milligrams(500_000),
                temperature,
                MaterialComposition::pure(MATERIAL_STONE),
            )
            .unwrap()],
        )
        .unwrap();
        let resource = source.first_resource_id();
        source.commit(&mut state).unwrap();
        if !hot {
            // The viable stone option cannot start work when the carried
            // stockpile has no space. It must not prevent drinking either.
            let carried = state.logistics().player().unwrap().carried_stockpile();
            deposit_lot_for_test(
                &registries,
                &mut state,
                carried,
                commodity,
                registries.survival().physiology().carried_mass_capacity(),
                Temperature::from_millikelvin(293_150),
            )
            .unwrap();
        }
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
        let matter = calculate_matter_accounting(&state).unwrap().total();
        let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
        let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
            .unwrap()
            .into_state(&registries)
            .unwrap();
        let outcome = attempt_empty_hand_secondary_action(&registries, &mut state);
        let replay_outcome = attempt_empty_hand_secondary_action(&registries, &mut replay);
        assert_eq!(outcome, replay_outcome);
        assert!(matches!(
            outcome,
            SecondaryActionOutcome::DrinkingStarted(ref drink) if drink.store() == stores[0]
        ));
        assert_eq!(state, replay);
        assert_eq!(
            state.surface().get(resource).unwrap().remaining_mass(),
            Mass::from_milligrams(500_000)
        );
        assert_eq!(calculate_matter_accounting(&state).unwrap().total(), matter);
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }
}

#[test]
fn unavailable_gathering_without_drink_retains_its_specific_rejection() {
    let (registries, mut state, _) = water_and_gathering_context(&[], false, true);
    let position = state.logistics().player().unwrap().position();
    let commodity = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
    let source = validate_create_surface_resources(
        &registries,
        &state,
        [SurfaceResourceSpec::new(
            SurfaceResourceKind::Opportunity,
            position,
            commodity,
            Mass::from_milligrams(500_000),
            Temperature::from_millikelvin(423_150),
            MaterialComposition::pure(MATERIAL_STONE),
        )
        .unwrap()],
    )
    .unwrap();
    let resource = source.first_resource_id();
    source.commit(&mut state).unwrap();
    assert!(matches!(
        attempt_empty_hand_secondary_action(&registries, &mut state),
        SecondaryActionOutcome::SurfaceGatheringRejected(SurfaceGatheringError::ResourceTooHot {
            resource: blocked,
            ..
        }) if blocked == resource
    ));
}

fn water_and_gathering_context(
    water_volumes: &[crate::core::quantity::Volume],
    forage: bool,
    thirsty: bool,
) -> (
    crate::registry::Registries,
    AppState,
    Vec<crate::fluid::FluidStoreId>,
) {
    use crate::content::{FLUID_WATER, FORM_LUMP, MATERIAL_STONE};
    use crate::fluid::add_fluid_store_with_contents_for_fixture;
    use crate::logistics::validate_place_fluid_store;

    let registries = build_registries();
    let mut state = AppState::new();
    let at = VoxelCoord::new(0, 0, 0);
    validate_admit_player(&registries, &state, at)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let mut stores = Vec::new();
    for &volume in water_volumes {
        let store = add_fluid_store_with_contents_for_fixture(
            &registries,
            &mut state,
            volume,
            FLUID_WATER,
            volume,
            Temperature::from_millikelvin(293_150),
        )
        .unwrap();
        validate_place_fluid_store(&state, store, at)
            .unwrap()
            .commit(&mut state)
            .unwrap();
        stores.push(store);
    }
    if forage {
        let commodity = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
        validate_create_surface_resources(
            &registries,
            &state,
            [SurfaceResourceSpec::new(
                SurfaceResourceKind::Opportunity,
                at,
                commodity,
                Mass::from_milligrams(500_000),
                Temperature::from_millikelvin(293_150),
                MaterialComposition::pure(MATERIAL_STONE),
            )
            .unwrap()],
        )
        .unwrap()
        .commit(&mut state)
        .unwrap();
    }
    if thirsty {
        let _ = crate::simulation::advance_tick(&registries, &mut state).unwrap();
        assert!(
            state.survival().player().unwrap().hydration()
                < registries.survival().physiology().maximum_hydration()
        );
    }
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
    (registries, state, stores)
}

#[test]
fn unusable_water_remainder_does_not_obstruct_empty_hand_gathering() {
    use crate::core::quantity::Volume;
    use crate::survival::available_local_drink_sources;

    let registries = build_registries();
    let minimum = registries
        .survival()
        .physiology()
        .direct_consumption()
        .minimum_drink_volume()
        .microliters();
    let (registries, mut state, stores) =
        water_and_gathering_context(&[Volume::from_microliters(minimum - 1)], true, true);
    assert_eq!(
        available_local_drink_sources(&registries, &state)
            .map(|source| source.store())
            .collect::<Vec<_>>(),
        stores,
        "the observed water remains visible even when it cannot be consumed"
    );
    assert!(matches!(
        attempt_empty_hand_secondary_action(&registries, &mut state),
        SecondaryActionOutcome::SurfaceGatheringStarted(_)
    ));
    assert_eq!(
        state.fluid().get_store(stores[0]).unwrap().stored_volume(),
        Volume::from_microliters(minimum - 1)
    );
}

#[test]
fn contextual_drinking_ignores_empty_servings_but_preserves_actual_ambiguity() {
    use crate::core::quantity::Volume;

    let minimum = build_registries()
        .survival()
        .physiology()
        .direct_consumption()
        .minimum_drink_volume();
    let tiny = Volume::from_microliters(minimum.microliters() - 1);
    let (registries, mut state, stores) =
        water_and_gathering_context(&[tiny, minimum], false, true);
    assert!(matches!(
        attempt_empty_hand_secondary_action(&registries, &mut state),
        SecondaryActionOutcome::DrinkingStarted(outcome) if outcome.store() == stores[1]
    ));

    let (registries, mut state, _) = water_and_gathering_context(&[minimum], true, true);
    assert_eq!(
        attempt_empty_hand_secondary_action(&registries, &mut state),
        SecondaryActionOutcome::AmbiguousEmptyHandContext {
            drink_sources: 1,
            surface_resources: 1,
        }
    );

    let (registries, mut state, _) = water_and_gathering_context(&[minimum], true, false);
    assert!(matches!(
        attempt_empty_hand_secondary_action(&registries, &mut state),
        SecondaryActionOutcome::SurfaceGatheringStarted(_)
    ));
}

#[test]
fn unavailable_water_egress_cannot_mask_local_gathering() {
    use crate::fluid::validate_fluid_egress;
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};

    let minimum = build_registries()
        .survival()
        .physiology()
        .direct_consumption()
        .minimum_drink_volume();
    let (registries, state, stores) = water_and_gathering_context(&[minimum], true, true);
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    encoded["state"]["systems"]["fluid"]["revision"] = serde_json::json!(u64::MAX);
    let mut state = serde_json::from_value::<LoadedSaveEnvelope>(encoded)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert!(validate_fluid_egress(&registries, &state, stores[0], minimum).is_err());
    assert_eq!(
        state.fluid().get_store(stores[0]).unwrap().stored_volume(),
        minimum
    );
    assert!(matches!(
        attempt_empty_hand_secondary_action(&registries, &mut state),
        SecondaryActionOutcome::SurfaceGatheringStarted(_)
    ));
    assert_eq!(
        state.fluid().get_store(stores[0]).unwrap().stored_volume(),
        minimum
    );
}

#[test]
fn water_only_context_retains_specific_failure_and_satisfaction_results() {
    use crate::core::quantity::Volume;
    use crate::survival::DrinkStoreToTargetError;

    let minimum = build_registries()
        .survival()
        .physiology()
        .direct_consumption()
        .minimum_drink_volume();
    let tiny = Volume::from_microliters(minimum.microliters() - 1);
    let (registries, mut state, stores) = water_and_gathering_context(&[tiny], false, true);
    assert_eq!(
        attempt_empty_hand_secondary_action(&registries, &mut state),
        SecondaryActionOutcome::DrinkingRejected(DrinkStoreToTargetError::InsufficientVolume {
            store: stores[0],
            available: tiny,
            required: minimum,
        })
    );

    let (registries, mut state, _) = water_and_gathering_context(&[minimum], false, false);
    assert_eq!(
        attempt_empty_hand_secondary_action(&registries, &mut state),
        SecondaryActionOutcome::AlreadySatisfied
    );
}
