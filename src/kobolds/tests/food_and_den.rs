//! Wild foraging, den meals, and resident food custody contracts.

use super::*;

#[test]
fn scout_switches_to_neighboring_wild_food_and_carries_its_catch_home() {
    let (registries, mut state, _) = generated_opening();
    let den = colony(&state).route[0];
    let (junction, forage, berries) = exhaust_junction_nuts(&registries, &mut state);
    assert!(state.tick().value() < 180);
    let original = state.surface().get(berries).unwrap().clone();
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    advance_to_tick(&registries, &mut state, 185);
    assert_eq!(colony(&state).scout_position(), forage);
    assert_eq!(colony(&state).detour, Some(forage));
    advance_to_tick(&registries, &mut state, 190);
    assert_eq!(colony(&state).scout_position(), junction);
    let caught = colony(&state)
        .scavenged_drop
        .expect("side forage produces carried world matter");
    let catch = state.surface().get(caught).unwrap();
    assert_eq!(catch.position(), junction);
    assert_eq!(catch.remaining_mass(), Mass::from_milligrams(100_000));
    assert_eq!(catch.material_provenance(), original.material_provenance());
    assert_eq!(catch.composition(), original.composition());
    assert_eq!(
        catch.ambient_storage_age_parts_at(state.tick()),
        original.ambient_storage_age_parts_at(state.tick())
    );
    assert_eq!(
        state.surface().get(berries).unwrap().remaining_mass(),
        original
            .remaining_mass()
            .checked_sub(Mass::from_milligrams(100_000))
            .unwrap()
    );
    assert_eq!(
        colony(&state).next_forage_at,
        Some(SimulationTick::new(290))
    );

    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&saved).unwrap();
    let mut loaded = decoded
        .into_state(&registries)
        .expect("carried wild catch must persist");
    for _ in 0..5 {
        assert_eq!(
            advance_tick(&registries, &mut state).unwrap(),
            advance_tick(&registries, &mut loaded).unwrap()
        );
    }
    assert_eq!(state, loaded);
    assert_eq!(colony(&state).scout_position(), den);
    assert_eq!(colony(&state).scavenged_drop, None);
    assert_eq!(colony(&state).detour, None);
    assert_eq!(state.surface().get(caught).unwrap().position(), den);
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
fn side_foraging_stops_at_the_remaining_den_capacity() {
    let (registries, mut state, _) = generated_opening();
    let den = colony(&state).route[0];
    let (_, forage, berries) = exhaust_junction_nuts(&registries, &mut state);
    let before = state.surface().get(berries).unwrap().remaining_mass();
    let hoard = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        den,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        Mass::from_milligrams(450_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap();
    validate_create_surface_resources(&registries, &state, [hoard])
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    advance_to_tick(&registries, &mut state, 185);
    assert_eq!(colony(&state).scout_position(), forage);
    advance_to_tick(&registries, &mut state, 195);
    assert_eq!(
        state.surface().get(berries).unwrap().remaining_mass(),
        before.checked_sub(Mass::from_milligrams(50_000)).unwrap()
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
fn kobold_scouts_wait_then_patrol_and_only_report_near_player() {
    let (registries, mut state, spawn) = generated_opening();
    assert!(
        available_nearby_kobold_scouts(&registries, &state)
            .next()
            .is_none()
    );
    validate_admit_player(&registries, &state, spawn)
        .expect("ordinary player admission should work")
        .commit(&mut state)
        .expect("player admission should commit");
    assert!(
        available_nearby_kobold_scouts(&registries, &state)
            .next()
            .is_none()
    );
    for _ in 0..179 {
        let tick =
            advance_tick(&registries, &mut state).expect("opening simulation should advance");
        assert!(
            tick.kobold_scout().is_none(),
            "a den is not an arbitrary raid timer"
        );
    }
    let outcome = advance_tick(&registries, &mut state).expect("first scout patrol should advance");
    let event = outcome
        .kobold_scout()
        .expect("the scout arrives within local observation range");
    assert!(!event.discovered_player_drop());
    assert_eq!(
        available_nearby_kobold_scouts(&registries, &state)
            .next()
            .unwrap()
            .position(),
        event.position()
    );
    assert_eq!(validate_loaded_kobolds(&registries, &state), Ok(()));
}

#[test]
fn scout_den_caps_natural_foraging_without_destroying_unharvested_food() {
    let (registries, mut state, spawn) = generated_opening();
    let [den, junction, _] = colony(&state).route;
    let source = state
        .surface()
        .available_resources_at(Some(junction))
        .find(|resource| resource.commodity().form() == crate::content::FORM_FOOD)
        .expect("opening has finite food");
    let food = source.id();
    let before = source.remaining_mass();
    let total = calculate_matter_accounting(&state).unwrap().total();
    validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();

    advance_to_tick(&registries, &mut state, 595);
    let hoard = state
        .surface()
        .available_resources_at(Some(den))
        .filter(|resource| resource.kind() == SurfaceResourceKind::Drop)
        .map(|resource| resource.remaining_mass().milligrams())
        .sum::<u64>();
    assert!(hoard <= 500_000);
    let consumed = state
        .kobold_state()
        .consumed_matter()
        .map(|(_, mass)| mass.milligrams())
        .sum::<u128>();
    assert!(consumed > 0, "the resident scout must eat from its den");
    let extracted = before
        .checked_sub(state.surface().get(food).unwrap().remaining_mass())
        .unwrap();
    assert_eq!(
        u128::from(extracted.milligrams()),
        u128::from(hoard) + consumed
    );

    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&saved).unwrap();
    let mut loaded = decoded
        .into_state(&registries)
        .expect("full den cache must persist with its forage schedule");
    for _ in 0..400 {
        assert_eq!(
            advance_tick(&registries, &mut state).unwrap(),
            advance_tick(&registries, &mut loaded).unwrap()
        );
    }
    assert_eq!(state, loaded);
    let current = state.surface().get(food).unwrap().remaining_mass();
    assert!(
        current < before.checked_sub(extracted).unwrap(),
        "feeding must free den space for later, physically gathered food"
    );
    let hoard_after = state
        .surface()
        .available_resources_at(Some(den))
        .filter(|resource| resource.kind() == SurfaceResourceKind::Drop)
        .map(|resource| resource.remaining_mass().milligrams())
        .sum::<u64>();
    assert!(hoard_after <= 500_000);
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), total);
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}

