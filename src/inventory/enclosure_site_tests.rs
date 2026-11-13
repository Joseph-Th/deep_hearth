//! In-place storage foundations must respect live surface matter and admission revisions.

use super::*;

use crate::content::{FORM_CHIP, MATERIAL_WOOD, STORAGE_ROUGH_TIMBER_FIELD_BOX, build_registries};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::validate_loaded_state;
use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};
use crate::logistics::validate_place_ground_stockpile;
use crate::material::{CommodityKey, MaterialComposition};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::simulation::advance_control_tick;
use crate::surface::{SurfaceResourceKind, SurfaceResourceSpec, validate_create_surface_resources};

const ROOM_TEMPERATURE: Temperature = Temperature::from_millikelvin(293_150);

fn located_enclosure_fixture() -> (Registries, AppState, StockpileId, StockpileId, VoxelCoord) {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(2, 0, 3);
    let definition = registries
        .storage()
        .get(STORAGE_ROUGH_TIMBER_FIELD_BOX)
        .expect("authored field box");
    let target = add_solid_stockpile_for_test(&mut state, definition.maximum_stockpile_capacity())
        .expect("target stockpile");
    let source =
        add_solid_stockpile_for_test(&mut state, definition.assembly_profile().input_mass())
            .expect("construction stockpile");
    for input in definition.assembly_profile().inputs() {
        deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            input.commodity(),
            input.mass(),
            ROOM_TEMPERATURE,
        )
        .expect("staged construction material");
    }
    for stockpile in [target, source] {
        validate_place_ground_stockpile(&state, stockpile, position)
            .expect("ungenerated ground location is permitted")
            .commit(&mut state)
            .expect("ground custody is placed");
    }
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    (registries, state, target, source, position)
}

fn place_exposed_chips(
    registries: &Registries,
    state: &mut AppState,
    position: VoxelCoord,
    mass: Mass,
) {
    let resource = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        position,
        CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
        mass,
        ROOM_TEMPERATURE,
        MaterialComposition::pure(MATERIAL_WOOD),
    )
    .expect("finite physically exposed chips");
    validate_create_surface_resources(registries, state, [resource])
        .expect("chips may be deposited next to ground inventory")
        .commit(state)
        .expect("surface owner accepts the drop");
}

#[test]
fn enclosing_an_existing_cache_requires_the_same_clear_footing_as_placing_a_new_box() {
    let (registries, mut state, target, source, position) = located_enclosure_fixture();
    place_exposed_chips(
        &registries,
        &mut state,
        position,
        Mass::from_milligrams(20_000_000),
    );
    let depth = exposed_residue_compact_depth_um(&registries, &state, position);
    assert!(depth >= OBSTRUCTING_RESIDUE_COMPACT_DEPTH_UM);
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let before = state.clone();
    assert!(matches!(
        validate_build_storage_enclosure(
            &registries,
            &state,
            STORAGE_ROUGH_TIMBER_FIELD_BOX,
            target,
            source
        ),
        Err(StorageEnclosureConstructionError::LooseResidueFooting {
            position: at,
            compact_depth_um,
        }) if at == position && compact_depth_um == depth
    ));
    assert_eq!(state, before);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn pending_in_place_enclosure_cannot_commit_after_a_surface_spill() {
    let (registries, mut state, target, source, position) = located_enclosure_fixture();
    let admitted = validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_ROUGH_TIMBER_FIELD_BOX,
        target,
        source,
    )
    .expect("clear foundation admits construction");
    let expected = state.surface().revision();
    place_exposed_chips(
        &registries,
        &mut state,
        position,
        Mass::from_milligrams(20_000_000),
    );
    let actual = state.surface().revision();
    assert!(actual > expected);
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let before = state.clone();
    assert_eq!(
        admitted.commit(&mut state),
        Err(StorageEnclosureCommitError::StaleSurfaceRevision { expected, actual })
    );
    assert_eq!(
        state, before,
        "stale admission must not spend construction matter"
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
        .unwrap()
        .into_state(&registries)
        .expect("rejected in-place construction leaves a trusted state");
    assert_eq!(
        advance_control_tick(&registries, &mut state).unwrap(),
        advance_control_tick(&registries, &mut replay).unwrap()
    );
    assert_eq!(state, replay);
}

#[test]
fn shallow_residue_still_allows_in_place_enclosure_without_erasing_the_drop() {
    let (registries, mut state, target, source, position) = located_enclosure_fixture();
    place_exposed_chips(
        &registries,
        &mut state,
        position,
        Mass::from_milligrams(10_000_000),
    );
    assert!(
        exposed_residue_compact_depth_um(&registries, &state, position)
            < OBSTRUCTING_RESIDUE_COMPACT_DEPTH_UM
    );
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_ROUGH_TIMBER_FIELD_BOX,
        target,
        source,
    )
    .expect("thin residue does not prohibit construction")
    .commit(&mut state)
    .expect("in-place box consumes only staged construction matter");
    assert!(
        state
            .inventory()
            .get_stockpile(target)
            .unwrap()
            .enclosure()
            .is_some()
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
