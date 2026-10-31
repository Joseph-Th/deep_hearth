//! Real sparse forage becomes an uncertain, wind-dependent clue before eyesight resolves it.

use super::*;

use crate::content::{BLOCK_STATE_STONE, FORM_FOOD, build_registries};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{AppState, apply_clock_advance, validate_loaded_state};
use crate::core::time::SimulationTick;
use crate::material::{CommodityKey, MaterialComposition};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::advance_tick;
use crate::surface::{
    AdjacentSurfaceResource, SurfaceResourceSpec, player_surface_gathering_options,
    validate_create_surface_resources, validate_start_player_surface_gathering_option,
};
use crate::world::{
    BlockStateId, ChunkCoord, WorldGenerationSeed, assess_player_weather,
    set_generated_block_for_test, validate_generate_chunk,
};

fn single_real_berry_patch() -> (Registries, AppState, VoxelCoord) {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0x5343_454E_545F_4652),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let site = (5..11)
        .flat_map(|x| (5..11).map(move |z| (x, z)))
        .find_map(|(x, z)| {
            let floor_y = (-10..=5).rev().find(|&y| {
                state.world().block_at(VoxelCoord::new(x, y, z)) == Some(BLOCK_STATE_STONE)
            })?;
            let position = VoxelCoord::new(x, floor_y + 1, z);
            (state.world().block_at(position) == Some(BlockStateId::AIR)
                && state.world().block_at(VoxelCoord::new(x, floor_y + 2, z))
                    == Some(BlockStateId::AIR)
                && state.world().tree_trunk_at(position).is_none()
                && state
                    .world()
                    .tree_trunk_at(VoxelCoord::new(x, floor_y + 2, z))
                    .is_none())
            .then_some(position)
        })
        .expect("standard stone terrain has a clear rooted food location");
    validate_admit_player(&registries, &state, site)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let spec = SurfaceResourceSpec::new(
        SurfaceResourceKind::Opportunity,
        site,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        Mass::from_milligrams(300_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_BERRIES),
    )
    .unwrap();
    validate_create_surface_resources(&registries, &state, [spec])
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    (registries, state, site)
}

/// No resource coordinate is supplied to the player. The test controls weather
/// and location to establish a repeatable *witness* of a real windborne clue.
fn try_find_downwind_witness(
    registries: &Registries,
    original: &AppState,
    site: VoxelCoord,
    kind: ForageScentKind,
) -> Option<(AppState, VoxelCoord)> {
    for front in 0..30 {
        let mut state = original.clone();
        apply_clock_advance(&mut state, SimulationTick::new(front * 200));
        let Some(direction) =
            assess_player_weather(registries, &state).and_then(|weather| weather.wind_direction())
        else {
            continue;
        };
        let (dx, dz) = direction.delta();
        for range in 3..=6_i64 {
            let x = site.x() + dx * range;
            let z = site.z() + dz * range;
            if state.world().block_at(VoxelCoord::new(x, 0, z)).is_none() {
                continue;
            }
            let Some(y) = (-10..=5).rev().find(|&y| {
                state.world().block_at(VoxelCoord::new(x, y, z)) != Some(BlockStateId::AIR)
            }) else {
                continue;
            };
            let observer = VoxelCoord::new(x, y + 1, z);
            if state.world().block_at(observer) != Some(BlockStateId::AIR)
                || state.world().tree_trunk_at(observer).is_some()
            {
                continue;
            }
            crate::logistics::relocate_player_for_test(&mut state, observer);
            if assess_player_forage_scents(registries, &state)
                .iter()
                .any(|cue| cue.kind() == kind)
            {
                return Some((state, observer));
            }
        }
    }
    None
}

fn find_downwind_witness(
    registries: &Registries,
    original: &AppState,
    site: VoxelCoord,
) -> (AppState, VoxelCoord) {
    try_find_downwind_witness(registries, original, site, ForageScentKind::SweetFruit)
        .expect("seeded wind, generated air and finite berry patch provide a real sensory trail")
}

