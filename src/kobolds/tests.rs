//! Kobold opening patrol, physical obstruction, local observation, and continuation contracts.

use crate::content::{FORM_CHIP, MATERIAL_STONE, build_registries};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
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

#[test]
fn fatal_survival_tick_does_not_make_a_dead_player_guard_the_scout() {
    use crate::core::quantity::Energy;
    use crate::logistics::add_traversable_edge_for_test;
    use crate::survival::{Vitality, player_record_with_thermal_comfort};

    let registries = build_registries();
    let mut state = AppState::new();
    let den = VoxelCoord::new(-1, 0, 0);
    let junction = VoxelCoord::new(0, 0, 0);
    let tip = VoxelCoord::new(1, 0, 0);
    let observer = VoxelCoord::new(-1, 0, 1);
    for (from, to) in [(den, junction), (junction, tip)] {
        add_traversable_edge_for_test(&mut state, from, to);
    }
    state
        .kobold_state_mut()
        .apply_opening_colony([den, junction, tip]);
    validate_admit_player(&registries, &state, observer)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    while state.tick().value() < 179 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    let survival = *state.survival().player().unwrap();
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record_with_thermal_comfort(
            Energy::ZERO,
            survival.hydration(),
            Vitality::from_parts_per_million_unchecked(
                registries
                    .survival()
                    .physiology()
                    .starvation_vitality_loss_ppm_per_tick(),
            ),
            survival.nutrition(),
            survival.vitality_recovery_remainder(),
            survival.thermal_comfort_ppm(),
        ),
    );
    assert!(super::patrol::living_player_deters_scout(
        &registries,
        &state,
        den
    ));
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );

    let mut surviving = state.clone();
    let before = *surviving.survival().player().unwrap();
    let revision = surviving.survival().revision();
    surviving.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record_with_thermal_comfort(
            registries
                .survival()
                .physiology()
                .maximum_metabolic_energy(),
            before.hydration(),
            Vitality::MAXIMUM,
            before.nutrition(),
            before.vitality_recovery_remainder(),
            before.thermal_comfort_ppm(),
        ),
    );
    let _ = advance_tick(&registries, &mut surviving).unwrap();
    assert_eq!(
        colony(&surviving).scout_position(),
        den,
        "a living nearby player actually keeps the scout from leaving the den"
    );

    let _ = advance_tick(&registries, &mut state).unwrap();
    assert_eq!(state.tick(), SimulationTick::new(180));
    assert_eq!(
        state.survival().player().unwrap().vitality(),
        Vitality::ZERO
    );
    assert_eq!(
        colony(&state).scout_position(),
        junction,
        "an actor dying this tick cannot preemptively frighten a scout after death"
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(state, loaded);
}

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
fn terrain_generation_must_not_bury_an_existing_opening_resident() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::core::state::{StateValidationError, validate_loaded_state};
    use crate::logistics::add_traversable_edge_for_test;
    use crate::world::{ChunkCoord, ChunkGenerationError, validate_generate_chunk};

    let registries = build_registries();
    let mut state = AppState::new();
    let den = VoxelCoord::new(8, -10, 8);
    let junction = VoxelCoord::new(9, -10, 8);
    let tip = VoxelCoord::new(10, -10, 8);
    add_traversable_edge_for_test(&mut state, den, junction);
    add_traversable_edge_for_test(&mut state, junction, tip);
    state
        .kobold_state_mut()
        .apply_opening_colony([den, junction, tip]);

    let before = state.clone();
    assert!(matches!(
        validate_generate_chunk(
            &registries,
            &state,
            WorldGenerationSeed::new(0x4B4F_424F_4C44_5445),
            ChunkCoord::containing(den),
            BLOCK_STATE_STONE,
        ),
        Err(ChunkGenerationError::OpeningResidentBodyCollision { voxel }) if voxel == den
    ));
    assert_eq!(state, before, "rejected generation must not modify custody");

    // Loading cannot legitimize an overlapped resident created by any other
    // state-construction path, even if the chunk itself is otherwise valid.
    let mut invalid = AppState::new();
    validate_generate_chunk(
        &registries,
        &invalid,
        WorldGenerationSeed::new(0x4B4F_424F_4C44_5445),
        ChunkCoord::containing(den),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut invalid)
    .unwrap();
    add_traversable_edge_for_test(&mut invalid, den, junction);
    add_traversable_edge_for_test(&mut invalid, junction, tip);
    invalid
        .kobold_state_mut()
        .apply_opening_colony([den, junction, tip]);
    assert!(matches!(
        validate_loaded_state(&registries, &invalid),
        Err(StateValidationError::Kobolds(
            "generated terrain intersects an opening kobold resident"
        ))
    ));

    // The same authored resident route above real ground does not reserve
    // an entire chunk or prevent valid generation and save admission.
    let mut clear = AppState::new();
    let air_den = VoxelCoord::new(8, 30, 8);
    let air_junction = VoxelCoord::new(9, 30, 8);
    let air_tip = VoxelCoord::new(10, 30, 8);
    add_traversable_edge_for_test(&mut clear, air_den, air_junction);
    add_traversable_edge_for_test(&mut clear, air_junction, air_tip);
    clear
        .kobold_state_mut()
        .apply_opening_colony([air_den, air_junction, air_tip]);
    validate_generate_chunk(
        &registries,
        &clear,
        WorldGenerationSeed::new(0x4B4F_424F_4C44_5445),
        ChunkCoord::containing(air_den),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut clear)
    .unwrap();
    assert_eq!(validate_loaded_state(&registries, &clear), Ok(()));
}