#[test]
fn den_meals_transfer_only_fresh_local_food_into_conserved_consumption() {
    let (registries, mut state, _) = generated_opening();
    let den = colony(&state).route[0];
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    advance_to_tick(&registries, &mut state, 579);
    let source = state
        .surface()
        .available_resources_at(Some(den))
        .find(|resource| {
            resource.kind() == SurfaceResourceKind::Drop
                && resource.commodity().form() == crate::content::FORM_FOOD
        })
        .expect("scout must have foraged an edible den parcel before its meal");
    let food = source.id();
    let mass_before = source.remaining_mass();
    assert_eq!(colony(&state).next_meal_at, SimulationTick::new(580));
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap()
            .consumed()
            .milligrams(),
        0
    );

    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&saved).unwrap();
    let mut restored = loaded.into_state(&registries).unwrap();
    assert_eq!(
        advance_tick(&registries, &mut state).unwrap(),
        advance_tick(&registries, &mut restored).unwrap()
    );
    assert_eq!(state, restored);
    assert_eq!(colony(&state).next_meal_at, SimulationTick::new(980));
    assert_eq!(
        state.surface().get(food).unwrap().remaining_mass(),
        mass_before
            .checked_sub(Mass::from_milligrams(20_000))
            .unwrap()
    );
    assert_eq!(
        state
            .kobold_state()
            .consumed_matter()
            .map(|(_, mass)| mass.milligrams())
            .sum::<u128>(),
        20_000
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
    for _ in 0..400 {
        assert_eq!(
            advance_tick(&registries, &mut state).unwrap(),
            advance_tick(&registries, &mut restored).unwrap()
        );
    }
    assert_eq!(state, restored);
    assert_eq!(colony(&state).next_meal_at, SimulationTick::new(1380));
    assert_eq!(
        state
            .kobold_state()
            .consumed_matter()
            .map(|(_, mass)| mass.milligrams())
            .sum::<u128>(),
        40_000
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
}

#[test]
fn spoiled_den_food_is_discarded_on_a_real_edge_and_not_raided_again() {
    use crate::content::{FORM_FOOD, MATERIAL_BERRIES};
    use crate::inventory::{MaterialLotProfile, MaterialLotProvenance, STORAGE_AGE_PARTS_PER_TICK};
    use crate::logistics::add_traversable_edge_for_test;
    use crate::material::MaterialLotSpec;

    let registries = build_registries();
    let mut state = AppState::new();
    let den = VoxelCoord::new(-1, 0, 0);
    let junction = VoxelCoord::new(0, 0, 0);
    let tip = VoxelCoord::new(1, 0, 0);
    add_traversable_edge_for_test(&mut state, den, junction);
    add_traversable_edge_for_test(&mut state, junction, tip);
    state
        .kobold_state_mut()
        .apply_opening_colony([den, junction, tip]);
    advance_to_tick(&registries, &mut state, 579);
    assert_eq!(colony(&state).scout_position(), den);

    let commodity = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    let mass = Mass::from_milligrams(300_000);
    let profile = MaterialLotProfile::from_spec(
        &MaterialLotSpec::with_composition(
            commodity,
            mass,
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(MATERIAL_BERRIES),
        )
        .unwrap(),
    );
    let shelf = registries
        .survival()
        .get_food(commodity)
        .unwrap()
        .shelf_life();
    // A parcel accumulated age in prior storage before entering loose custody.
    let specification = SurfaceResourceSpec::from_existing_material(
        SurfaceResourceKind::Drop,
        den,
        mass,
        profile,
        MaterialLotProvenance::single(SimulationTick::ZERO),
        u128::from(shelf.value()) * STORAGE_AGE_PARTS_PER_TICK,
    )
    .unwrap();
    let creation = validate_create_surface_resources(&registries, &state, [specification]).unwrap();
    let resource = creation.first_resource_id();
    creation.commit(&mut state).unwrap();
    let initial = calculate_matter_accounting(&state).unwrap().total();
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );

    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&saved).unwrap();
    let mut loaded = decoded.into_state(&registries).unwrap();
    let mut cautious = loaded.clone();
    cautious
        .kobold_state_mut()
        .colony
        .as_mut()
        .unwrap()
        .wary_until = Some(SimulationTick::new(615));
    let _ = advance_tick(&registries, &mut cautious).unwrap();
    assert_eq!(cautious.surface().get(resource).unwrap().position(), den);
    assert_eq!(colony(&cautious).scout_position(), den);
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &cautious),
        Ok(())
    );
    for _ in 0..20 {
        assert_eq!(
            advance_tick(&registries, &mut state).unwrap(),
            advance_tick(&registries, &mut loaded).unwrap()
        );
    }
    assert_eq!(state, loaded);
    assert_eq!(state.surface().get(resource).unwrap().position(), junction);
    assert_eq!(
        state.surface().get(resource).unwrap().remaining_mass(),
        mass
    );
    assert_eq!(colony(&state).scout_position(), den);
    assert!(state.kobold_state().consumed_matter().next().is_none());
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        initial
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}

