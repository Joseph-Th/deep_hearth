//! A salvaged warning device must change a real patrol without inventing matter or radio.

use super::*;

use crate::content::{
    ENERGY_ELECTRICAL_BUFFER, FORM_ELECTRICAL_WINDING, FORM_HANDLE, MATERIAL_COPPER,
    build_registries,
};
use crate::core::quantity::{Energy, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
use crate::energy::add_energy_store_with_initial_for_fixture;
use crate::logistics::{
    add_traversable_edge_for_test, place_energy_store_for_fixture, relocate_player_for_test,
};
use crate::material::{CommodityKey, MaterialComposition};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::{advance_control_tick, advance_tick};
use crate::surface::{SurfaceResourceSpec, validate_create_surface_resources};

const DEN: VoxelCoord = VoxelCoord::new(-1, 0, 0);
const JUNCTION: VoxelCoord = VoxelCoord::new(0, 0, 0);
const TIP: VoxelCoord = VoxelCoord::new(1, 0, 0);
const OBSERVER: VoxelCoord = VoxelCoord::new(-3, 0, 0);
const TEMPERATURE: Temperature = Temperature::from_millikelvin(293_150);

fn advance_to(registries: &Registries, state: &mut AppState, until: u64) {
    while state.tick().value() < until {
        if state.logistics().player().is_some() {
            let _ = advance_control_tick(registries, state).unwrap();
        } else {
            let _ = advance_tick(registries, state).unwrap();
        }
    }
}

fn put_drop(
    registries: &Registries,
    state: &mut AppState,
    commodity: CommodityKey,
) -> SurfaceResourceId {
    let spec = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        DEN,
        commodity,
        Mass::from_milligrams(50_000),
        TEMPERATURE,
        MaterialComposition::pure(commodity.material()),
    )
    .unwrap();
    let planned = validate_create_surface_resources(registries, state, [spec]).unwrap();
    let id = planned.first_resource_id();
    planned.commit(state).unwrap();
    id
}

fn mobilized_den() -> (Registries, AppState) {
    let registries = build_registries();
    let mut state = AppState::new();
    for (a, b) in [(DEN, JUNCTION), (JUNCTION, TIP)] {
        add_traversable_edge_for_test(&mut state, a, b);
    }
    state
        .kobold_state_mut()
        .apply_opening_colony([DEN, JUNCTION, TIP]);
    let machinery = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        ENERGY_ELECTRICAL_BUFFER,
        Energy::ZERO,
    )
    .unwrap();
    place_energy_store_for_fixture(&mut state, machinery, JUNCTION);
    advance_to(&registries, &mut state, 200);
    assert_eq!(
        state.kobold_state().colony.as_ref().unwrap().mobilized_at,
        Some(SimulationTick::new(195))
    );
    assert_eq!(state.kobold_state().colony.as_ref().unwrap().scout_step, 1);
    (registries, state)
}

#[test]
fn real_salvage_makes_an_unreliable_local_clatter_alarm_and_changes_a_scouts_route() {
    let (registries, mut state) = mobilized_den();
    let mut without_parts = state.clone();
    let core = put_drop(
        &registries,
        &mut state,
        CommodityKey::new(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING),
    );
    let striker = put_drop(
        &registries,
        &mut state,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
    );
    for world in [&mut state, &mut without_parts] {
        validate_admit_player(&registries, world, OBSERVER)
            .unwrap()
            .commit(world)
            .unwrap();
    }
    let rig = available_nearby_kobold_junk_rig(&registries, &state).unwrap();
    assert_eq!(rig.core(), core);
    assert_eq!(rig.striker(), striker);
    assert_eq!(rig.position(), DEN);
    assert_eq!(rig.kind(), KoboldJunkRigKind::StoneClatter);
    assert!(rig.sounds_at(SimulationTick::new(205)));
    assert!(!rig.sounds_at(SimulationTick::new(210)));
    assert_eq!(
        available_nearby_kobold_junk_rig(&registries, &without_parts),
        None
    );

    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    advance_to(&registries, &mut state, 205);
    advance_to(&registries, &mut replay, 205);
    advance_to(&registries, &mut without_parts, 205);
    assert_eq!(state, replay);
    let warned = state.kobold_state().colony.as_ref().unwrap();
    assert_eq!(
        warned.scout_step, 0,
        "the nearby scout physically retreated home"
    );
    assert!(warned.wary_until.is_some());
    assert_eq!(
        without_parts
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_step,
        2,
        "without the salvaged apparatus the far scout continues normally"
    );
    assert_eq!(
        state.surface().get(core).unwrap().remaining_mass(),
        Mass::from_milligrams(50_000)
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    relocate_player_for_test(&mut state, VoxelCoord::new(15, 0, 15));
    assert_eq!(available_nearby_kobold_junk_rig(&registries, &state), None);
}

#[test]
fn a_better_timber_ratchet_keeps_a_distinct_working_cycle() {
    let (registries, mut state) = mobilized_den();
    put_drop(
        &registries,
        &mut state,
        CommodityKey::new(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING),
    );
    put_drop(
        &registries,
        &mut state,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
    );
    let handle = put_drop(
        &registries,
        &mut state,
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
    );
    validate_admit_player(&registries, &state, OBSERVER)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let rig = available_nearby_kobold_junk_rig(&registries, &state).unwrap();
    assert_eq!(rig.striker(), handle);
    assert_eq!(rig.kind(), KoboldJunkRigKind::TimberRatchet);
    assert!(!rig.sounds_at(SimulationTick::new(205)));
    assert!(rig.sounds_at(SimulationTick::new(225)));
}