#[test]
fn scout_move_invalidates_a_previously_clear_chunk_generation_plan() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::logistics::add_traversable_edge_for_test;
    use crate::world::{ChunkCoord, ChunkGenerationCommitError, validate_generate_chunk};

    let registries = build_registries();
    let mut state = AppState::new();
    let den = VoxelCoord::new(15, -10, 8);
    let junction = VoxelCoord::new(16, -10, 8);
    let tip = VoxelCoord::new(17, -10, 8);
    add_traversable_edge_for_test(&mut state, den, junction);
    add_traversable_edge_for_test(&mut state, junction, tip);
    state
        .kobold_state_mut()
        .apply_opening_colony([den, junction, tip]);

    // The scout is initially across the chunk border, so a solid candidate
    // chunk at the junction is legal at validation time.
    let planned = validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0x4B4F_424F_4C44_5445),
        ChunkCoord::containing(junction),
        BLOCK_STATE_STONE,
    )
    .unwrap();
    while state.tick().value() < 180 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(colony(&state).scout_position(), junction);
    let before = state.clone();
    assert!(matches!(
        planned.commit(&mut state),
        Err(ChunkGenerationCommitError::StaleOpeningResidents {
            expected: Some([expected_scout, _]),
            actual: Some([actual_scout, _]),
        }) if expected_scout == den && actual_scout == junction
    ));
    assert_eq!(state, before, "stale generation must not create terrain");
}

#[test]
fn distant_scout_patrol_does_not_invalidate_unrelated_chunk_generation() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::logistics::add_traversable_edge_for_test;
    use crate::world::{ChunkCoord, validate_generate_chunk};

    let registries = build_registries();
    let mut state = AppState::new();
    let den = VoxelCoord::new(8, -10, 8);
    let junction = VoxelCoord::new(9, -10, 8);
    let tip = VoxelCoord::new(10, -10, 8);
    for (from, to) in [(den, junction), (junction, tip)] {
        add_traversable_edge_for_test(&mut state, from, to);
    }
    state
        .kobold_state_mut()
        .apply_opening_colony([den, junction, tip]);
    let far_chunk = ChunkCoord::new(4, 4);
    let planned = validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0x4B4F_424F_4C44_5445),
        far_chunk,
        BLOCK_STATE_STONE,
    )
    .unwrap();
    while state.tick().value() < 180 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(colony(&state).scout_position(), junction);
    assert_eq!(planned.commit(&mut state), Ok(far_chunk));
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(loaded, state);
}