#[test]
fn nearby_player_can_observe_den_feeding_without_revealing_remote_inventory() {
    let (registries, mut state, _) = generated_opening();
    advance_to_tick(&registries, &mut state, 579);
    let [den, junction, _] = colony(&state).route;
    let available = state
        .surface()
        .available_resources_at(Some(den))
        .find(|resource| {
            resource.kind() == SurfaceResourceKind::Drop
                && resource.commodity().form() == crate::content::FORM_FOOD
        })
        .unwrap();
    let source = available.id();
    let original = available.remaining_mass();
    // The player can still see the two-voxel-away den without standing close
    // enough to stop its meal, unlike the former diagonally adjacent route tip.
    let observer = VoxelCoord::new(
        junction.x() + (junction.x() - den.x()),
        den.y(),
        junction.z() + (junction.z() - den.z()),
    );
    validate_admit_player(&registries, &state, observer)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert!(!crate::kobolds::patrol::living_player_deters_scout(
        &registries,
        &state,
        den
    ));
    let event = advance_tick(&registries, &mut state)
        .unwrap()
        .kobold_scout()
        .expect("the scout is visible beyond the den guarding radius");
    assert_eq!(
        event.consumed_food(),
        Some((source, Mass::from_milligrams(20_000)))
    );
    assert_eq!(
        state.surface().get(source).unwrap().remaining_mass(),
        original.checked_sub(Mass::from_milligrams(20_000)).unwrap()
    );
}

#[test]
fn player_guarding_the_den_prevents_a_meal_without_consuming_the_food() {
    let (registries, mut state, _) = generated_opening();
    advance_to_tick(&registries, &mut state, 579);
    let [den, junction, _] = colony(&state).route;
    let den_food = state
        .surface()
        .available_resources_at(Some(den))
        .find(|resource| resource.kind() == SurfaceResourceKind::Drop)
        .unwrap();
    let food = den_food.id();
    let mass = den_food.remaining_mass();
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    validate_admit_player(&registries, &state, junction)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let _ = advance_tick(&registries, &mut state).unwrap();
    assert_eq!(state.surface().get(food).unwrap().remaining_mass(), mass);
    assert!(state.kobold_state().consumed_matter().next().is_none());
    assert_eq!(colony(&state).next_meal_at, SimulationTick::new(980));
    assert_eq!(colony(&state).scout_position(), den);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
}

