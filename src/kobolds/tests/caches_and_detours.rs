//! Scout cache pursuit, detours, and caution contracts.

use super::*;

#[test]
fn scout_scavenges_a_two_edge_camp_cache_and_resumes_its_authored_patrol() {
    let (registries, mut state, spawn) = generated_opening();
    let [den, junction, tip] = colony(&state).route;
    let forage = state
        .logistics()
        .traversable_destinations(junction)
        .find(|step| {
            *step != den && *step != tip && state.logistics().has_traversal_edge(*step, spawn)
        })
        .expect("the opening camp is two authored edges beyond the junction");
    let cache = place_loose_stone_drop(&registries, &mut state, spawn);
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    advance_to_tick(&registries, &mut state, 180);
    assert_eq!(colony(&state).scout_position(), junction);
    advance_to_tick(&registries, &mut state, 185);
    assert_eq!(colony(&state).scout_position(), forage);
    assert_eq!(colony(&state).detour, Some(forage));
    assert_eq!(colony(&state).detour_goal, Some(spawn));
    advance_to_tick(&registries, &mut state, 190);
    assert_eq!(colony(&state).scout_position(), spawn);
    assert_eq!(colony(&state).scout_step, 3);
    assert_eq!(state.surface().get(cache).unwrap().position(), spawn);
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded).unwrap();
    let mut restored = decoded
        .into_state(&registries)
        .expect("four-step outbound detour must be loadable");
    for _ in 0..15 {
        let expected = advance_tick(&registries, &mut state).unwrap();
        assert_eq!(expected, advance_tick(&registries, &mut restored).unwrap());
        if state.tick().value() == 195 {
            assert_eq!(colony(&state).scavenged_drop, Some(cache));
            assert_eq!(state.surface().get(cache).unwrap().position(), forage);
        }
        if state.tick().value() == 200 {
            assert_eq!(state.surface().get(cache).unwrap().position(), junction);
        }
    }
    assert_eq!(state, restored);
    assert_eq!(colony(&state).scout_position(), den);
    assert_eq!(colony(&state).detour, None);
    assert_eq!(colony(&state).detour_goal, None);
    assert_eq!(state.surface().get(cache).unwrap().position(), den);
    assert_eq!(
        state.surface().get(cache).unwrap().remaining_mass(),
        Mass::from_milligrams(250_000)
    );
    advance_to_tick(&registries, &mut state, 215);
    assert_eq!(colony(&state).scout_position(), tip);
    assert_eq!(colony(&state).route, [den, junction, tip]);
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
fn player_intercepts_far_cache_before_scout_reaches_it() {
    let (registries, mut state, spawn) = generated_opening();
    let [den, junction, tip] = colony(&state).route;
    let parcel = place_loose_stone_drop(&registries, &mut state, spawn);
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    advance_to_tick(&registries, &mut state, 185);
    assert_eq!(colony(&state).scout_step, 2);
    assert_eq!(colony(&state).detour_goal, Some(spawn));
    assert!(!colony(&state).returning);
    let first = colony(&state).scout_position();

    // Canonical pickup happens while the scout traverses its intermediate
    // graph cell; the player leaves before the next five-tick scout step.
    let carried = validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    let pickup = advance_control_tick(&registries, &mut state)
        .unwrap()
        .surface_drop_pickup()
        .expect("camp parcel is still pickable");
    assert_eq!(pickup.resource(), parcel);
    assert_eq!(pickup.picked_mass(), Mass::from_milligrams(250_000));
    // Observe the stale-goal cancellation from above the junction without
    // guarding its return edge. A player standing at the old tip would now
    // correctly intercept the homeward scout instead.
    crate::logistics::relocate_player_for_test(
        &mut state,
        VoxelCoord::new(junction.x(), junction.y() + 2, junction.z()),
    );
    while !state.control_tick().is_simulation_boundary() {
        let _ = advance_control_tick(&registries, &mut state).unwrap();
    }
    let checkpoint = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&checkpoint).unwrap();
    let mut restored = loaded.into_state(&registries).unwrap();

    advance_to_tick(&registries, &mut state, 189);
    advance_to_tick(&registries, &mut restored, 189);
    let retreat = advance_tick(&registries, &mut state).unwrap();
    assert_eq!(retreat, advance_tick(&registries, &mut restored).unwrap());
    assert_eq!(colony(&state).scout_position(), junction);
    assert!(colony(&state).returning);
    assert_eq!(colony(&state).detour, Some(first));
    assert!(
        retreat.kobold_scout().is_some_and(
            |event| event.cancelled_empty_cache_raid() && !event.turned_back_at_barrier()
        )
    );
    advance_to_tick(&registries, &mut state, 195);
    advance_to_tick(&registries, &mut restored, 195);
    assert_eq!(state, restored);
    assert_eq!(colony(&state).scout_position(), den);
    assert_eq!(colony(&state).route, [den, junction, tip]);
    assert_eq!(colony(&state).detour, None);
    assert_eq!(colony(&state).detour_goal, None);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .stored_mass(),
        Mass::from_milligrams(250_000)
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
fn full_den_cancels_a_far_cache_raid_before_contact() {
    let (registries, mut state, spawn) = generated_opening();
    let [den, junction, tip] = colony(&state).route;
    let parcel = place_loose_stone_drop(&registries, &mut state, spawn);
    advance_to_tick(&registries, &mut state, 185);
    assert_eq!(colony(&state).detour_goal, Some(spawn));
    let occupied = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        den,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        Mass::from_milligrams(500_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap();
    validate_create_surface_resources(&registries, &state, [occupied])
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    advance_to_tick(&registries, &mut state, 190);
    assert_eq!(colony(&state).scout_position(), junction);
    assert!(colony(&state).returning);
    assert_eq!(state.surface().get(parcel).unwrap().position(), spawn);
    advance_to_tick(&registries, &mut state, 195);
    assert_eq!(colony(&state).scout_position(), den);
    assert_eq!(colony(&state).detour, None);
    assert_eq!(colony(&state).detour_goal, None);
    assert_eq!(colony(&state).route, [den, junction, tip]);
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
fn invalid_two_edge_scout_detours_are_rejected_on_load() {
    let (registries, mut state, spawn) = generated_opening();
    let [den, junction, tip] = colony(&state).route;
    place_loose_stone_drop(&registries, &mut state, spawn);
    advance_to_tick(&registries, &mut state, 190);
    assert_eq!(colony(&state).scout_step, 3);
    let valid = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    for invalid in [den, junction, tip, VoxelCoord::new(999, 2, 999)] {
        let mut altered = valid.clone();
        altered["state"]["systems"]["kobolds"]["colony"]["detour_goal"] =
            serde_json::to_value(invalid).unwrap();
        let envelope: LoadedSaveEnvelope = serde_json::from_value(altered).unwrap();
        assert!(envelope.into_state(&registries).is_err());
    }
    let mut missing = valid.clone();
    missing["state"]["systems"]["kobolds"]["colony"]
        .as_object_mut()
        .unwrap()
        .remove("detour_goal");
    assert!(serde_json::from_value::<LoadedSaveEnvelope>(missing).is_err());
    let mut none = valid;
    none["state"]["systems"]["kobolds"]["colony"]["detour_goal"] = serde_json::Value::Null;
    let envelope: LoadedSaveEnvelope = serde_json::from_value(none).unwrap();
    assert!(envelope.into_state(&registries).is_err());
}

#[test]
fn scout_pursues_and_splits_an_overweight_cache_two_edges_away() {
    let (registries, mut state, spawn) = generated_opening();
    let mass = Mass::from_milligrams(2_000_000);
    let bulk = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        spawn,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        mass,
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap();
    let creation = validate_create_surface_resources(&registries, &state, [bulk]).unwrap();
    let pile = creation.first_resource_id();
    creation.commit(&mut state).unwrap();
    let den = colony(&state).route[0];
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    advance_to_tick(&registries, &mut state, 185);
    assert!(colony(&state).detour.is_some());
    assert_eq!(colony(&state).detour_goal, Some(spawn));
    advance_to_tick(&registries, &mut state, 205);
    assert_eq!(colony(&state).scout_position(), den);
    assert_eq!(
        state.surface().get(pile).unwrap().remaining_mass(),
        mass.checked_sub(Mass::from_milligrams(250_000)).unwrap()
    );
    assert_eq!(
        crate::kobolds::den_cache_space(&state, den),
        Mass::from_milligrams(250_000)
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
fn scout_diverts_into_a_traversable_side_branch_but_retreats_when_guarded() {
    let (registries, mut state, spawn) = generated_opening();
    let [den, junction, original_tip] = colony(&state).route;
    let side = state
        .logistics()
        .traversable_destinations(junction)
        .find(|candidate| *candidate != den && *candidate != original_tip)
        .expect("generated junction must connect an accessible forage approach");
    let drop = place_loose_stone_drop(&registries, &mut state, side);
    let total = calculate_matter_accounting(&state).unwrap().total();
    advance_to_tick(&registries, &mut state, 180);
    assert_eq!(colony(&state).scout_position(), junction);
    assert_eq!(colony(&state).route[2], original_tip);

    advance_to_tick(&registries, &mut state, 185);
    assert_eq!(colony(&state).scout_position(), side);
    assert_eq!(colony(&state).route[2], original_tip);
    assert_eq!(colony(&state).detour, Some(side));
    assert!(colony(&state).noticed_player_drop);
    assert_eq!(state.surface().get(drop).unwrap().position(), side);
    // The scout enters the branch first. An actor already guarding this
    // approach would now turn it back before entry.
    validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();

    let save = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&save).unwrap();
    let mut restored = loaded
        .into_state(&registries)
        .expect("detour route save must validate");
    for _ in 0..10 {
        let original = advance_tick(&registries, &mut state).unwrap();
        assert_eq!(original, advance_tick(&registries, &mut restored).unwrap());
        if state.tick().value() == 190 {
            assert!(
                original
                    .kobold_scout()
                    .is_some_and(|step| step.fled_player()),
                "the player at spawn can scare the scout off the side branch"
            );
            assert_eq!(state.surface().get(drop).unwrap().position(), side);
        }
    }
    assert_eq!(state, restored);
    assert_eq!(colony(&state).scout_position(), den);
    assert_eq!(colony(&state).scavenged_drop, None);
    assert_eq!(colony(&state).detour, None);
    assert_eq!(colony(&state).route[2], original_tip);
    assert_eq!(state.surface().get(drop).unwrap().position(), side);
    assert_eq!(
        state.surface().get(drop).unwrap().remaining_mass(),
        Mass::from_milligrams(250_000)
    );
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), total);
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}

#[test]
fn scout_resumes_authored_patrol_after_the_side_cache_is_recovered() {
    let (registries, mut state, spawn) = generated_opening();
    let [den, junction, tip] = colony(&state).route;
    let side = state
        .logistics()
        .traversable_destinations(junction)
        .find(|candidate| *candidate != den && *candidate != tip)
        .expect("the generated opening has an adjacent side branch");
    let parcel = place_loose_stone_drop(&registries, &mut state, side);
    let total = calculate_matter_accounting(&state).unwrap().total();
    advance_to_tick(&registries, &mut state, 185);
    assert_eq!(colony(&state).scout_position(), side);
    assert_eq!(colony(&state).detour, Some(side));
    let carried = validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();

    // Owner-level setup only: normal local pickup removes the bait. The player
    // must not need to edit a scout route to restore its ordinary behavior.
    crate::logistics::relocate_player_for_test(&mut state, side);
    let receipt = advance_control_tick(&registries, &mut state)
        .unwrap()
        .surface_drop_pickup()
        .expect("the side parcel is recoverable");
    assert_eq!(receipt.resource(), parcel);
    assert_eq!(receipt.picked_mass(), Mass::from_milligrams(250_000));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .stored_mass(),
        Mass::from_milligrams(250_000)
    );
    crate::logistics::relocate_player_for_test(&mut state, spawn);
    while !state.control_tick().is_simulation_boundary() {
        let _ = advance_control_tick(&registries, &mut state).unwrap();
    }

    advance_to_tick(&registries, &mut state, 195);
    assert_eq!(colony(&state).scout_position(), den);
    assert_eq!(colony(&state).detour, None);
    advance_to_tick(&registries, &mut state, 205);
    assert_eq!(
        colony(&state).scout_position(),
        den,
        "an intimidated scout should remain cautious at its den"
    );
    advance_to_tick(&registries, &mut state, 235);
    assert_eq!(
        colony(&state).scout_position(),
        tip,
        "after caution expires the scout resumes its authored patrol"
    );
    assert_eq!(colony(&state).detour, None);
    assert_eq!(colony(&state).route, [den, junction, tip]);
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), total);
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}

