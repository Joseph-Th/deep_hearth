//! Late chunk discovery must not temporarily restore crops that already aged away.

use super::*;
use crate::content::{BLOCK_STATE_STONE, build_registries};
use crate::core::state::{AppState, apply_clock_advance, validate_loaded_state};
use crate::core::time::SimulationTick;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::advance_tick;
use crate::surface::SurfaceResourceLifecycle;
use crate::world::{ChunkCoord, WorldGenerationSeed, validate_generate_chunk};

const FIRST_SENESCENCE: u64 = super::super::super::forage_cycle::FRUIT_SENESCENCE_TICKS;

fn discover_at(at: SimulationTick) -> (Registries, AppState) {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x464F_5241_4745_434F);
    let mut state = AppState::new();
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
    let spawn = VoxelCoord::new(0, super::terrain_height(seed, 0, 0) + 1, 0);
    validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    apply_clock_advance(&mut state, at);
    super::super::validate_generate_player_neighborhood(&registries, &state)
        .unwrap()
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    (registries, state)
}

#[test]
fn lazy_original_crop_senescence_matches_real_weather_front_execution() {
    let (registries, mut continuous) = discover_at(SimulationTick::ZERO);
    for _ in 0..FIRST_SENESCENCE {
        let _ = advance_tick(&registries, &mut continuous).unwrap();
    }
    let (_, delayed) = discover_at(continuous.tick());
    let snapshot = |state: &AppState| {
        state
            .surface()
            .perennial_resources()
            .map(|plant| {
                (
                    plant.position(),
                    plant.initial_mass(),
                    plant.remaining_mass(),
                    plant.perennial_root_mass(),
                    plant.lifecycle(),
                    plant.depleted_at(),
                    plant.perennial.unwrap().ripened_at,
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        snapshot(&delayed),
        snapshot(&continuous),
        "late generation must match actual first-front senescence, not a stale harvest window"
    );
    assert_eq!(validate_loaded_state(&registries, &continuous), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &delayed), Ok(()));
}

#[test]
fn undiscovered_seeded_fruit_senesces_at_its_original_front_without_refreshing_biomass() {
    let (_, early) = discover_at(SimulationTick::new(FIRST_SENESCENCE - 1));
    let (registries, late) = discover_at(SimulationTick::new(FIRST_SENESCENCE));
    let early_patches = early
        .surface()
        .perennial_resources()
        .map(|source| {
            (
                source.position(),
                source.initial_mass(),
                source.remaining_mass(),
            )
        })
        .collect::<Vec<_>>();
    assert!(!early_patches.is_empty());
    for (position, initial, fruit) in early_patches {
        assert!(!fruit.is_zero());
        let patch = late
            .surface()
            .perennial_resources()
            .find(|source| source.position() == position)
            .expect("the same seed-derived plant must persist through delayed generation");
        assert_eq!(patch.initial_mass(), initial);
        assert_eq!(patch.lifecycle(), SurfaceResourceLifecycle::Depleted);
        assert_eq!(patch.remaining_mass(), Mass::ZERO);
        assert_eq!(patch.perennial_root_mass(), initial);
        assert_eq!(patch.created_at(), SimulationTick::ZERO);
        assert_eq!(
            patch.depleted_at(),
            Some(SimulationTick::new(FIRST_SENESCENCE))
        );
        assert!(late.surface().rooted_biomass_occupies_voxel(position));
    }
    assert_eq!(
        calculate_matter_accounting(&late).unwrap().total(),
        calculate_matter_accounting(&early).unwrap().total(),
        "senescence must only transfer the same finite biomass into roots"
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &late)).unwrap();
    let reloaded = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(reloaded, late);
}

#[test]
fn late_seeded_roots_can_ripen_again_on_real_fronts_without_minting_food() {
    let (registries, mut state) = discover_at(SimulationTick::new(FIRST_SENESCENCE));
    assert!(
        state
            .surface()
            .perennial_resources()
            .all(|plant| plant.lifecycle() == SurfaceResourceLifecycle::Depleted)
    );

    let mut regrew = false;
    for _ in 0..FIRST_SENESCENCE {
        let _ = advance_tick(&registries, &mut state).unwrap();
        for plant in state.surface().perennial_resources() {
            assert_eq!(
                plant
                    .perennial_root_mass()
                    .checked_add(plant.remaining_mass()),
                Some(plant.initial_mass()),
                "each new fruit crop transfers existing biomass from the same living roots"
            );
            regrew |= plant.lifecycle() == SurfaceResourceLifecycle::Available;
        }
        if regrew {
            break;
        }
    }
    assert!(
        regrew,
        "spring ripening should recover a delayed but still living patch"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for _ in 0..24 {
        let _ = advance_tick(&registries, &mut state).unwrap();
        let _ = advance_tick(&registries, &mut replay).unwrap();
    }
    assert_eq!(state, replay);
    assert_eq!(validate_loaded_state(&registries, &replay), Ok(()));
}