#[test]
fn distant_scout_patrol_does_not_block_player_neighborhood_streaming() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::logistics::add_traversable_edge_for_test;
    use crate::world::{
        BlockStateId, ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, set_generated_block_for_test,
        validate_generate_chunk, validate_generate_player_neighborhood,
    };

    let registries = build_registries();
    let mut state = AppState::new();
    let seed = WorldGenerationSeed::new(0x4B4F_424F_4C44_5445);
    for chunk in [ChunkCoord::new(0, 0), ChunkCoord::new(4, 4)] {
        validate_generate_chunk(&registries, &state, seed, chunk, BLOCK_STATE_STONE)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    }
    let surface = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|y| state.world().block_at(VoxelCoord::new(0, *y, 0)) == Some(BLOCK_STATE_STONE))
        .unwrap();
    validate_admit_player(&registries, &state, VoxelCoord::new(0, surface + 1, 0))
        .unwrap()
        .commit(&mut state)
        .unwrap();

    // A physically supported patrol in an already generated distant column
    // continues while the player demands only the missing nearby chunks.
    let den = VoxelCoord::new(72, 80, 72);
    let junction = VoxelCoord::new(73, 80, 72);
    let tip = VoxelCoord::new(74, 80, 72);
    for location in [den, junction, tip] {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(location.x(), location.y() - 1, location.z()),
            BLOCK_STATE_STONE,
        );
        for y in location.y()..=location.y() + 1 {
            set_generated_block_for_test(
                registries.world(),
                &mut state,
                VoxelCoord::new(location.x(), y, location.z()),
                BlockStateId::AIR,
            );
        }
    }
    for (from, to) in [(den, junction), (junction, tip)] {
        add_traversable_edge_for_test(&mut state, from, to);
    }
    state
        .kobold_state_mut()
        .apply_opening_colony([den, junction, tip]);
    let pending = validate_generate_player_neighborhood(&registries, &state)
        .unwrap()
        .unwrap();
    assert_eq!(pending.chunks().len(), 8);
    while state.tick().value() < 180 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(colony(&state).scout_position(), junction);
    assert_eq!(pending.commit(&mut state).unwrap().len(), 8);
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}

#[test]
fn player_guards_diagonal_clearing_but_cannot_deter_through_an_opaque_corner() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::world::{
        BlockStateId, ChunkCoord, set_generated_block_for_test, validate_generate_chunk,
    };

    let (registries, mut state, _) = generated_opening();
    let scout = colony(&state).route[1];
    let observer = VoxelCoord::new(scout.x() + 1, scout.y(), scout.z() + 1);
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0x4B4F_424F_4C44),
        ChunkCoord::containing(scout),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let x_corner = VoxelCoord::new(observer.x(), scout.y(), scout.z());
    let z_corner = VoxelCoord::new(scout.x(), scout.y(), observer.z());
    for position in [scout, observer, x_corner, z_corner] {
        for y in position.y()..=position.y() + 1 {
            set_generated_block_for_test(
                registries.world(),
                &mut state,
                VoxelCoord::new(position.x(), y, position.z()),
                BlockStateId::AIR,
            );
        }
    }
    validate_admit_player(&registries, &state, observer)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert!(super::patrol::living_player_deters_scout(
        &registries,
        &state,
        scout
    ));

    set_generated_block_for_test(registries.world(), &mut state, x_corner, BLOCK_STATE_STONE);
    assert!(
        !super::patrol::living_player_deters_scout(&registries, &state, scout),
        "a player hidden behind a real opaque corner must not intimidate a scout"
    );
    set_generated_block_for_test(registries.world(), &mut state, x_corner, BlockStateId::AIR);
    assert!(super::patrol::living_player_deters_scout(
        &registries,
        &state,
        scout
    ));
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}

