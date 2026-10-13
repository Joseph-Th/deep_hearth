//! Contract tests for material-backed local campfire combustion and warmth.

use crate::content::{
    CAMPFIRE_FUEL_MASS, CAMPFIRE_FUEL_THERMAL_ENERGY, ENERGY_STONE_RING_CAMPFIRE, FORM_LOG,
    FORM_LUMP, MATERIAL_STONE, MATERIAL_WOOD, build_registries,
};
use crate::core::quantity::{AggregateMass, Mass};
use crate::core::state::{AppState, validate_loaded_state};
use crate::inventory::deposit_bulk_for_test;
use crate::logistics::{add_traversable_edge_for_test, validate_start_player_step};
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::player::validate_admit_player;
use crate::simulation::advance_tick;
use crate::spatial::VoxelCoord;
use crate::survival::player_record_with_thermal_comfort;

use crate::energy::validate_assemble_energy_store;

use super::validate_combust_fuel;

fn campfire_fixture() -> (
    crate::registry::Registries,
    AppState,
    crate::inventory::StockpileId,
    crate::energy::EnergyStoreId,
) {
    let registries = build_registries();
    let mut state = AppState::new();
    let origin = VoxelCoord::new(0, 0, 0);
    validate_admit_player(&registries, &state, origin)
        .unwrap_or_else(|error| panic!("campfire player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("campfire player admission commit failed: {error}"));
    let carried = state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .unwrap_or_else(|| panic!("campfire fixture player disappeared"));
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(1_000_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            CAMPFIRE_FUEL_MASS,
        ),
    ] {
        deposit_bulk_for_test(&registries, &mut state, carried, commodity, mass)
            .unwrap_or_else(|error| panic!("campfire material setup failed: {error}"));
    }
    let fire =
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_RING_CAMPFIRE, carried)
            .unwrap_or_else(|error| panic!("campfire assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("campfire assembly commit failed: {error}"));
    (registries, state, carried, fire)
}

#[test]
fn authored_combustion_conserves_fuel_matter_and_creates_finite_heat() {
    let (registries, mut state, carried, fire) = campfire_fixture();
    let before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("campfire pre-burn matter accounting failed: {error}"));

    let released = validate_combust_fuel(&registries, &state, fire, carried)
        .unwrap_or_else(|error| panic!("campfire fuel validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("campfire fuel commit failed: {error}"));
    let after = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("campfire post-burn matter accounting failed: {error}"));

    assert_eq!(released, CAMPFIRE_FUEL_THERMAL_ENERGY);
    assert_eq!(
        state.energy().get_store(fire).map(|store| store.stored()),
        Some(released)
    );
    assert_eq!(after.total(), before.total());
    assert_eq!(
        after.combusted(),
        AggregateMass::from_mass(CAMPFIRE_FUEL_MASS)
    );
    assert_eq!(
        before.stored().checked_sub(after.stored()),
        Some(AggregateMass::from_mass(CAMPFIRE_FUEL_MASS))
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn passive_campfire_heat_warms_only_a_colocated_player() {
    let (registries, mut state, carried, fire) = campfire_fixture();
    validate_combust_fuel(&registries, &state, fire, carried)
        .unwrap_or_else(|error| panic!("campfire fuel validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("campfire fuel commit failed: {error}"));
    let player = state
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("campfire survival player disappeared"));
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record_with_thermal_comfort(
            player.metabolic_energy(),
            player.hydration(),
            player.vitality(),
            player.nutrition(),
            player.vitality_recovery_remainder(),
            700_000,
        ),
    );

    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("campfire warming tick failed: {error}"));
    assert_eq!(
        state
            .survival()
            .player()
            .map(|player| player.thermal_comfort_ppm()),
        Some(719_000),
        "one local fire tick should gain 20,000 ppm and pay 1,000 ppm unsheltered loss"
    );

    let origin = VoxelCoord::new(0, 0, 0);
    let remote = VoxelCoord::new(1, 0, 0);
    add_traversable_edge_for_test(&mut state, origin, remote);
    validate_start_player_step(&registries, &state, remote)
        .unwrap_or_else(|error| panic!("campfire departure failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("campfire departure commit failed: {error}"));
    while state
        .logistics()
        .player()
        .is_some_and(|player| player.position() != remote)
    {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("campfire departure tick failed: {error}"));
    }
    let remote_before = state
        .survival()
        .player()
        .map(|player| player.thermal_comfort_ppm())
        .unwrap_or(0);
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("remote exposure tick failed: {error}"));
    assert_eq!(
        state
            .survival()
            .player()
            .map(|player| player.thermal_comfort_ppm()),
        remote_before.checked_sub(1_000),
        "a remote burning campfire must not warm the player"
    );
    assert!(
        state
            .energy()
            .get_store(fire)
            .is_some_and(|store| !store.stored().is_zero())
    );
}