#[test]
fn a_real_patch_is_detectable_downwind_before_sight_but_not_upwind_or_through_rock() {
    let (registries, state, site) = single_real_berry_patch();
    let (downwind, observer) = find_downwind_witness(&registries, &state, site);
    let before = downwind.clone();
    let smell = assess_player_forage_scents(&registries, &downwind);
    assert_eq!(smell.len(), 1);
    assert_eq!(smell[0].kind(), ForageScentKind::SweetFruit);
    assert!(
        AdjacentSurfaceResource::visible_in_generated_world(&registries, &downwind)
            .all(|sighting| sighting.position() != site),
        "the source is not inside two-cell visual reach when its scent becomes apparent"
    );
    assert_eq!(before, downwind, "scent observation is read-only");
    let (wx, wz) = assess_player_weather(&registries, &downwind)
        .unwrap()
        .wind_direction()
        .unwrap()
        .delta();
    let span = observer
        .x()
        .abs_diff(site.x())
        .max(observer.z().abs_diff(site.z())) as i64;
    let upwind = VoxelCoord::new(site.x() - wx * span, site.y(), site.z() - wz * span);
    let mut reversed = downwind.clone();
    crate::logistics::relocate_player_for_test(&mut reversed, upwind);
    assert!(
        assess_player_forage_scents(&registries, &reversed).is_empty(),
        "airflow away from the player cannot reveal remote food"
    );

    // Enclose the *source* on every open face. Air cannot teleport through
    // a physical stone barrier even when the player is in the downwind fan.
    let mut walled = downwind.clone();
    for (dx, dy, dz) in [(1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1), (0, 1, 0)] {
        let voxel = VoxelCoord::new(site.x() + dx, site.y() + dy, site.z() + dz);
        if walled.world().block_at(voxel) == Some(BlockStateId::AIR) {
            set_generated_block_for_test(registries.world(), &mut walled, voxel, BLOCK_STATE_STONE);
        }
    }
    assert!(assess_player_forage_scents(&registries, &walled).is_empty());
    assert_eq!(validate_loaded_state(&registries, &downwind), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &downwind)).unwrap();
    let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(restored, downwind);
    assert_eq!(assess_player_forage_scents(&registries, &restored), smell);
}

#[test]
fn scent_fades_with_true_source_depletion_and_does_not_leak_spoiled_food() {
    let (registries, state, site) = single_real_berry_patch();
    let (mut witness, observer) = find_downwind_witness(&registries, &state, site);
    let old_cues = assess_player_forage_scents(&registries, &witness);
    assert_eq!(old_cues.len(), 1);
    let original_total = calculate_matter_accounting(&witness).unwrap().total();
    let spoiled_at = SimulationTick::new(registries.core().calendar().ticks_per_day() * 5);
    let mut spoiled = witness.clone();
    apply_clock_advance(&mut spoiled, spoiled_at);
    assert!(
        assess_player_forage_scents(&registries, &spoiled).is_empty(),
        "five ambient days must spoil four-day berries without mysteriously refreshing their scent"
    );

    crate::logistics::relocate_player_for_test(&mut witness, site);
    let source_id = witness
        .surface()
        .available_resources_at(Some(site))
        .next()
        .unwrap()
        .id();
    let batch = Mass::from_milligrams(100_000);
    let option = player_surface_gathering_options(&registries, &witness)
        .unwrap()
        .into_iter()
        .find(|option| option.resource() == source_id)
        .unwrap();
    validate_start_player_surface_gathering_option(&registries, &witness, option, batch)
        .unwrap()
        .commit(&mut witness)
        .unwrap();
    for _ in 0..300 {
        if witness.player_work().active().is_none() {
            break;
        }
        let _ = advance_tick(&registries, &mut witness).unwrap();
    }
    assert!(witness.player_work().active().is_none());
    assert_eq!(
        witness.surface().get(source_id).unwrap().remaining_mass(),
        Mass::from_milligrams(200_000)
    );
    assert_eq!(
        calculate_matter_accounting(&witness).unwrap().total(),
        original_total
    );
    crate::logistics::relocate_player_for_test(&mut witness, observer);
    let reduced = assess_player_forage_scents(&registries, &witness);
    assert!(
        reduced
            .first()
            .is_some_and(|cue| cue.intensity() <= old_cues[0].intensity()),
        "removing most fragrant biomass cannot strengthen a scent"
    );
    assert_eq!(reduced[0].intensity(), ForageScentIntensity::Faint);
    crate::logistics::relocate_player_for_test(&mut witness, site);
    for _ in 0..2 {
        let option = player_surface_gathering_options(&registries, &witness)
            .unwrap()
            .into_iter()
            .find(|option| option.resource() == source_id)
            .unwrap();
        validate_start_player_surface_gathering_option(&registries, &witness, option, batch)
            .unwrap()
            .commit(&mut witness)
            .unwrap();
        for _ in 0..300 {
            if witness.player_work().active().is_none() {
                break;
            }
            let _ = advance_tick(&registries, &mut witness).unwrap();
        }
        assert!(witness.player_work().active().is_none());
    }
    assert!(
        witness
            .surface()
            .get(source_id)
            .unwrap()
            .remaining_mass()
            .is_zero()
    );
    crate::logistics::relocate_player_for_test(&mut witness, observer);
    assert!(assess_player_forage_scents(&registries, &witness).is_empty());
    assert_eq!(
        calculate_matter_accounting(&witness).unwrap().total(),
        original_total
    );
    assert_eq!(validate_loaded_state(&registries, &witness), Ok(()));
}

