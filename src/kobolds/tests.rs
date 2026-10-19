//! Kobold opening patrol, physical obstruction, local observation, and continuation contracts.

use crate::content::{FORM_CHIP, MATERIAL_STONE, build_registries};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::AppState;
use crate::material::{CommodityKey, MaterialComposition};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::{advance_control_tick, advance_tick};
use crate::spatial::VoxelCoord;
use crate::surface::{SurfaceResourceKind, SurfaceResourceSpec, validate_create_surface_resources};
use crate::world::WorldGenerationSeed;
use crate::world_generation::validate_generate_initial_wilderness;

use super::{KoboldColony, available_nearby_kobold_scouts, validate_loaded_kobolds};

fn colony(state: &AppState) -> &KoboldColony {
    state
        .kobold_state()
        .colony
        .as_ref()
        .unwrap_or_else(|| panic!("opening patrol missing"))
}

fn generated_opening() -> (crate::Registries, AppState, VoxelCoord) {
    let registries = build_registries();
    let mut state = AppState::new();
    let spawn = VoxelCoord::new(8, 2, 8);
    validate_generate_initial_wilderness(
        &registries,
        &state,
        WorldGenerationSeed::new(0x4B4F_424F_4C44),
        spawn,
    )
    .expect("opening colony should be admitted with wilderness")
    .commit(&mut state)
    .expect("opening colony should commit atomically");
    (registries, state, spawn)
}

#[test]
fn kobold_scouts_wait_then_patrol_and_only_report_near_player() {
    let (registries, mut state, spawn) = generated_opening();
    assert!(available_nearby_kobold_scouts(&state).next().is_none());
    validate_admit_player(&registries, &state, spawn)
        .expect("ordinary player admission should work")
        .commit(&mut state)
        .expect("player admission should commit");
    assert!(available_nearby_kobold_scouts(&state).next().is_none());
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
        available_nearby_kobold_scouts(&state)
            .next()
            .unwrap()
            .position(),
        event.position()
    );
    assert_eq!(validate_loaded_kobolds(&state), Ok(()));
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
        available_nearby_kobold_scouts(&state)
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

