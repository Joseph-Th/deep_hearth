//! Physical scavenger cargo priorities, division, and recovery contracts.

use super::*;

#[test]
fn scout_prefers_later_food_over_earlier_stone_at_same_junction() {
    use crate::content::{FORM_FOOD, MATERIAL_BERRIES};
    let (registries, mut state, spawn) = generated_opening();
    let [den, junction, _] = colony(&state).route;
    let stone = place_loose_stone_drop(&registries, &mut state, junction);
    let food = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        junction,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        Mass::from_milligrams(250_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_BERRIES),
    )
    .unwrap();
    let creation = validate_create_surface_resources(&registries, &state, [food]).unwrap();
    let food_id = creation.first_resource_id();
    assert!(stone < food_id, "the earlier stone drop must lose the tie");
    creation.commit(&mut state).unwrap();
    let total = calculate_matter_accounting(&state).unwrap().total();
    validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();

    advance_to_tick(&registries, &mut state, 185);
    assert_eq!(state.surface().get(food_id).unwrap().position(), den);
    assert_eq!(state.surface().get(stone).unwrap().position(), junction);
    let checkpoint = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let restored: LoadedSaveEnvelope = serde_json::from_slice(&checkpoint).unwrap();
    let mut replay = restored.into_state(&registries).unwrap();
    for _ in 0..10 {
        assert_eq!(
            advance_tick(&registries, &mut state).unwrap(),
            advance_tick(&registries, &mut replay).unwrap()
        );
    }
    assert_eq!(state, replay);
    assert_eq!(
        state.surface().get(stone).unwrap().position(),
        den,
        "the less desirable parcel remains eligible on a subsequent trip"
    );
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), total);
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}

#[test]
fn nearer_stone_drop_still_outweighs_farther_food_cache() {
    use crate::content::{FORM_FOOD, MATERIAL_BERRIES};
    let (registries, mut state, spawn) = generated_opening();
    let [den, junction, tip] = colony(&state).route;
    let forage = state
        .logistics()
        .traversable_destinations(junction)
        .find(|position| {
            *position != den
                && *position != tip
                && state.logistics().has_traversal_edge(*position, spawn)
        })
        .unwrap();
    let stone = place_loose_stone_drop(&registries, &mut state, forage);
    let food = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        spawn,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        Mass::from_milligrams(250_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_BERRIES),
    )
    .unwrap();
    let creation = validate_create_surface_resources(&registries, &state, [food]).unwrap();
    let food_id = creation.first_resource_id();
    creation.commit(&mut state).unwrap();
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    advance_to_tick(&registries, &mut state, 185);
    assert_eq!(colony(&state).scout_position(), forage);
    assert_eq!(colony(&state).detour, Some(forage));
    assert_eq!(
        colony(&state).detour_goal,
        None,
        "one-edge accessibility outranks material preference"
    );
    advance_to_tick(&registries, &mut state, 195);
    assert_eq!(state.surface().get(stone).unwrap().position(), den);
    assert_eq!(state.surface().get(food_id).unwrap().position(), spawn);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
}

#[test]
fn scout_splits_food_before_older_heavy_stone_when_no_portable_parcel_fits() {
    use crate::content::{FORM_FOOD, MATERIAL_BERRIES};
    let (registries, mut state, spawn) = generated_opening();
    let [den, junction, _] = colony(&state).route;
    let stone = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        junction,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        Mass::from_milligrams(2_000_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap();
    let food = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        junction,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        Mass::from_milligrams(2_000_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_BERRIES),
    )
    .unwrap();
    let creation = validate_create_surface_resources(&registries, &state, [stone, food]).unwrap();
    let stone_id = creation.first_resource_id();
    creation.commit(&mut state).unwrap();
    let food_id = state
        .surface()
        .available_resources_at(Some(junction))
        .find(|r| r.commodity().form() == FORM_FOOD && r.kind() == SurfaceResourceKind::Drop)
        .unwrap()
        .id();
    assert!(stone_id < food_id);
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    advance_to_tick(&registries, &mut state, 185);
    assert_eq!(
        state.surface().get(stone_id).unwrap().remaining_mass(),
        Mass::from_milligrams(2_000_000)
    );
    assert_eq!(
        state.surface().get(food_id).unwrap().remaining_mass(),
        Mass::from_milligrams(1_750_000)
    );
    let catch = state
        .surface()
        .available_resources_at(Some(den))
        .find(|r| r.kind() == SurfaceResourceKind::Drop)
        .unwrap();
    assert_eq!(
        catch.commodity(),
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD)
    );
    assert_eq!(catch.remaining_mass(), Mass::from_milligrams(250_000));
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
}

