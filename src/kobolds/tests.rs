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

use super::{available_nearby_kobold_scouts, validate_loaded_kobolds};

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
    let patrol_junction = state.kobold_state().colonies[0].route[1];
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
fn kobold_invalid_colony_route_is_rejected_from_trusted_save() {
    let (registries, state, _) = generated_opening();
    let mut tampered = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    tampered["state"]["systems"]["kobolds"]["colonies"][0]["scout_step"] = serde_json::json!(99);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(tampered).unwrap();
    assert!(decoded.into_state(&registries).is_err());
}

#[test]
fn kobold_scout_does_not_pass_through_the_player() {
    let (registries, mut state, _) = generated_opening();
    let [den, junction, _] = state.kobold_state().colonies[0].route;
    validate_admit_player(&registries, &state, junction)
        .expect("player should be admitted at the contested junction")
        .commit(&mut state)
        .expect("junction admission should commit");
    for _ in 0..599 {
        let _ = advance_tick(&registries, &mut state).expect("quiet opening should advance");
    }
    let outcome = advance_tick(&registries, &mut state).expect("blocked patrol should advance");
    assert!(outcome.kobold_scout().is_none());
    assert_eq!(state.kobold_state().colonies[0].scout_position(), den);
    assert_eq!(validate_loaded_kobolds(&state), Ok(()));
}
