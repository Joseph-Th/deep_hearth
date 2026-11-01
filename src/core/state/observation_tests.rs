//! Local observation is distinct from persisted physical custody and occupancy.

use super::*;
use crate::content::{
    ENERGY_ELECTRICAL_BUFFER, FLUID_WATER, FORM_CHIP, MATERIAL_STONE,
    STORAGE_ROUGH_TIMBER_FIELD_BOX, build_registries,
};
use crate::core::quantity::{Energy, Mass, Temperature, Volume};
use crate::energy::add_energy_store_with_initial_for_fixture;
use crate::fluid::add_fluid_store_with_contents_for_fixture;
use crate::inventory::StockpileId;
use crate::logistics::{
    add_traversable_edge_for_test, place_energy_store_for_fixture,
    validate_allocate_ground_stockpile, validate_place_fluid_store,
};
use crate::material::{CommodityKey, MaterialComposition};
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::{validate_admit_player, validate_respawn_player};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::{SurfaceResourceKind, SurfaceResourceSpec, validate_create_surface_resources};
use crate::survival::{Vitality, player_record_with_thermal_comfort};

const LOCAL: VoxelCoord = VoxelCoord::new(0, 0, 0);
const NEXT: VoxelCoord = VoxelCoord::new(1, 0, 0);

fn observed_sources() -> (Registries, AppState, StockpileId) {
    let registries = build_registries();
    let mut state = AppState::new();
    add_traversable_edge_for_test(&mut state, LOCAL, NEXT);
    let energy = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        ENERGY_ELECTRICAL_BUFFER,
        Energy::ZERO,
    )
    .unwrap();
    place_energy_store_for_fixture(&mut state, energy, LOCAL);
    let water = Volume::from_microliters(250_000);
    let fluid = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        water,
        FLUID_WATER,
        water,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    validate_place_fluid_store(&state, fluid, LOCAL)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let stockpile = validate_allocate_ground_stockpile(
        &state,
        LOCAL,
        registries
            .storage()
            .get(STORAGE_ROUGH_TIMBER_FIELD_BOX)
            .unwrap()
            .maximum_stockpile_capacity(),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let drop = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        LOCAL,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        Mass::from_milligrams(250_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap();
    validate_create_surface_resources(&registries, &state, [drop])
        .unwrap()
        .commit(&mut state)
        .unwrap();
    (registries, state, stockpile)
}

fn observations(state: &AppState) -> [usize; 6] {
    [
        state.available_local_energy_stores().count(),
        state.available_local_fluid_stores().count(),
        state.available_local_ground_stockpiles().count(),
        state.available_surface_resources().count(),
        state.available_local_equipment().count(),
        state.available_adjacent_traversal_destinations().count(),
    ]
}

#[test]
fn living_local_observations_close_at_death_without_erasing_world_custody() {
    let (registries, mut state, stockpile) = observed_sources();
    assert_eq!(
        observations(&state),
        [0; 6],
        "no actor may observe before admission"
    );
    validate_admit_player(&registries, &state, LOCAL)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(observations(&state), [1, 1, 1, 1, 0, 1]);
    assert_eq!(state.player_observation_position(), Some(LOCAL));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let player = *state.survival().player().unwrap();
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record_with_thermal_comfort(
            player.metabolic_energy(),
            player.hydration(),
            Vitality::ZERO,
            player.nutrition(),
            player.vitality_recovery_remainder(),
            player.thermal_comfort_ppm(),
        ),
    );
    assert_eq!(state.player_observation_position(), None);
    assert_eq!(observations(&state), [0; 6]);
    assert_eq!(state.logistics().player().unwrap().position(), LOCAL);
    assert_eq!(
        state.logistics().stationary_stockpile_position(stockpile),
        Some(LOCAL)
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(state, replay);
    assert_eq!(observations(&replay), [0; 6]);
    let _ = validate_respawn_player(&registries, &state)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let _ = validate_respawn_player(&registries, &replay)
        .unwrap()
        .commit(&mut replay)
        .unwrap();
    assert_eq!(state, replay);
    assert_eq!(state.player_observation_position(), Some(LOCAL));
    // Respawn turns the old carried pack into a second recoverable local ground
    // stockpile. Neither death nor restored sight may delete that custody.
    assert_eq!(observations(&state), [1, 1, 2, 1, 0, 1]);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