#[test]
fn scout_prioritizes_an_intact_portable_parcel_over_splitting_bulk() {
    let (registries, mut state, spawn) = generated_opening();
    let [_, junction, tip] = colony(&state).route;
    let heavy_mass = Mass::from_milligrams(2_000_000);
    let heavy_spec = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        tip,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        heavy_mass,
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .expect("heavy loose stone parcel must be representable");
    let creation = validate_create_surface_resources(&registries, &state, [heavy_spec])
        .expect("heavy drop creation failed");
    let heavy = creation.first_resource_id();
    creation
        .commit(&mut state)
        .expect("heavy drop commit failed");
    let portable = place_loose_stone_drop(&registries, &mut state, tip);
    let total = calculate_matter_accounting(&state).unwrap().total();
    validate_admit_player(&registries, &state, spawn)
        .expect("player admission failed")
        .commit(&mut state)
        .expect("player commit failed");
    advance_to_tick(&registries, &mut state, 190);
    assert_eq!(colony(&state).scavenged_drop, Some(portable));
    assert_eq!(state.surface().get(portable).unwrap().position(), junction);
    assert_eq!(state.surface().get(heavy).unwrap().position(), tip);
    assert_eq!(
        state.surface().get(heavy).unwrap().remaining_mass(),
        heavy_mass
    );
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), total);
}

#[test]
fn scout_splits_bulky_material_at_junction_into_finite_recoverable_den_parcels() {
    let (registries, mut state, spawn) = generated_opening();
    let [den, junction, tip] = colony(&state).route;
    let heavy_mass = Mass::from_milligrams(2_000_000);
    let heavy_spec = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        junction,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        heavy_mass,
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap();
    let creation = validate_create_surface_resources(&registries, &state, [heavy_spec]).unwrap();
    let heavy = creation.first_resource_id();
    creation.commit(&mut state).unwrap();
    let original = state.surface().get(heavy).unwrap().clone();
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    advance_to_tick(&registries, &mut state, 180);
    assert_eq!(colony(&state).scout_position(), junction);
    assert!(colony(&state).noticed_player_drop);
    advance_to_tick(&registries, &mut state, 185);
    assert_eq!(colony(&state).scout_position(), den);
    assert_eq!(colony(&state).route[2], tip);
    let first = state
        .surface()
        .available_resources_at(Some(den))
        .find(|resource| resource.kind() == SurfaceResourceKind::Drop)
        .unwrap();
    let first_id = first.id();
    assert_ne!(first_id, heavy);
    let portion = Mass::from_milligrams(250_000);
    assert_eq!(first.remaining_mass(), portion);
    assert_eq!(first.composition(), original.composition());
    assert_eq!(first.temperature(), original.temperature());
    assert_eq!(first.material_provenance(), original.material_provenance());
    assert_eq!(
        first.ambient_storage_age_parts_at(state.tick()),
        original.ambient_storage_age_parts_at(state.tick())
    );
    assert_eq!(state.surface().get(heavy).unwrap().position(), junction);
    assert_eq!(
        state.surface().get(heavy).unwrap().remaining_mass(),
        heavy_mass.checked_sub(portion).unwrap()
    );
    let envelope = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&envelope).unwrap();
    let mut restored = loaded
        .into_state(&registries)
        .expect("split piles must load");
    for _ in 0..20 {
        assert_eq!(
            advance_tick(&registries, &mut state).unwrap(),
            advance_tick(&registries, &mut restored).unwrap()
        );
    }
    assert_eq!(state, restored);
    assert_eq!(
        state.surface().get(heavy).unwrap().remaining_mass(),
        heavy_mass
            .checked_sub(Mass::from_milligrams(500_000))
            .unwrap()
    );
    assert_eq!(crate::kobolds::den_cache_space(&state, den), Mass::ZERO);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}