#[test]
fn a_new_wind_front_reorients_a_real_scent_without_moving_or_resetting_food() {
    let (registries, original, site) = single_real_berry_patch();
    let (initial, _) = find_downwind_witness(&registries, &original, site);
    let first_direction = assess_player_weather(&registries, &initial)
        .unwrap()
        .wind_direction();
    assert_eq!(assess_player_forage_scents(&registries, &initial).len(), 1);
    let original_source = initial
        .surface()
        .available_resources_at(Some(site))
        .next()
        .unwrap()
        .remaining_mass();
    let mut saw_shift = false;
    for later_front in initial.tick().value() / 200 + 1..initial.tick().value() / 200 + 40 {
        let mut later = initial.clone();
        apply_clock_advance(&mut later, SimulationTick::new(later_front * 200));
        let condition = assess_player_weather(&registries, &later).unwrap();
        if condition.wind_direction() == first_direction {
            continue;
        }
        if assess_player_forage_scents(&registries, &later).is_empty() {
            assert_eq!(
                later
                    .surface()
                    .available_resources_at(Some(site))
                    .next()
                    .unwrap()
                    .remaining_mass(),
                original_source,
                "a wind change cannot harvest or erase the underlying patch"
            );
            assert_eq!(validate_loaded_state(&registries, &later), Ok(()));
            saw_shift = true;
            break;
        }
    }
    assert!(
        saw_shift,
        "seeded shifting winds must change the location of the sensory plume"
    );
}

/// Walk one *horizontal* face-adjacent cell using the authoritative body
/// controller. No test fixture relocation occurs after the actual expedition
/// starts, and neutral control settles the body's inertia and metabolic cost.
fn walk_physical_for_scent(
    registries: &Registries,
    state: &mut AppState,
    destination: VoxelCoord,
) -> u64 {
    use crate::logistics::{
        CONTROL_INPUT_PARTS_PER_MILLION, HorizontalControlInput, PlayerControlInput,
        project_player_adjacent_walk,
    };
    use crate::simulation::{advance_control_tick, advance_control_tick_with_input};
    use crate::spatial::HorizontalVelocity;

    let from = state.logistics().player().unwrap().position();
    assert_eq!(from.y(), destination.y());
    assert!(from.is_face_adjacent_to(destination));
    let projected = project_player_adjacent_walk(registries, state, destination).unwrap();
    let x = i32::try_from(destination.x() - from.x()).unwrap() * CONTROL_INPUT_PARTS_PER_MILLION;
    let z = i32::try_from(destination.z() - from.z()).unwrap() * CONTROL_INPUT_PARTS_PER_MILLION;
    let movement = PlayerControlInput::new(HorizontalControlInput::new(x, z).unwrap());
    let mut controls = 0;
    while state.logistics().player().unwrap().position() != destination {
        assert!(controls < 80, "physical scent pursuit cannot stall");
        let _ = advance_control_tick_with_input(registries, state, movement).unwrap();
        controls += 1;
    }
    while state
        .logistics()
        .player()
        .unwrap()
        .body()
        .horizontal_velocity()
        != HorizontalVelocity::ZERO
    {
        assert!(controls < 100, "physical scent pursuit must brake safely");
        let _ = advance_control_tick(registries, state).unwrap();
        controls += 1;
        assert_eq!(state.logistics().player().unwrap().position(), destination);
    }
    assert_eq!(controls, projected.control_ticks());
    while !state.control_tick().is_simulation_boundary() {
        let _ = advance_control_tick(registries, state).unwrap();
        assert_eq!(state.logistics().player().unwrap().position(), destination);
    }
    controls
}