#[test]
fn intimidated_scout_waits_at_den_then_resumes_scavenging_without_losing_matter() {
    let (registries, mut state, spawn) = generated_opening();
    let [den, junction, tip] = colony(&state).route;
    let parcel = place_loose_stone_drop(&registries, &mut state, junction);
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    validate_admit_player(&registries, &state, tip)
        .unwrap()
        .commit(&mut state)
        .unwrap();

    advance_to_tick(&registries, &mut state, 185);
    assert_eq!(colony(&state).scout_position(), den);
    assert_eq!(colony(&state).wary_until, Some(SimulationTick::new(225)));
    assert!(
        available_nearby_kobold_scouts(&registries, &state)
            .next()
            .is_some_and(|sighting| sighting.is_wary())
    );
    // The player leaves the trail but the scout remembers the encounter.
    crate::logistics::relocate_player_for_test(&mut state, spawn);
    advance_to_tick(&registries, &mut state, 205);
    assert_eq!(colony(&state).scout_position(), den);
    assert_eq!(state.surface().get(parcel).unwrap().position(), junction);
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&encoded).unwrap();
    let mut restored = loaded
        .into_state(&registries)
        .expect("caution must persist");
    for _ in 0..25 {
        assert_eq!(
            advance_tick(&registries, &mut state).unwrap(),
            advance_tick(&registries, &mut restored).unwrap()
        );
    }
    assert_eq!(state, restored);
    assert_eq!(state.tick().value(), 230);
    assert_eq!(colony(&state).scout_position(), den);
    assert_eq!(
        state.surface().get(parcel).unwrap().position(),
        den,
        "once caution expires, unattended supplies become vulnerable again"
    );
    assert_eq!(
        state.surface().get(parcel).unwrap().remaining_mass(),
        Mass::from_milligrams(250_000)
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
fn invalid_kobold_caution_deadlines_are_rejected_on_load() {
    let (registries, mut state, _) = generated_opening();
    let tip = colony(&state).route[2];
    validate_admit_player(&registries, &state, tip)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    advance_to_tick(&registries, &mut state, 185);
    assert_eq!(colony(&state).wary_until, Some(SimulationTick::new(225)));
    let valid = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    for invalid in [180, 217, 10_000_000] {
        let mut corrupted = valid.clone();
        corrupted["state"]["systems"]["kobolds"]["colony"]["wary_until"] =
            serde_json::json!(invalid);
        let loaded: LoadedSaveEnvelope = serde_json::from_value(corrupted).unwrap();
        assert!(loaded.into_state(&registries).is_err());
    }
    let mut missing = valid;
    missing["state"]["systems"]["kobolds"]["colony"]
        .as_object_mut()
        .unwrap()
        .remove("wary_until");
    assert!(serde_json::from_value::<LoadedSaveEnvelope>(missing).is_err());
}

#[test]
fn invalid_temporary_scout_detours_are_rejected_on_load() {
    let (registries, mut state, _) = generated_opening();
    let [den, junction, original_tip] = colony(&state).route;
    let side = state
        .logistics()
        .traversable_destinations(junction)
        .find(|candidate| *candidate != den && *candidate != original_tip)
        .unwrap();
    place_loose_stone_drop(&registries, &mut state, side);
    advance_to_tick(&registries, &mut state, 185);
    assert_eq!(colony(&state).detour, Some(side));

    let valid = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    for illegal in [den, junction, original_tip, VoxelCoord::new(999, 2, 999)] {
        let mut altered = valid.clone();
        altered["state"]["systems"]["kobolds"]["colony"]["detour"] =
            serde_json::to_value(illegal).unwrap();
        let envelope: LoadedSaveEnvelope = serde_json::from_value(altered).unwrap();
        assert!(envelope.into_state(&registries).is_err());
    }
    let mut impossible = valid.clone();
    impossible["state"]["systems"]["kobolds"]["colony"]["scout_step"] = serde_json::json!(0);
    let envelope: LoadedSaveEnvelope = serde_json::from_value(impossible).unwrap();
    assert!(envelope.into_state(&registries).is_err());
    let mut missing = valid;
    missing["state"]["systems"]["kobolds"]["colony"]
        .as_object_mut()
        .unwrap()
        .remove("detour");
    assert!(serde_json::from_value::<LoadedSaveEnvelope>(missing).is_err());
}

#[test]
fn scout_diverts_to_split_a_bulk_side_branch_drop_and_returns_it_physically() {
    let (registries, mut state, _) = generated_opening();
    let [den, junction, tip] = colony(&state).route;
    let side = state
        .logistics()
        .traversable_destinations(junction)
        .find(|candidate| *candidate != den && *candidate != tip)
        .expect("opening junction has one alternative edge");
    let heavy = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        side,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        Mass::from_milligrams(2_000_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap();
    let creation = validate_create_surface_resources(&registries, &state, [heavy]).unwrap();
    let id = creation.first_resource_id();
    creation.commit(&mut state).unwrap();
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    advance_to_tick(&registries, &mut state, 185);
    assert_eq!(colony(&state).scout_position(), side);
    assert_eq!(colony(&state).detour, Some(side));
    assert_eq!(colony(&state).route[2], tip);
    advance_to_tick(&registries, &mut state, 190);
    let split = colony(&state)
        .scavenged_drop
        .expect("bulk split stays with the returning scout");
    assert_ne!(split, id);
    assert_eq!(colony(&state).scout_position(), junction);
    assert_eq!(
        state.surface().get(split).unwrap().remaining_mass(),
        Mass::from_milligrams(250_000)
    );
    assert_eq!(state.surface().get(split).unwrap().position(), junction);
    assert_eq!(
        state.surface().get(id).unwrap().remaining_mass(),
        Mass::from_milligrams(1_750_000)
    );
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&encoded).unwrap();
    let mut restored = loaded.into_state(&registries).unwrap();
    for _ in 0..5 {
        assert_eq!(
            advance_tick(&registries, &mut state).unwrap(),
            advance_tick(&registries, &mut restored).unwrap()
        );
    }
    assert_eq!(state, restored);
    assert_eq!(colony(&state).scout_position(), den);
    assert_eq!(colony(&state).scavenged_drop, None);
    assert_eq!(state.surface().get(split).unwrap().position(), den);
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
fn scout_splits_a_far_cache_that_exceeds_remaining_den_capacity() {
    let (registries, mut state, spawn) = generated_opening();
    let [den, junction, tip] = colony(&state).route;
    let halfway = state
        .logistics()
        .traversable_destinations(junction)
        .find(|step| {
            *step != den && *step != tip && state.logistics().has_traversal_edge(*step, spawn)
        })
        .expect("the starting camp is two route edges from the junction");
    let source = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        spawn,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        Mass::from_milligrams(2_000_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap();
    let den_fill = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        den,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        Mass::from_milligrams(450_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap();
    let creation =
        validate_create_surface_resources(&registries, &state, [source, den_fill]).unwrap();
    let source_id = creation.first_resource_id();
    creation.commit(&mut state).unwrap();
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    advance_to_tick(&registries, &mut state, 185);
    assert_eq!(colony(&state).scout_position(), halfway);
    assert_eq!(colony(&state).detour_goal, Some(spawn));
    advance_to_tick(&registries, &mut state, 190);
    assert_eq!(colony(&state).scout_position(), spawn);
    advance_to_tick(&registries, &mut state, 195);
    let split = colony(&state)
        .scavenged_drop
        .expect("far pile must split before returning");
    assert_eq!(state.surface().get(split).unwrap().position(), halfway);
    assert_eq!(
        state.surface().get(split).unwrap().remaining_mass(),
        Mass::from_milligrams(50_000)
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&saved).unwrap();
    let mut restored = loaded.into_state(&registries).unwrap();
    for _ in 0..10 {
        assert_eq!(
            advance_tick(&registries, &mut state).unwrap(),
            advance_tick(&registries, &mut restored).unwrap()
        );
    }
    assert_eq!(state, restored);
    assert_eq!(colony(&state).scout_position(), den);
    assert_eq!(state.surface().get(split).unwrap().position(), den);
    assert_eq!(
        state.surface().get(source_id).unwrap().remaining_mass(),
        Mass::from_milligrams(1_950_000)
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
fn kobold_scout_notices_exposed_drop_without_consuming_or_teleporting_matter() {
    let (registries, mut state, spawn) = generated_opening();
    let patrol_junction = colony(&state).route[1];
    let drop = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        patrol_junction,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        Mass::from_milligrams(250_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .expect("stone scrap should be representable");
    validate_create_surface_resources(&registries, &state, vec![drop])
        .expect("authoritative surface creation should admit a finite drop")
        .commit(&mut state)
        .expect("created surface matter should commit");
    validate_admit_player(&registries, &state, spawn)
        .expect("player should be admitted")
        .commit(&mut state)
        .expect("player admission should commit");
    for _ in 0..179 {
        let _ = advance_tick(&registries, &mut state).expect("scout prelude should advance");
    }
    let observed = advance_tick(&registries, &mut state)
        .expect("scout should visit the exposed drop")
        .kobold_scout()
        .expect("local movement should be reported");
    assert!(observed.discovered_player_drop());
    assert!(
        available_nearby_kobold_scouts(&registries, &state)
            .next()
            .unwrap()
            .has_noticed_player_drop()
    );
    let recovered = state
        .surface()
        .available_resources_at(Some(patrol_junction))
        .find(|resource| resource.kind() == SurfaceResourceKind::Drop)
        .expect("scout must not take or destroy the drop");
    assert_eq!(recovered.remaining_mass(), Mass::from_milligrams(250_000));

    let encoded = serde_json::to_string(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded: LoadedSaveEnvelope = serde_json::from_str(&encoded).unwrap();
    let mut restored = loaded
        .into_state(&registries)
        .expect("kobold patrol continuation should validate");
    assert_eq!(restored, state);
    for _ in 0..20 {
        let a = advance_tick(&registries, &mut state).expect("original should advance");
        let b = advance_tick(&registries, &mut restored).expect("replay should advance");
        assert_eq!(a.kobold_scout(), b.kobold_scout());
    }
    assert_eq!(state, restored);
}

#[test]
fn unattended_natural_food_is_finitely_foraged_into_a_recoverable_den_parcel() {
    let (registries, mut state, spawn) = generated_opening();
    let [den, junction, _] = colony(&state).route;
    let food = state
        .surface()
        .available_resources_at(Some(junction))
        .find(|source| {
            source.kind() == SurfaceResourceKind::Opportunity
                && source.commodity().form() == crate::content::FORM_FOOD
        })
        .expect("the ordinary opening contains forage at the junction");
    let id = food.id();
    let initial = food.remaining_mass();
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    advance_to_tick(&registries, &mut state, 194);
    assert_eq!(state.surface().get(id).unwrap().remaining_mass(), initial);
    // The scout has been visible at the fork, but the unattended catch moves out of
    // local sight as it reaches the den; it is not an omniscient player event.
    let event = advance_tick(&registries, &mut state).unwrap();
    assert!(event.kobold_scout().is_none());
    let batch = Mass::from_milligrams(100_000);
    assert_eq!(
        state.surface().get(id).unwrap().remaining_mass(),
        initial.checked_sub(batch).unwrap()
    );
    let forage_trace = colony(&state).tracks.last().unwrap();
    assert_eq!(forage_trace.position(), junction);
    assert_eq!(forage_trace.toward(), den);
    assert_eq!(
        forage_trace.observed_activity(),
        crate::kobolds::KoboldTrailKind::Foraging
    );
    let parcels = state
        .surface()
        .available_resources_at(Some(den))
        .filter(|record| record.kind() == SurfaceResourceKind::Drop)
        .collect::<Vec<_>>();
    assert_eq!(parcels.len(), 1);
    assert_eq!(parcels[0].remaining_mass(), batch);
    assert_eq!(
        parcels[0].commodity(),
        state.surface().get(id).unwrap().commodity()
    );
    assert_eq!(
        parcels[0].composition(),
        state.surface().get(id).unwrap().composition()
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
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&encoded).unwrap();
    let mut restored = loaded
        .into_state(&registries)
        .expect("foraged den parcel must load");
    for _ in 0..99 {
        assert_eq!(
            advance_tick(&registries, &mut state).unwrap(),
            advance_tick(&registries, &mut restored).unwrap()
        );
    }
    assert_eq!(state.tick().value(), 294);
    assert_eq!(
        state.surface().get(id).unwrap().remaining_mass(),
        initial.checked_sub(batch).unwrap(),
        "foraging must not strip the natural patch on every scout circuit"
    );
    assert_eq!(
        advance_tick(&registries, &mut state).unwrap(),
        advance_tick(&registries, &mut restored).unwrap()
    );
    assert_eq!(state, restored);
    assert_eq!(state.tick().value(), 295);
    assert_eq!(
        state.surface().get(id).unwrap().remaining_mass(),
        initial.checked_sub(Mass::from_milligrams(200_000)).unwrap()
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
}