#[test]
fn invalid_kobold_meal_schedule_and_consumption_ledger_are_rejected_on_load() {
    let (registries, mut state, _) = generated_opening();
    advance_to_tick(&registries, &mut state, 580);
    let valid = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    for due in [0_u64, 580, 581, 800] {
        let mut tampered = valid.clone();
        tampered["state"]["systems"]["kobolds"]["colony"]["next_meal_at"] = serde_json::json!(due);
        let decoded: LoadedSaveEnvelope = serde_json::from_value(tampered).unwrap();
        assert!(decoded.into_state(&registries).is_err());
    }
    let mut missing = valid.clone();
    missing["state"]["systems"]["kobolds"]["colony"]
        .as_object_mut()
        .unwrap()
        .remove("next_meal_at");
    assert!(serde_json::from_value::<LoadedSaveEnvelope>(missing).is_err());

    let mut corrupt_ledger = valid;
    let ledger = corrupt_ledger["state"]["systems"]["kobolds"]["consumed_matter"]
        .as_object_mut()
        .unwrap();
    assert!(!ledger.is_empty());
    *ledger.values_mut().next().unwrap() = serde_json::json!(0_u64);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(corrupt_ledger).unwrap();
    assert!(decoded.into_state(&registries).is_err());
}

#[test]
fn limited_den_space_only_forages_the_mass_that_fits() {
    let (registries, mut state, spawn) = generated_opening();
    let [den, junction, _] = colony(&state).route;
    let source = state
        .surface()
        .available_resources_at(Some(junction))
        .find(|resource| resource.commodity().form() == crate::content::FORM_FOOD)
        .expect("opening has finite food");
    let food = source.id();
    let source_before = source.remaining_mass();
    let hoard_spec = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        den,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        Mass::from_milligrams(450_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap();
    validate_create_surface_resources(&registries, &state, [hoard_spec])
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let total = calculate_matter_accounting(&state).unwrap().total();
    validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    advance_to_tick(&registries, &mut state, 195);
    let remaining = state.surface().get(food).unwrap().remaining_mass();
    assert_eq!(
        remaining,
        source_before
            .checked_sub(Mass::from_milligrams(50_000))
            .unwrap()
    );
    let cache = state
        .surface()
        .available_resources_at(Some(den))
        .filter(|item| item.kind() == SurfaceResourceKind::Drop)
        .map(|item| item.remaining_mass().milligrams())
        .sum::<u64>();
    assert_eq!(cache, 500_000);
    advance_to_tick(&registries, &mut state, 1005);
    assert!(
        state.surface().get(food).unwrap().remaining_mass() < remaining,
        "den meals must reopen limited staging space for later forage"
    );
    assert_eq!(
        state
            .kobold_state()
            .consumed_matter()
            .map(|(_, mass)| mass.milligrams())
            .sum::<u128>(),
        40_000
    );
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), total);
}

#[test]
fn scout_abandons_cargo_when_den_fills_mid_retreat() {
    let (registries, mut state, _) = generated_opening();
    let [den, junction, tip] = colony(&state).route;
    let cargo = place_loose_stone_drop(&registries, &mut state, tip);
    advance_to_tick(&registries, &mut state, 190);
    assert_eq!(colony(&state).scavenged_drop, Some(cargo));
    assert_eq!(state.surface().get(cargo).unwrap().position(), junction);
    let blocking_spec = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        den,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        Mass::from_milligrams(500_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap();
    validate_create_surface_resources(&registries, &state, [blocking_spec])
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let total = calculate_matter_accounting(&state).unwrap().total();
    advance_to_tick(&registries, &mut state, 195);
    assert_eq!(colony(&state).scout_position(), den);
    assert_eq!(colony(&state).scavenged_drop, None);
    assert_eq!(state.surface().get(cargo).unwrap().position(), junction);
    assert_eq!(
        state.surface().get(cargo).unwrap().remaining_mass(),
        Mass::from_milligrams(250_000)
    );
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), total);
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}

#[test]
fn impossible_scavenger_foraging_schedule_is_rejected_on_load() {
    let (registries, mut state, _) = generated_opening();
    advance_to_tick(&registries, &mut state, 195);
    let mut value = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    value["state"]["systems"]["kobolds"]["colony"]["next_forage_at"] =
        serde_json::json!(999_999_u64);
    let loaded: LoadedSaveEnvelope = serde_json::from_value(value).unwrap();
    assert!(loaded.into_state(&registries).is_err());
}

#[test]
fn guarding_the_natural_food_prevents_the_scout_from_foraging() {
    let (registries, mut state, _) = generated_opening();
    let junction = colony(&state).route[1];
    let food = state
        .surface()
        .available_resources_at(Some(junction))
        .find(|source| source.commodity().form() == crate::content::FORM_FOOD)
        .unwrap();
    let id = food.id();
    let initial = food.remaining_mass();
    validate_admit_player(&registries, &state, junction)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    advance_to_tick(&registries, &mut state, 210);
    assert_eq!(state.surface().get(id).unwrap().remaining_mass(), initial);
    assert!(
        !state
            .surface()
            .resources()
            .any(|record| record.kind() == SurfaceResourceKind::Drop)
    );
}