/// A generated forage patch becomes a player-controlled pursuit instead of
/// an ID oracle: smell first, walk upwind, see the resource, approach, gather,
/// and return real matter to carried custody. Only initial fixture placement
/// may consult the hidden world; the actor's subsequent steps use its senses.
#[test]
fn generated_wilderness_scent_leads_to_real_physical_walk_sighting_and_timed_food() {
    use crate::logistics::project_player_adjacent_walk;
    use crate::world::validate_generate_player_neighborhood;
    let registries = build_registries();
    let mut candidate = None;
    'seeds: for seed_value in 1..=25 {
        let seed = WorldGenerationSeed::new(seed_value);
        let mut world = AppState::new();
        validate_generate_chunk(
            &registries,
            &world,
            seed,
            ChunkCoord::new(0, 0),
            BLOCK_STATE_STONE,
        )
        .unwrap()
        .commit(&mut world)
        .unwrap();
        let origin = VoxelCoord::new(
            0,
            super::super::generation::terrain_height(seed, 0, 0) + 1,
            0,
        );
        validate_admit_player(&registries, &world, origin)
            .unwrap()
            .commit(&mut world)
            .unwrap();
        validate_generate_player_neighborhood(&registries, &world)
            .unwrap()
            .unwrap()
            .commit(&mut world)
            .unwrap();
        let sources = world
            .surface()
            .resources()
            .filter(|resource| resource.kind() == SurfaceResourceKind::Opportunity)
            .map(|resource| {
                (
                    resource.id(),
                    resource.position(),
                    resource.commodity().material(),
                )
            })
            .collect::<Vec<_>>();
        for (resource_id, site, material) in sources {
            let kind = if material == MATERIAL_BERRIES {
                ForageScentKind::SweetFruit
            } else {
                ForageScentKind::WildNuts
            };
            for (dx, dz) in [(1_i64, 0_i64), (-1, 0), (0, 1), (0, -1)] {
                let trail = (0..=3)
                    .map(|step| {
                        VoxelCoord::new(site.x() + dx * step, site.y(), site.z() + dz * step)
                    })
                    .collect::<Vec<_>>();
                if !trail.iter().all(|&voxel| {
                    world
                        .world()
                        .has_standing_clearance(registries.world(), voxel)
                }) {
                    continue;
                }
                let mut trial = world.clone();
                crate::logistics::relocate_player_for_test(&mut trial, trail[3]);
                for front in 0..12 {
                    let mut current = trial.clone();
                    apply_clock_advance(&mut current, SimulationTick::new(front * 200));
                    if assess_player_weather(&registries, &current)
                        .and_then(|weather| weather.wind_direction())
                        .map(|wind| wind.delta())
                        != Some((dx, dz))
                        || !assess_player_forage_scents(&registries, &current)
                            .iter()
                            .any(|scent| scent.kind() == kind)
                        || AdjacentSurfaceResource::visible_in_generated_world(
                            &registries,
                            &current,
                        )
                        .any(|visible| visible.resource() == resource_id)
                    {
                        continue;
                    }
                    if project_player_adjacent_walk(&registries, &current, trail[2]).is_err() {
                        continue;
                    }
                    candidate = Some((current, resource_id, trail[3], kind));
                    break 'seeds;
                }
            }
        }
    }
    let (mut state, source, initial_position, kind) = candidate
        .expect("bounded seed search has a safe, flat downwind path to real generated food");
    let starting_mass = state.surface().get(source).unwrap().remaining_mass();
    let accounting = calculate_matter_accounting(&state).unwrap().total();
    let beginning = state.tick();
    let weather = assess_player_weather(&registries, &state).unwrap();
    let (wx, wz) = weather.wind_direction().unwrap().delta();
    let first_step = VoxelCoord::new(
        initial_position.x() - wx,
        initial_position.y(),
        initial_position.z() - wz,
    );
    assert!(
        assess_player_forage_scents(&registries, &state)
            .iter()
            .any(|scent| scent.kind() == kind)
    );
    assert!(
        AdjacentSurfaceResource::visible_in_generated_world(&registries, &state)
            .all(|visible| visible.resource() != source)
    );
    let initial_metabolic_energy = state.survival().player().unwrap().metabolic_energy();
    let control_ticks = walk_physical_for_scent(&registries, &mut state, first_step);
    assert!(control_ticks > 0);
    assert!(state.tick() > beginning);
    assert!(state.survival().player().unwrap().metabolic_energy() < initial_metabolic_energy);
    let seen = AdjacentSurfaceResource::visible_in_generated_world(&registries, &state)
        .find(|visible| visible.resource() == source)
        .expect("approaching upwind must reveal the real source inside sight reach");
    assert_eq!(seen.resource(), source);

    // Replay from the actual first sensory sighting, then approach and work.
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut restored = serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(restored, state);
    let finish = |state: &mut AppState| {
        let destination = seen.position();
        while state.logistics().player().unwrap().position() != destination {
            let current = state.logistics().player().unwrap().position();
            let next = VoxelCoord::new(
                current.x() + (destination.x() - current.x()).signum(),
                current.y(),
                current.z() + (destination.z() - current.z()).signum(),
            );
            walk_physical_for_scent(&registries, state, next);
        }
        let option = player_surface_gathering_options(&registries, state)
            .unwrap()
            .into_iter()
            .find(|choice| choice.resource() == source)
            .expect("physically reached fresh food must have an ordinary gather action");
        let food = option.food_value(&registries, state).unwrap().unwrap();
        assert!(food.dietary_energy().nanojoules() > 0);
        let batch = Mass::from_milligrams(50_000);
        validate_start_player_surface_gathering_option(&registries, state, option, batch)
            .unwrap()
            .commit(state)
            .unwrap();
        let mut worked = 0;
        while state.player_work().active().is_some() {
            assert!(worked < 300);
            let _ = advance_tick(&registries, state).unwrap();
            worked += 1;
        }
        assert!(worked > 0);
        assert_eq!(
            state.surface().get(source).unwrap().remaining_mass(),
            starting_mass.checked_sub(batch).unwrap(),
        );
        assert_eq!(validate_loaded_state(&registries, state), Ok(()));
    };
    finish(&mut state);
    finish(&mut restored);
    assert_eq!(
        state, restored,
        "save/load must preserve the physical expedition and timed work"
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        accounting
    );
}