/// Gather every locally observed nut through the same player work owner used
/// in gameplay. The real remaining surface source, not a flag on the scout,
/// then drives its decision to seek the neighboring berry opportunity.
fn exhaust_junction_nuts(
    registries: &crate::Registries,
    state: &mut AppState,
) -> (VoxelCoord, VoxelCoord, crate::surface::SurfaceResourceId) {
    use crate::content::{FORM_FOOD, MATERIAL_BERRIES, MATERIAL_WILD_NUTS};
    use crate::surface::{
        player_surface_gathering_options, validate_start_player_surface_gathering_option,
    };
    let junction = colony(state).route[1];
    let forage = state
        .logistics()
        .traversable_destinations(junction)
        .find(|position| {
            state
                .surface()
                .available_resources_at(Some(*position))
                .any(|resource| {
                    resource.commodity() == CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD)
                })
        })
        .expect("opening provides an adjacent berry opportunity");
    let source = state
        .surface()
        .available_resources_at(Some(forage))
        .find(|resource| resource.commodity() == CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD))
        .unwrap()
        .id();
    let nuts = state
        .surface()
        .available_resources_at(Some(junction))
        .find(|resource| resource.commodity() == CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD))
        .unwrap()
        .id();
    validate_admit_player(registries, state, junction)
        .unwrap()
        .commit(state)
        .unwrap();
    while state.surface().get(nuts).unwrap().remaining_mass() != Mass::ZERO {
        let option = player_surface_gathering_options(registries, state)
            .unwrap()
            .into_iter()
            .find(|option| option.resource() == nuts)
            .unwrap();
        let batch = state
            .surface()
            .get(nuts)
            .unwrap()
            .remaining_mass()
            .min(option.maximum_batch_mass());
        validate_start_player_surface_gathering_option(registries, state, option, batch)
            .unwrap()
            .commit(state)
            .unwrap();
        for _ in 0..100 {
            if state.player_work().active().is_none() {
                break;
            }
            let _ = advance_tick(registries, state).unwrap();
        }
        assert!(
            state.player_work().active().is_none(),
            "authoritative surface gathering must finish before the scout arrives"
        );
    }
    // Graph distance is not physical proximity: clay may be diagonally adjacent
    // to the scout's clearing. Owner-level setup must put the observer beyond the
    // actual local fear radius for a test of unguarded foraging.
    let water = state
        .logistics()
        .traversable_destinations(forage)
        .find(|position| {
            *position != junction
                && state.logistics().has_traversal_edge(
                    *position,
                    state
                        .logistics()
                        .traversable_destinations(*position)
                        .find(|next| *next != forage)
                        .unwrap(),
                )
        })
        .expect("opening berry branch reaches the water cell");
    let clay = state
        .logistics()
        .traversable_destinations(water)
        .find(|position| *position != forage)
        .unwrap();
    crate::logistics::relocate_player_for_test(
        state,
        VoxelCoord::new(clay.x() + 4, clay.y(), clay.z() + 4),
    );
    (junction, forage, source)
}

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
    assert_eq!(super::den_cache_space(&state, den), Mass::ZERO);
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
        super::den_cache_space(&state, den),
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
    assert_eq!(super::den_cache_space(&state, den), Mass::ZERO);
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
        super::KoboldTrailKind::Foraging
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
    assert!(!super::patrol::living_player_deters_scout(
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
    assert_eq!(super::den_cache_space(&state, den), Mass::ZERO);
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
    assert_eq!(super::den_cache_space(&state, den), Mass::ZERO);
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
fn opaque_terrain_conceals_a_close_scout_until_line_of_sight_is_cleared() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::world::{
        BlockStateId, ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, set_generated_block_for_test,
        validate_generate_chunk,
    };

    let registries = build_registries();
    let mut state = AppState::new();
    let seed = WorldGenerationSeed::new(0x5749_4C44_4552_4E53);
    let anchor = VoxelCoord::new(8, 0, 8);
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::containing(anchor),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|&y| {
            state
                .world()
                .block_at(VoxelCoord::new(anchor.x(), y, anchor.z()))
                == Some(BLOCK_STATE_STONE)
        })
        .expect("generated stone floor exists");
    let spawn = VoxelCoord::new(anchor.x(), surface_y + 1, anchor.z());
    validate_generate_initial_wilderness(&registries, &state, seed, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();

    let [den, junction, tip] = colony(&state).route;
    advance_to_tick(&registries, &mut state, 185);
    assert_eq!(colony(&state).scout_position(), tip);
    assert_eq!(state.world().block_at(junction), Some(BlockStateId::AIR));
    validate_admit_player(&registries, &state, den)
        .unwrap()
        .commit(&mut state)
        .unwrap();

    let visible = available_nearby_kobold_scouts(&registries, &state)
        .next()
        .expect("unobstructed scout is visible at two voxels");
    assert_eq!(visible.position(), tip);
    let conserved = calculate_matter_accounting(&state).unwrap().total();

    set_generated_block_for_test(registries.world(), &mut state, junction, BLOCK_STATE_STONE);
    let hidden_scout = available_nearby_kobold_scouts(&registries, &state).collect::<Vec<_>>();
    assert!(
        hidden_scout
            .iter()
            .all(|resident| resident.role() != super::KoboldRole::Scout),
        "a stone wall must conceal the scout across the junction"
    );
    assert!(
        hidden_scout.iter().any(|resident| {
            resident.role() == super::KoboldRole::Watchman && resident.position() == den
        }),
        "the original den resident remains physically visible beside the player"
    );

    set_generated_block_for_test(registries.world(), &mut state, junction, BlockStateId::AIR);
    assert_eq!(
        available_nearby_kobold_scouts(&registries, &state)
            .next()
            .unwrap()
            .position(),
        tip
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );

    advance_to_tick(&registries, &mut state, 190);
    assert_eq!(colony(&state).scout_position(), junction);
    assert!(
        available_nearby_kobold_scouts(&registries, &state)
            .next()
            .is_some()
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}

#[test]
fn opaque_wall_hides_a_moving_scout_event_without_blocking_its_actual_route() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::world::{
        BlockStateId, ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, set_generated_block_for_test,
        validate_generate_chunk,
    };
    let registries = build_registries();
    let mut state = AppState::new();
    let seed = WorldGenerationSeed::new(0x5749_4C44_4552_4E53);
    let anchor = VoxelCoord::new(8, 0, 8);
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::containing(anchor),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|&y| {
            state
                .world()
                .block_at(VoxelCoord::new(anchor.x(), y, anchor.z()))
                == Some(BLOCK_STATE_STONE)
        })
        .unwrap();
    let spawn = VoxelCoord::new(anchor.x(), surface_y + 1, anchor.z());
    validate_generate_initial_wilderness(&registries, &state, seed, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let [den, junction, tip] = colony(&state).route;
    let (mid, observer) = [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .into_iter()
        .find_map(|(dx, dz)| {
            let mid = VoxelCoord::new(junction.x() + dx, junction.y(), junction.z() + dz);
            let observer = VoxelCoord::new(mid.x() + dx, mid.y(), mid.z() + dz);
            (mid != den
                && mid != tip
                && state.world().block_at(mid) == Some(BlockStateId::AIR)
                && state.world().block_at(observer) == Some(BlockStateId::AIR))
            .then_some((mid, observer))
        })
        .expect("opening terrain includes one clear off-route two-voxel sightline");
    validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    crate::logistics::relocate_player_for_test(&mut state, observer);
    advance_to_tick(&registries, &mut state, 179);
    let mut concealed = state.clone();
    set_generated_block_for_test(registries.world(), &mut concealed, mid, BLOCK_STATE_STONE);
    let clear_event = advance_tick(&registries, &mut state).unwrap();
    assert_eq!(clear_event.kobold_scout().unwrap().position(), junction);
    let concealed_event = advance_tick(&registries, &mut concealed).unwrap();
    assert!(
        concealed_event.kobold_scout().is_none(),
        "the same moving scout must not leak through a nearby opaque wall"
    );
    assert_eq!(
        colony(&state).scout_position(),
        colony(&concealed).scout_position(),
        "sight occlusion must not stop or teleport the actual scout"
    );
    assert!(
        available_nearby_kobold_scouts(&registries, &concealed)
            .next()
            .is_none()
    );
    set_generated_block_for_test(registries.world(), &mut concealed, mid, BlockStateId::AIR);
    assert_eq!(
        available_nearby_kobold_scouts(&registries, &concealed)
            .next()
            .unwrap()
            .position(),
        junction
    );
}

#[test]
fn a_diagonal_scout_sighting_does_not_peek_through_an_opaque_corner() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::world::{
        BlockStateId, ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, set_generated_block_for_test,
        validate_generate_chunk,
    };

    let registries = build_registries();
    let mut state = AppState::new();
    let seed = WorldGenerationSeed::new(0x5749_4C44_4552_4E53);
    let anchor = VoxelCoord::new(8, 0, 8);
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::containing(anchor),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|&y| {
            state
                .world()
                .block_at(VoxelCoord::new(anchor.x(), y, anchor.z()))
                == Some(BLOCK_STATE_STONE)
        })
        .unwrap();
    let spawn = VoxelCoord::new(anchor.x(), surface_y + 1, anchor.z());
    validate_generate_initial_wilderness(&registries, &state, seed, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let scout = colony(&state).route[1];
    let diagonal = VoxelCoord::new(scout.x() + 1, scout.y(), scout.z() + 1);
    let x_leg = VoxelCoord::new(diagonal.x(), scout.y(), scout.z());
    let z_leg = VoxelCoord::new(scout.x(), scout.y(), diagonal.z());
    for voxel in [scout, diagonal, x_leg, z_leg] {
        assert_eq!(state.world().block_at(voxel), Some(BlockStateId::AIR));
    }
    assert!(super::scout_visible_from(
        &registries,
        &state,
        scout,
        diagonal
    ));
    for occlusion in [x_leg, z_leg] {
        set_generated_block_for_test(registries.world(), &mut state, occlusion, BLOCK_STATE_STONE);
        assert!(
            !super::scout_visible_from(&registries, &state, scout, diagonal),
            "a solid wall on either side of a corner conceals the scout"
        );
        set_generated_block_for_test(registries.world(), &mut state, occlusion, BlockStateId::AIR);
    }
    assert!(super::scout_visible_from(
        &registries,
        &state,
        scout,
        diagonal
    ));
}

fn generated_terrain_scout_opening() -> (crate::Registries, AppState, VoxelCoord) {
    use crate::content::BLOCK_STATE_STONE;
    use crate::world::{ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, validate_generate_chunk};
    let registries = build_registries();
    let mut state = AppState::new();
    let seed = WorldGenerationSeed::new(0x5749_4C44_4552_4E53);
    let origin = VoxelCoord::new(8, 0, 8);
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::containing(origin),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|&y| state.world().block_at(VoxelCoord::new(8, y, 8)) == Some(BLOCK_STATE_STONE))
        .unwrap();
    let spawn = VoxelCoord::new(8, surface_y + 1, 8);
    validate_generate_initial_wilderness(&registries, &state, seed, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    (registries, state, spawn)
}

#[test]
fn terrain_barrier_after_first_patrol_step_sends_scout_home_without_teleportation() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::world::{BlockStateId, set_generated_block_for_test};
    let (registries, mut state, spawn) = generated_terrain_scout_opening();
    let [den, junction, tip] = colony(&state).route;
    advance_to_tick(&registries, &mut state, 180);
    assert_eq!(colony(&state).scout_position(), junction);
    assert_eq!(state.world().block_at(tip), Some(BlockStateId::AIR));

    // The observer beyond the den now actually guards the return edge.
    // An obstructed scout must wait at the junction until both the route
    // and the visible threat allow it to reach home.
    let beyond = VoxelCoord::new(
        den.x() + (den.x() - junction.x()),
        den.y(),
        den.z() + (den.z() - junction.z()),
    );
    validate_admit_player(&registries, &state, beyond)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let before = calculate_matter_accounting(&state).unwrap().total();
    // Block standing head clearance without burying the finite natural
    // opportunity already present on the trail floor at tip.
    let tip_head = VoxelCoord::new(tip.x(), tip.y() + 1, tip.z());
    set_generated_block_for_test(registries.world(), &mut state, tip_head, BLOCK_STATE_STONE);
    advance_to_tick(&registries, &mut state, 184);
    let outcome = advance_tick(&registries, &mut state).unwrap();
    assert_eq!(colony(&state).scout_position(), junction);
    assert!(!colony(&state).returning);
    assert_eq!(colony(&state).route, [den, junction, tip]);
    assert_eq!(outcome.kobold_scout(), None);
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&saved).unwrap();
    let mut loaded = decoded.into_state(&registries).unwrap();
    assert_eq!(state, loaded);
    crate::logistics::relocate_player_for_test(&mut state, spawn);
    crate::logistics::relocate_player_for_test(&mut loaded, spawn);
    advance_to_tick(&registries, &mut state, 190);
    advance_to_tick(&registries, &mut loaded, 190);
    assert_eq!(state, loaded);
    assert_eq!(colony(&state).scout_position(), den);
    assert!(colony(&state).returning);
    set_generated_block_for_test(registries.world(), &mut state, tip_head, BlockStateId::AIR);
    set_generated_block_for_test(registries.world(), &mut loaded, tip_head, BlockStateId::AIR);
    advance_to_tick(&registries, &mut state, 200);
    advance_to_tick(&registries, &mut loaded, 200);
    assert_eq!(state, loaded);
    assert_eq!(colony(&state).scout_position(), den);
    assert_eq!(
        colony(&state).wary_until,
        Some(SimulationTick::new(230)),
        "the den watchman's physically delivered warning keeps the scout cautious after retreat"
    );
    advance_to_tick(&registries, &mut state, 235);
    advance_to_tick(&registries, &mut loaded, 235);
    assert_eq!(state, loaded);
    assert_eq!(colony(&state).scout_position(), tip);
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), before);
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}