#[test]
fn scout_splits_bulky_tip_pile_then_carries_daughter_parcel_one_step_at_a_time() {
    let (registries, mut state, spawn) = generated_opening();
    let [den, junction, tip] = colony(&state).route;
    let large = Mass::from_milligrams(2_000_000);
    let spec = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        tip,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        large,
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap();
    let creation = validate_create_surface_resources(&registries, &state, [spec]).unwrap();
    let original = creation.first_resource_id();
    creation.commit(&mut state).unwrap();
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    advance_to_tick(&registries, &mut state, 189);
    let step = advance_tick(&registries, &mut state).unwrap();
    let (new_id, mass) = step
        .kobold_scout()
        .unwrap()
        .split_drop()
        .expect("locally visible split should disclose the created parcel");
    assert_ne!(new_id, original);
    assert_eq!(mass, Mass::from_milligrams(250_000));
    assert_eq!(state.surface().get(original).unwrap().position(), tip);
    assert_eq!(
        state.surface().get(original).unwrap().remaining_mass(),
        large.checked_sub(mass).unwrap()
    );
    assert_eq!(state.surface().get(new_id).unwrap().position(), junction);
    assert_eq!(colony(&state).scavenged_drop, Some(new_id));
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let saved: LoadedSaveEnvelope = serde_json::from_slice(&encoded).unwrap();
    let mut restored = saved
        .into_state(&registries)
        .expect("split cargo must save and load");
    for _ in 0..5 {
        assert_eq!(
            advance_tick(&registries, &mut state).unwrap(),
            advance_tick(&registries, &mut restored).unwrap()
        );
    }
    assert_eq!(state, restored);
    assert_eq!(colony(&state).scavenged_drop, None);
    assert_eq!(state.surface().get(new_id).unwrap().position(), den);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
}