#[test]
fn seeded_player_neighborhood_forage_is_detectable_beyond_eye_reach() {
    let registries = build_registries();
    let mut state = AppState::new();
    let seed = WorldGenerationSeed::new(0x464F_5241_4745_434F);
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let spawn = VoxelCoord::new(
        0,
        super::super::generation::terrain_height(seed, 0, 0) + 1,
        0,
    );
    validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    super::super::validate_generate_player_neighborhood(&registries, &state)
        .unwrap()
        .expect("generated world needs unseen terrain")
        .commit(&mut state)
        .unwrap();
    let sources = state
        .surface()
        .resources()
        .filter(|source| source.kind() == SurfaceResourceKind::Opportunity)
        .filter_map(|source| {
            let kind = match source.commodity().material() {
                MATERIAL_BERRIES => ForageScentKind::SweetFruit,
                MATERIAL_WILD_NUTS => ForageScentKind::WildNuts,
                _ => return None,
            };
            Some((source.position(), kind))
        })
        .collect::<Vec<_>>();
    assert!(!sources.is_empty());
    let (observed, origin, kind) = sources
        .into_iter()
        .find_map(|(site, kind)| {
            let (witness, _observer) = try_find_downwind_witness(&registries, &state, site, kind)?;
            Some((witness, site, kind))
        })
        .expect("a player should sometimes smell generated wilderness forage before seeing it");
    assert!(
        assess_player_forage_scents(&registries, &observed)
            .iter()
            .any(|scent| scent.kind() == kind)
    );
    assert!(
        AdjacentSurfaceResource::visible_in_generated_world(&registries, &observed)
            .all(|visible| visible.position() != origin),
        "the distant generated source must remain unidentified by short-range vision"
    );
    assert_eq!(validate_loaded_state(&registries, &observed), Ok(()));
}