#[test]
fn blocked_far_cache_goal_cancels_detour_and_preserves_original_parcel() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::world::{BlockStateId, set_generated_block_for_test};
    let (registries, mut state, spawn) = generated_terrain_scout_opening();
    let [den, junction, tip] = colony(&state).route;
    let cache = place_loose_stone_drop(&registries, &mut state, spawn);
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    advance_to_tick(&registries, &mut state, 185);
    assert_eq!(colony(&state).scout_step, 2);
    assert_eq!(colony(&state).detour_goal, Some(spawn));
    let branch = colony(&state).scout_position();
    // Keep the cache's real drop exposed while closing the headroom above it.
    let cache_head = VoxelCoord::new(spawn.x(), spawn.y() + 1, spawn.z());
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        cache_head,
        BLOCK_STATE_STONE,
    );
    advance_to_tick(&registries, &mut state, 190);
    assert_eq!(colony(&state).scout_position(), junction);
    assert!(colony(&state).returning);
    assert_eq!(state.surface().get(cache).unwrap().position(), spawn);
    advance_to_tick(&registries, &mut state, 195);
    assert_eq!(colony(&state).scout_position(), den);
    assert_eq!(colony(&state).detour_goal, None);
    assert_eq!(colony(&state).detour, None);
    assert_eq!(colony(&state).route, [den, junction, tip]);
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&saved).unwrap();
    let mut loaded = decoded.into_state(&registries).unwrap();
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        cache_head,
        BlockStateId::AIR,
    );
    set_generated_block_for_test(
        registries.world(),
        &mut loaded,
        cache_head,
        BlockStateId::AIR,
    );
    for _ in 0..30 {
        assert_eq!(
            advance_tick(&registries, &mut state).unwrap(),
            advance_tick(&registries, &mut loaded).unwrap()
        );
    }
    assert_eq!(state, loaded);
    assert_ne!(branch, spawn);
    assert_eq!(state.surface().get(cache).unwrap().position(), den);
    assert_eq!(
        state.surface().get(cache).unwrap().remaining_mass(),
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
    assert_eq!(validate_loaded_kobolds(&registries, &state), Ok(()));
}
