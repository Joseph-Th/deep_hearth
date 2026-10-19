//! Kobold opening patrol, physical obstruction, local observation, and continuation contracts.

use crate::content::{FORM_CHIP, MATERIAL_STONE, build_registries};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::AppState;
use crate::material::{CommodityKey, MaterialComposition};
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::advance_tick;
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
    for _ in 0..599 {
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
    for _ in 0..599 {
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

        for _ in 0..599 {
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
        for _ in 0..20 {
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
    for due in [601_u64, 620_u64] {
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
    for _ in 0..599 {
        let _ = advance_tick(&registries, &mut state).expect("quiet opening should advance");
    }
    let outcome = advance_tick(&registries, &mut state).expect("blocked patrol should advance");
    assert!(outcome.kobold_scout().is_none());
    assert_eq!(colony(&state).scout_position(), den);
    assert_eq!(validate_loaded_kobolds(&state), Ok(()));
}