#[test]
fn bulk_splitting_respects_last_fifty_grams_of_den_capacity() {
    let (registries, mut state, spawn) = generated_opening();
    let [den, junction, _] = colony(&state).route;
    let stone = CommodityKey::new(MATERIAL_STONE, FORM_CHIP);
    let bulk = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        junction,
        stone,
        Mass::from_milligrams(2_000_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap();
    let preexisting = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        den,
        stone,
        Mass::from_milligrams(450_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap();
    let plan = validate_create_surface_resources(&registries, &state, [bulk, preexisting]).unwrap();
    let bulk_id = plan.first_resource_id();
    plan.commit(&mut state).unwrap();
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    advance_to_tick(&registries, &mut state, 185);
    let remaining = state.surface().get(bulk_id).unwrap().remaining_mass();
    assert_eq!(remaining, Mass::from_milligrams(1_950_000));
    let den_mass = state
        .surface()
        .available_resources_at(Some(den))
        .filter(|item| item.kind() == SurfaceResourceKind::Drop)
        .map(|item| item.remaining_mass().milligrams())
        .sum::<u64>();
    assert_eq!(den_mass, 500_000);
    assert_eq!(crate::kobolds::den_cache_space(&state, den), Mass::ZERO);
    advance_to_tick(&registries, &mut state, 225);
    assert_eq!(
        state.surface().get(bulk_id).unwrap().remaining_mass(),
        remaining
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}

#[test]
fn guarding_exposed_bulk_denies_the_scout_a_split() {
    let (registries, mut state, _) = generated_opening();
    let [_, junction, patrol_tip] = colony(&state).route;
    let heavy = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        junction,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        Mass::from_milligrams(2_000_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap();
    let plan = validate_create_surface_resources(&registries, &state, [heavy]).unwrap();
    let id = plan.first_resource_id();
    plan.commit(&mut state).unwrap();
    validate_admit_player(&registries, &state, patrol_tip)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    advance_to_tick(&registries, &mut state, 205);
    assert_eq!(
        state.surface().get(id).unwrap().remaining_mass(),
        Mass::from_milligrams(2_000_000)
    );
    assert!(
        !state
            .surface()
            .resources()
            .any(|r| r.kind() == SurfaceResourceKind::Drop && r.id() != id)
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}

#[test]
fn scavenging_moves_one_finite_drop_to_den_without_deleting_or_replacing_it() {
    let (registries, mut state, spawn) = generated_opening();
    let [den, junction, tip] = colony(&state).route;
    let drop = place_loose_stone_drop(&registries, &mut state, tip);
    let original_parcel = state.surface().get(drop).unwrap().clone();
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    validate_admit_player(&registries, &state, spawn)
        .expect("player admission should work")
        .commit(&mut state)
        .expect("player admission should commit");

    advance_to_tick(&registries, &mut state, 185);
    assert_eq!(state.surface().get(drop).unwrap().position(), tip);
    let noticed = colony(&state).noticed_player_drop;
    assert!(
        noticed,
        "a visible drop should be discovered before it is taken"
    );
    advance_to_tick(&registries, &mut state, 189);
    let first_drag = advance_tick(&registries, &mut state).expect("scout should retreat");
    assert_eq!(
        first_drag.kobold_scout().unwrap().relocated_drop(),
        Some(drop)
    );
    assert_eq!(state.surface().get(drop).unwrap().position(), junction);
    assert_eq!(colony(&state).scavenged_drop, Some(drop));
    assert_eq!(
        available_nearby_kobold_scouts(&registries, &state)
            .next()
            .expect("player should see the carrying scout")
            .scavenged_material(),
        Some((
            CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
            Mass::from_milligrams(250_000),
        )),
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let saved: LoadedSaveEnvelope = serde_json::from_slice(&encoded).unwrap();
    let mut restored = saved
        .into_state(&registries)
        .expect("carried loot must be loadable");
    for _ in 0..20 {
        let before = advance_tick(&registries, &mut state).unwrap();
        let after = advance_tick(&registries, &mut restored).unwrap();
        assert_eq!(before.kobold_scout(), after.kobold_scout());
    }
    assert_eq!(state, restored);
    assert_eq!(state.surface().get(drop).unwrap().position(), den);
    assert_eq!(
        state.surface().get(drop).unwrap().remaining_mass(),
        Mass::from_milligrams(250_000)
    );
    assert_eq!(colony(&state).scavenged_drop, None);
    let recovered = state.surface().get(drop).unwrap();
    assert_eq!(recovered.composition(), original_parcel.composition());
    assert_eq!(recovered.temperature(), original_parcel.temperature());
    assert_eq!(recovered.particle_size(), original_parcel.particle_size());
    assert_eq!(
        recovered.material_provenance(),
        original_parcel.material_provenance(),
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
}

#[test]
fn player_intercepts_scavenged_drop_using_ordinary_local_pickup() {
    let (registries, mut state, _) = generated_opening();
    let [den, junction, tip] = colony(&state).route;
    let drop = place_loose_stone_drop(&registries, &mut state, tip);
    advance_to_tick(&registries, &mut state, 190);
    assert_eq!(colony(&state).scavenged_drop, Some(drop));
    assert_eq!(state.surface().get(drop).unwrap().position(), junction);
    // The intercepting player's admission establishes the same local body/carry owner used by
    // an actor arriving at the contested junction. Pickup itself is an ordinary control step.
    let carried = validate_admit_player(&registries, &state, junction)
        .expect("intercepting player should be admitted")
        .commit(&mut state)
        .expect("intercepting player admission should commit")
        .carried_stockpile();
    let receipt = advance_control_tick(&registries, &mut state)
        .expect("local pickup should resolve")
        .surface_drop_pickup()
        .expect("player should reclaim scavenged matter before the next return step");
    assert_eq!(receipt.resource(), drop);
    assert_eq!(receipt.picked_mass(), Mass::from_milligrams(250_000));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .stored_mass(),
        Mass::from_milligrams(250_000)
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
    // The player can physically intercept carried loot between patrol steps.
    // Until the next scout decision, the persisted resident still references
    // its lost parcel; loading that transient state must replay the same recovery.
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(state, replay);
    while !state.control_tick().is_simulation_boundary() {
        assert_eq!(
            advance_control_tick(&registries, &mut state).unwrap(),
            advance_control_tick(&registries, &mut replay).unwrap()
        );
    }
    while state.tick().value() < 195 {
        assert_eq!(
            advance_tick(&registries, &mut state).unwrap(),
            advance_tick(&registries, &mut replay).unwrap()
        );
    }
    assert_eq!(state, replay);
    assert_eq!(colony(&state).scavenged_drop, None);
    assert_eq!(
        state.surface().get(drop).unwrap().remaining_mass(),
        Mass::ZERO
    );
    assert!(
        !state
            .surface()
            .available_resources_at(Some(den))
            .any(|item| item.id() == drop)
    );
}

#[test]
fn guarding_the_den_suppresses_patrol_without_touching_supplies() {
    let (registries, mut state, _) = generated_opening();
    let [den, junction, _] = colony(&state).route;
    let drop = place_loose_stone_drop(&registries, &mut state, junction);
    let _ = validate_admit_player(&registries, &state, den)
        .expect("player can guard the den")
        .commit(&mut state)
        .expect("den guardian should be admitted");
    advance_to_tick(&registries, &mut state, 195);
    assert_eq!(colony(&state).scout_position(), den);
    assert!(!colony(&state).noticed_player_drop);
    assert_eq!(state.surface().get(drop).unwrap().position(), junction);
    assert_eq!(colony(&state).scavenged_drop, None);
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}
