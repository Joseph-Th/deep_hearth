//! A lower-caste scout builds useful work capability out of recoverable scrap.

use super::*;
use crate::content::{FORM_LUMP, build_registries};
use crate::core::quantity::Temperature;
use crate::core::state::validate_loaded_state;
use crate::logistics::{add_traversable_edge_for_test, relocate_player_for_test};
use crate::material::{CommodityKey, MaterialComposition};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::{advance_control_tick, advance_tick};
use crate::surface::{SurfaceResourceSpec, validate_create_surface_resources};

const DEN: VoxelCoord = VoxelCoord::new(-1, 0, 0);
const JUNCTION: VoxelCoord = VoxelCoord::new(0, 0, 0);
const TIP: VoxelCoord = VoxelCoord::new(1, 0, 0);
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

fn drop_at(
    registries: &Registries,
    state: &mut AppState,
    position: VoxelCoord,
    material: crate::material::MaterialId,
    form: crate::material::FormId,
    mass: Mass,
) -> SurfaceResourceId {
    let spec = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        position,
        CommodityKey::new(material, form),
        mass,
        TEMPERATURE,
        MaterialComposition::pure(material),
    )
    .unwrap();
    let planned = validate_create_surface_resources(registries, state, [spec]).unwrap();
    let id = planned.first_resource_id();
    planned.commit(state).unwrap();
    id
}

#[test]
fn a_junction_tool_needs_two_separate_portable_and_local_parts() {
    let registries = build_registries();
    let mut state = AppState::new();
    add_traversable_edge_for_test(&mut state, DEN, JUNCTION);
    add_traversable_edge_for_test(&mut state, JUNCTION, TIP);
    state
        .kobold_state_mut()
        .apply_opening_colony([DEN, JUNCTION, TIP]);
    let handle = drop_at(
        &registries,
        &mut state,
        JUNCTION,
        MATERIAL_WOOD,
        FORM_HANDLE,
        Mass::from_milligrams(50_000),
    );
    let _remote_wedge = drop_at(
        &registries,
        &mut state,
        TIP,
        MATERIAL_STONE,
        FORM_CHIP,
        Mass::from_milligrams(50_000),
    );
    let _oversized_wedge = drop_at(
        &registries,
        &mut state,
        JUNCTION,
        MATERIAL_STONE,
        FORM_CHIP,
        Mass::from_milligrams(1_500_000),
    );
    assert!(
        current_junction_scrap_lever(&state, state.kobold_state().colony.as_ref().unwrap())
            .is_none(),
        "distant or immovable stone cannot act as a local wedge"
    );
    let wedge = drop_at(
        &registries,
        &mut state,
        JUNCTION,
        MATERIAL_STONE,
        FORM_CHIP,
        Mass::from_milligrams(50_000),
    );
    let tool = current_junction_scrap_lever(&state, state.kobold_state().colony.as_ref().unwrap())
        .unwrap();
    assert_eq!((tool.handle(), tool.wedge()), (handle, wedge));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn scout_stages_physical_pry_parts_and_player_interception_reverses_its_effect() {
    let registries = build_registries();
    let mut world = AppState::new();
    add_traversable_edge_for_test(&mut world, DEN, JUNCTION);
    add_traversable_edge_for_test(&mut world, JUNCTION, TIP);
    world
        .kobold_state_mut()
        .apply_opening_colony([DEN, JUNCTION, TIP]);
    advance_to(&registries, &mut world, 200);
    let wedge = drop_at(
        &registries,
        &mut world,
        TIP,
        MATERIAL_STONE,
        FORM_CHIP,
        Mass::from_milligrams(50_000),
    );
    let handle = drop_at(
        &registries,
        &mut world,
        TIP,
        MATERIAL_WOOD,
        FORM_HANDLE,
        Mass::from_milligrams(50_000),
    );
    let initial = calculate_matter_accounting(&world).unwrap().total();
    advance_to(&registries, &mut world, 210);
    assert_eq!(world.surface().get(handle).unwrap().position(), JUNCTION);
    assert_eq!(world.surface().get(wedge).unwrap().position(), TIP);
    assert_eq!(
        world.kobold_state().colony.as_ref().unwrap().scavenged_drop,
        None
    );
    assert!(
        current_junction_scrap_lever(&world, world.kobold_state().colony.as_ref().unwrap())
            .is_none()
    );
    let snapshot = serde_json::to_vec(&SaveEnvelope::new(&registries, &world)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&snapshot)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    advance_to(&registries, &mut world, 235);
    advance_to(&registries, &mut replay, 235);
    assert_eq!(world, replay);
    let lever = current_junction_scrap_lever(&world, world.kobold_state().colony.as_ref().unwrap())
        .expect("two actual return trips must stage the improvised kit");
    assert_eq!(
        (lever.position(), lever.handle(), lever.wedge()),
        (JUNCTION, handle, wedge)
    );
    assert_eq!(
        calculate_matter_accounting(&world).unwrap().total(),
        initial
    );
    assert!(available_nearby_kobold_scrap_lever(&registries, &world).is_none());

    let mut observer = world.clone();
    validate_admit_player(&registries, &observer, VoxelCoord::new(-2, 0, 0))
        .unwrap()
        .commit(&mut observer)
        .unwrap();
    assert_eq!(
        available_nearby_kobold_scrap_lever(&registries, &observer),
        Some(lever)
    );

    let mut intercepted = world.clone();
    validate_admit_player(&registries, &intercepted, JUNCTION)
        .unwrap()
        .commit(&mut intercepted)
        .unwrap();
    let _ = advance_control_tick(&registries, &mut intercepted).unwrap();
    assert!(
        current_junction_scrap_lever(
            &intercepted,
            intercepted.kobold_state().colony.as_ref().unwrap()
        )
        .is_none(),
        "ordinary player pickup breaks the same physical kit"
    );
    assert_eq!(
        calculate_matter_accounting(&intercepted).unwrap().total(),
        initial
    );
    relocate_player_for_test(&mut intercepted, VoxelCoord::new(15, 0, 15));

    let source_with = drop_at(
        &registries,
        &mut world,
        JUNCTION,
        MATERIAL_STONE,
        FORM_LUMP,
        Mass::from_milligrams(2_000_000),
    );
    let source_without = drop_at(
        &registries,
        &mut intercepted,
        JUNCTION,
        MATERIAL_STONE,
        FORM_LUMP,
        Mass::from_milligrams(2_000_000),
    );
    let with_total = calculate_matter_accounting(&world).unwrap().total();
    let without_total = calculate_matter_accounting(&intercepted).unwrap().total();
    advance_to(&registries, &mut world, 245);
    advance_to(&registries, &mut intercepted, 245);
    assert_eq!(
        world.surface().get(source_with).unwrap().remaining_mass(),
        Mass::from_milligrams(1_500_000),
        "lever-backed scout splits 500 g of recoverable bulk"
    );
    assert_eq!(
        intercepted
            .surface()
            .get(source_without)
            .unwrap()
            .remaining_mass(),
        Mass::from_milligrams(1_750_000),
        "intercepting real tools restores the 250 g hand-splitting limit"
    );
    assert_eq!(
        calculate_matter_accounting(&world).unwrap().total(),
        with_total
    );
    assert_eq!(
        calculate_matter_accounting(&intercepted).unwrap().total(),
        without_total
    );
    assert_eq!(validate_loaded_state(&registries, &world), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &intercepted), Ok(()));
}