fn place_loose_stone_drop(
    registries: &crate::Registries,
    state: &mut AppState,
    position: VoxelCoord,
) -> crate::surface::SurfaceResourceId {
    let spec = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        position,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        Mass::from_milligrams(250_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .expect("stone chips form finite loose material");
    let plan = validate_create_surface_resources(registries, state, vec![spec])
        .expect("stone chips should have valid surface custody");
    let id = plan.first_resource_id();
    plan.commit(state).expect("stone chips should commit");
    id
}

fn advance_to_tick(registries: &crate::Registries, state: &mut AppState, tick: u64) {
    while state.tick().value() < tick {
        let _ = advance_tick(registries, state).expect("kobold encounter slow tick failed");
    }
    assert_eq!(state.tick().value(), tick);
}

#[test]
fn scout_takes_one_portable_parcel_but_cannot_drag_a_heavy_loose_pile() {
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
fn unliftable_material_at_junction_does_not_fake_a_scavenging_detour() {
    let (registries, mut state, spawn) = generated_opening();
    let junction = colony(&state).route[1];
    let heavy_spec = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        junction,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        Mass::from_milligrams(2_000_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap();
    validate_create_surface_resources(&registries, &state, [heavy_spec])
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    advance_to_tick(&registries, &mut state, 185);
    assert_eq!(colony(&state).scout_position(), colony(&state).route[2]);
    assert!(!colony(&state).noticed_player_drop);
    assert_eq!(colony(&state).scavenged_drop, None);
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
        available_nearby_kobold_scouts(&state)
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
    while !state.control_tick().is_simulation_boundary() {
        let _ = advance_control_tick(&registries, &mut state).unwrap();
    }
    advance_to_tick(&registries, &mut state, 195);
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

#[test]
fn approaching_a_loaded_scout_forces_it_to_abandon_recoverable_material() {
    let (registries, mut state, _) = generated_opening();
    let [den, junction, tip] = colony(&state).route;
    let drop = place_loose_stone_drop(&registries, &mut state, tip);
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    advance_to_tick(&registries, &mut state, 190);
    assert_eq!(colony(&state).scavenged_drop, Some(drop));
    assert_eq!(state.surface().get(drop).unwrap().position(), junction);

    validate_admit_player(&registries, &state, tip)
        .expect("player can intercept adjacent to the carrying scout")
        .commit(&mut state)
        .expect("interception admission failed");
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&encoded).unwrap();
    let mut replay = loaded
        .into_state(&registries)
        .expect("carrying scout saved");
    for _ in 0..5 {
        let a = advance_tick(&registries, &mut state).unwrap();
        let b = advance_tick(&registries, &mut replay).unwrap();
        assert_eq!(a.kobold_scout(), b.kobold_scout());
        if state.tick().value() == 195 {
            let event = a.kobold_scout().expect("retreat is locally observable");
            assert!(event.fled_player());
            assert_eq!(event.abandoned_drop(), Some(drop));
            assert_eq!(event.relocated_drop(), None);
        }
    }
    assert_eq!(state, replay);
    assert_eq!(colony(&state).scout_position(), den);
    assert_eq!(colony(&state).scavenged_drop, None);
    assert_eq!(state.surface().get(drop).unwrap().position(), junction);
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
fn blocked_flight_releases_material_without_moving_or_multiplying_it() {
    let (registries, mut state, _) = generated_opening();
    let [den, junction, tip] = colony(&state).route;
    let drop = place_loose_stone_drop(&registries, &mut state, tip);
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    advance_to_tick(&registries, &mut state, 190);
    assert_eq!(colony(&state).scavenged_drop, Some(drop));
    validate_admit_player(&registries, &state, den)
        .expect("player can guard the only retreat cell")
        .commit(&mut state)
        .expect("guard must enter authoritative world");
    advance_to_tick(&registries, &mut state, 194);
    let event = advance_tick(&registries, &mut state)
        .expect("blocked retreat must still complete slow tick")
        .kobold_scout()
        .expect("abandoned cargo should be observable near the player");
    assert!(
        !event.fled_player(),
        "no false movement through the occupied den"
    );
    assert_eq!(event.abandoned_drop(), Some(drop));
    assert_eq!(event.relocated_drop(), None);
    assert_eq!(event.position(), junction);
    assert_eq!(colony(&state).scout_position(), junction);
    assert_eq!(colony(&state).scavenged_drop, None);
    assert_eq!(state.surface().get(drop).unwrap().position(), junction);
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
fn tampered_scavenged_parcel_identity_is_rejected_on_load() {
    let (registries, mut state, _) = generated_opening();
    let tip = colony(&state).route[2];
    let drop = place_loose_stone_drop(&registries, &mut state, tip);
    advance_to_tick(&registries, &mut state, 190);
    assert_eq!(colony(&state).scavenged_drop, Some(drop));
    let mut tampered = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    tampered["state"]["systems"]["kobolds"]["colony"]["scavenged_drop"] = serde_json::json!(999999);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(tampered).unwrap();
    assert!(decoded.into_state(&registries).is_err());
}

#[test]
fn terrain_scout_waits_for_floor_and_head_clearance_and_resumes_after_repair() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::core::state::validate_loaded_state;
    use crate::world::{
        BlockStateId, ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, set_generated_block_for_test,
        validate_generate_chunk,
    };

    let seed = WorldGenerationSeed::new(0x5749_4C44_4552_4E53);
    for block_floor in [true, false] {
        let registries = build_registries();
        let mut state = AppState::new();
        let horizontal = VoxelCoord::new(8, 0, 8);
        validate_generate_chunk(
            &registries,
            &state,
            seed,
            ChunkCoord::containing(horizontal),
            BLOCK_STATE_STONE,
        )
        .unwrap_or_else(|error| panic!("patrol terrain generation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("patrol terrain commit failed: {error}"));
        let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
            .rev()
            .find(|&y| {
                state
                    .world()
                    .block_at(VoxelCoord::new(horizontal.x(), y, horizontal.z()))
                    == Some(BLOCK_STATE_STONE)
            })
            .unwrap_or_else(|| panic!("patrol terrain surface missing"));
        let spawn = VoxelCoord::new(horizontal.x(), surface_y + 1, horizontal.z());
        validate_generate_initial_wilderness(&registries, &state, seed, spawn)
            .unwrap_or_else(|error| panic!("terrain patrol opening rejected: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("terrain patrol opening commit failed: {error}"));
        validate_admit_player(&registries, &state, spawn)
            .unwrap_or_else(|error| panic!("terrain patrol player admission failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("terrain patrol player commit failed: {error}"));

        let [den, junction, _] = colony(&state).route;
        let blocked_y = if block_floor {
            junction.y() - 1
        } else {
            junction.y() + 1
        };
        let blocked = VoxelCoord::new(junction.x(), blocked_y, junction.z());
        let original = state
            .world()
            .block_at(blocked)
            .unwrap_or_else(|| panic!("patrol obstacle coordinate not generated"));
        let obstruction = if block_floor {
            BlockStateId::AIR
        } else {
            BLOCK_STATE_STONE
        };
        assert_ne!(original, obstruction);
        set_generated_block_for_test(registries.world(), &mut state, blocked, obstruction);
        assert!(
            !state
                .world()
                .has_standing_clearance(registries.world(), junction)
        );

        for _ in 0..179 {
            let _ = advance_tick(&registries, &mut state)
                .unwrap_or_else(|error| panic!("obstructed patrol prelude failed: {error}"));
        }
        let blocked_step = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("obstructed patrol advance failed: {error}"));
        assert_eq!(blocked_step.kobold_scout(), None);
        assert_eq!(colony(&state).scout_position(), den);
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

        let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
            .unwrap_or_else(|error| panic!("blocked patrol save failed: {error}"));
        let envelope: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
            .unwrap_or_else(|error| panic!("blocked patrol decode failed: {error}"));
        let mut loaded = envelope
            .into_state(&registries)
            .unwrap_or_else(|error| panic!("blocked patrol load failed: {error}"));
        assert_eq!(loaded, state);

        set_generated_block_for_test(registries.world(), &mut state, blocked, original);
        set_generated_block_for_test(registries.world(), &mut loaded, blocked, original);
        assert!(
            state
                .world()
                .has_standing_clearance(registries.world(), junction)
        );
        for _ in 0..5 {
            let original_step = advance_tick(&registries, &mut state)
                .unwrap_or_else(|error| panic!("repaired patrol advance failed: {error}"));
            let loaded_step = advance_tick(&registries, &mut loaded)
                .unwrap_or_else(|error| panic!("loaded repaired patrol advance failed: {error}"));
            assert_eq!(original_step.kobold_scout(), loaded_step.kobold_scout());
        }
        assert_eq!(colony(&state).scout_position(), junction);
        assert_eq!(loaded, state);
        assert_eq!(validate_loaded_state(&registries, &loaded), Ok(()));
    }
}

#[test]
fn kobold_invalid_colony_route_is_rejected_from_trusted_save() {
    let (registries, state, _) = generated_opening();
    let mut tampered = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    tampered["state"]["systems"]["kobolds"]["colony"]["scout_step"] = serde_json::json!(99);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(tampered).unwrap();
    assert!(decoded.into_state(&registries).is_err());
}

#[test]
fn kobold_invalid_patrol_schedule_is_rejected_from_trusted_save() {
    let (registries, state, _) = generated_opening();
    for due in [181_u64, 200_u64] {
        let mut tampered = serde_json::to_value(SaveEnvelope::new(&registries, &state))
            .unwrap_or_else(|error| panic!("patrol schedule fixture failed: {error}"));
        tampered["state"]["systems"]["kobolds"]["colony"]["next_step_at"] = serde_json::json!(due);
        let decoded: LoadedSaveEnvelope = serde_json::from_value(tampered)
            .unwrap_or_else(|error| panic!("patrol schedule fixture decode failed: {error}"));
        assert!(decoded.into_state(&registries).is_err());
    }
}

#[test]
fn kobold_scout_does_not_pass_through_the_player() {
    let (registries, mut state, _) = generated_opening();
    let [den, junction, _] = colony(&state).route;
    validate_admit_player(&registries, &state, junction)
        .expect("player should be admitted at the contested junction")
        .commit(&mut state)
        .expect("junction admission should commit");
    for _ in 0..179 {
        let _ = advance_tick(&registries, &mut state).expect("quiet opening should advance");
    }
    let outcome = advance_tick(&registries, &mut state).expect("blocked patrol should advance");
    assert!(outcome.kobold_scout().is_none());
    assert_eq!(colony(&state).scout_position(), den);
    assert_eq!(validate_loaded_kobolds(&state), Ok(()));
}
