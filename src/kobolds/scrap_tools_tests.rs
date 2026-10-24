//! A lower-caste scout builds useful work capability out of recoverable scrap.

use super::*;
use crate::content::{
    FORM_INGOT, FORM_LUMP, FORM_REINFORCEMENT, FORM_SCRAP, MATERIAL_IRON, build_registries,
};
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
fn scavenger_fits_recovered_iron_bearing_scrap_and_conserves_its_abrasion() {
    let registries = build_registries();
    let mut state = AppState::new();
    for (a, b) in [(DEN, JUNCTION), (JUNCTION, TIP)] {
        add_traversable_edge_for_test(&mut state, a, b);
    }
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
    let stone = drop_at(
        &registries,
        &mut state,
        JUNCTION,
        MATERIAL_STONE,
        FORM_CHIP,
        Mass::from_milligrams(50_000),
    );
    let bulk = drop_at(
        &registries,
        &mut state,
        JUNCTION,
        MATERIAL_STONE,
        FORM_LUMP,
        Mass::from_milligrams(2_000_000),
    );
    // Temple bearing housings yield this actual material/form, not an ingot.
    let recovered = drop_at(
        &registries,
        &mut state,
        TIP,
        MATERIAL_IRON,
        FORM_SCRAP,
        Mass::from_milligrams(50_000),
    );
    let initial = calculate_matter_accounting(&state).unwrap().total();
    advance_to(&registries, &mut state, 180);
    let mut guarded = state.clone();
    validate_admit_player(&registries, &guarded, TIP)
        .unwrap()
        .commit(&mut guarded)
        .unwrap();
    advance_to(&registries, &mut guarded, 195);
    assert_eq!(guarded.surface().get(recovered).unwrap().position(), TIP);
    advance_to(&registries, &mut state, 185);
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        TIP
    );
    assert_eq!(
        state.surface().get(bulk).unwrap().remaining_mass(),
        Mass::from_milligrams(2_000_000)
    );
    advance_to(&registries, &mut state, 190);
    assert_eq!(state.surface().get(recovered).unwrap().position(), JUNCTION);
    assert_eq!(
        current_junction_scrap_lever(&state, state.kobold_state().colony.as_ref().unwrap())
            .unwrap()
            .head(),
        stone,
        "scavenged scrap still needs a two-step physical fitting"
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    advance_to(&registries, &mut state, 195);
    advance_to(&registries, &mut replay, 195);
    assert_eq!(state, replay);
    let work = state
        .kobold_state()
        .colony
        .as_ref()
        .unwrap()
        .pry_fitting
        .unwrap();
    assert_eq!(
        (work.handle, work.head, work.completed_at),
        (handle, recovered, None)
    );
    advance_to(&registries, &mut state, 200);
    advance_to(&registries, &mut replay, 200);
    assert_eq!(state, replay);
    let kit = current_junction_scrap_lever(&state, state.kobold_state().colony.as_ref().unwrap())
        .unwrap();
    assert_eq!(
        (kit.head(), kit.kind()),
        (recovered, KoboldPryHeadKind::IronScrapTooth)
    );
    assert_eq!(
        kit.kind().abrasion_per_split(),
        Mass::from_milligrams(4_000)
    );
    advance_to(&registries, &mut state, 205);
    advance_to(&registries, &mut replay, 205);
    assert_eq!(state, replay);
    assert_eq!(
        state.surface().get(recovered).unwrap().remaining_mass(),
        Mass::from_milligrams(46_000)
    );
    assert_eq!(
        state.surface().get(stone).unwrap().remaining_mass(),
        Mass::from_milligrams(50_000)
    );
    assert_eq!(
        state.surface().get(bulk).unwrap().remaining_mass(),
        Mass::from_milligrams(1_500_000)
    );
    assert!(
        state
            .surface()
            .available_resources_at(Some(JUNCTION))
            .any(|part| {
                part.id() != recovered
                    && part.commodity() == CommodityKey::new(MATERIAL_IRON, FORM_SCRAP)
                    && part.remaining_mass() == Mass::from_milligrams(4_000)
                    && part.material_provenance()
                        == state
                            .surface()
                            .get(recovered)
                            .unwrap()
                            .material_provenance()
            })
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        initial
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &guarded), Ok(()));
}

#[test]
fn short_iron_scrap_only_strikes_alarms_and_cannot_inherit_fitting() {
    let registries = build_registries();
    let mut state = AppState::new();
    for (a, b) in [(DEN, JUNCTION), (JUNCTION, TIP)] {
        add_traversable_edge_for_test(&mut state, a, b);
    }
    state
        .kobold_state_mut()
        .apply_opening_colony([DEN, JUNCTION, TIP]);
    drop_at(
        &registries,
        &mut state,
        JUNCTION,
        MATERIAL_WOOD,
        FORM_HANDLE,
        Mass::from_milligrams(50_000),
    );
    let small_scrap = drop_at(
        &registries,
        &mut state,
        JUNCTION,
        MATERIAL_IRON,
        FORM_SCRAP,
        Mass::from_milligrams(20_000),
    );
    assert!(
        current_junction_scrap_lever(&state, state.kobold_state().colony.as_ref().unwrap())
            .is_none()
    );
    assert!(!is_needed_outer_tool_part(
        &state,
        TIP,
        state.surface().get(small_scrap).unwrap()
    ));
    let larger_scrap = drop_at(
        &registries,
        &mut state,
        JUNCTION,
        MATERIAL_IRON,
        FORM_SCRAP,
        Mass::from_milligrams(40_000),
    );
    let stone = drop_at(
        &registries,
        &mut state,
        JUNCTION,
        MATERIAL_STONE,
        FORM_CHIP,
        Mass::from_milligrams(50_000),
    );
    advance_to(&registries, &mut state, 190);
    let fitted =
        current_junction_scrap_lever(&state, state.kobold_state().colony.as_ref().unwrap())
            .unwrap();
    assert_eq!(
        (fitted.head(), fitted.kind()),
        (larger_scrap, KoboldPryHeadKind::IronScrapTooth)
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let mut forged = saved;
    forged["state"]["systems"]["kobolds"]["colony"]["pry_fitting"]["head"] =
        serde_json::to_value(small_scrap).unwrap();
    assert!(
        serde_json::from_value::<LoadedSaveEnvelope>(forged)
            .unwrap()
            .into_state(&registries)
            .is_err(),
        "a tiny but alarm-useful iron clapper cannot become a structural tooth"
    );
    let before = calculate_matter_accounting(&state).unwrap().total();
    let bulk = drop_at(
        &registries,
        &mut state,
        JUNCTION,
        MATERIAL_STONE,
        FORM_LUMP,
        Mass::from_milligrams(1_000_000),
    );
    let total = calculate_matter_accounting(&state).unwrap().total();
    let at = state.tick();
    assert!(
        crate::surface::split_bulk_scavenged_drop_with_head_wear(
            &registries,
            &mut state,
            bulk,
            JUNCTION,
            DEN,
            at,
            Mass::from_milligrams(250_000),
            larger_scrap,
            fitted.kind().abrasion_per_split(),
            fitted.kind().abrasion_form(),
        )
        .is_some()
    );
    assert!(
        current_junction_scrap_lever(&state, state.kobold_state().colony.as_ref().unwrap())
            .unwrap()
            .head()
            == stone,
        "the tooth immediately falls below its 40 g usable limit and the stone wedge takes over"
    );
    assert_eq!(
        state.surface().get(larger_scrap).unwrap().remaining_mass(),
        Mass::from_milligrams(36_000)
    );
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), total);
    assert!(before < total);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn new_iron_upgrade_does_not_disable_a_working_copper_tool_before_fitting() {
    let registries = build_registries();
    let mut state = AppState::new();
    for (a, b) in [(DEN, JUNCTION), (JUNCTION, TIP)] {
        add_traversable_edge_for_test(&mut state, a, b);
    }
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
    let stone = drop_at(
        &registries,
        &mut state,
        JUNCTION,
        MATERIAL_STONE,
        FORM_CHIP,
        Mass::from_milligrams(50_000),
    );
    let copper = drop_at(
        &registries,
        &mut state,
        JUNCTION,
        MATERIAL_COPPER,
        FORM_INGOT,
        Mass::from_milligrams(50_000),
    );
    advance_to(&registries, &mut state, 190);
    assert_eq!(
        current_junction_scrap_lever(&state, state.kobold_state().colony.as_ref().unwrap())
            .unwrap()
            .head(),
        copper
    );
    let iron = drop_at(
        &registries,
        &mut state,
        JUNCTION,
        MATERIAL_IRON,
        FORM_INGOT,
        Mass::from_milligrams(50_000),
    );
    let before = calculate_matter_accounting(&state).unwrap().total();
    assert_eq!(
        current_junction_scrap_lever(&state, state.kobold_state().colony.as_ref().unwrap())
            .unwrap()
            .head(),
        copper,
        "an unfitted replacement must not disable an existing completed pair"
    );
    assert!(is_staged_scrap_tool_part(
        &state,
        state.surface().get(copper).unwrap()
    ));
    assert!(is_staged_scrap_tool_part(
        &state,
        state.surface().get(iron).unwrap()
    ));
    // Player pressure postpones the retrofit. The installed copper head must
    // not silently lose its prior work while the scout avoids danger.
    let mut guarded = state.clone();
    validate_admit_player(&registries, &guarded, TIP)
        .unwrap()
        .commit(&mut guarded)
        .unwrap();
    advance_to(&registries, &mut guarded, 195);
    assert_eq!(
        current_junction_scrap_lever(&guarded, guarded.kobold_state().colony.as_ref().unwrap())
            .unwrap()
            .head(),
        copper
    );
    assert_eq!(validate_loaded_state(&registries, &guarded), Ok(()));
    let save = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&save)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    advance_to(&registries, &mut state, 195);
    advance_to(&registries, &mut replay, 195);
    assert_eq!(state, replay);
    let fitted =
        current_junction_scrap_lever(&state, state.kobold_state().colony.as_ref().unwrap())
            .unwrap();
    assert_eq!(
        fitted.head(),
        stone,
        "retooling occupies the first real work slot"
    );
    advance_to(&registries, &mut state, 200);
    advance_to(&registries, &mut replay, 200);
    assert_eq!(state, replay);
    let fitted =
        current_junction_scrap_lever(&state, state.kobold_state().colony.as_ref().unwrap())
            .unwrap();
    assert_eq!(
        (fitted.handle(), fitted.head(), fitted.kind()),
        (handle, iron, KoboldPryHeadKind::IronShoe)
    );
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), before);
    assert!(crate::surface::relocate_scavenged_drop_step(
        &registries,
        &mut state,
        iron,
        JUNCTION,
        TIP
    ));
    let fallback =
        current_junction_scrap_lever(&state, state.kobold_state().colony.as_ref().unwrap())
            .unwrap();
    assert_eq!(
        fallback.head(),
        stone,
        "the old copper head does not inherit the iron fitting"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn iron_work_shoe_requires_fitting_and_wears_less_than_copper_on_real_bulk() {
    let registries = build_registries();
    let mut state = AppState::new();
    for (from, to) in [(DEN, JUNCTION), (JUNCTION, TIP)] {
        add_traversable_edge_for_test(&mut state, from, to);
    }
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
    let stone = drop_at(
        &registries,
        &mut state,
        JUNCTION,
        MATERIAL_STONE,
        FORM_CHIP,
        Mass::from_milligrams(50_000),
    );
    let bulk = drop_at(
        &registries,
        &mut state,
        JUNCTION,
        MATERIAL_STONE,
        FORM_LUMP,
        Mass::from_milligrams(2_000_000),
    );
    let iron = drop_at(
        &registries,
        &mut state,
        TIP,
        MATERIAL_IRON,
        FORM_INGOT,
        Mass::from_milligrams(50_000),
    );
    let initial = calculate_matter_accounting(&state).unwrap().total();
    advance_to(&registries, &mut state, 180);
    let mut guarded = state.clone();
    validate_admit_player(&registries, &guarded, TIP)
        .unwrap()
        .commit(&mut guarded)
        .unwrap();
    advance_to(&registries, &mut guarded, 195);
    assert_eq!(guarded.surface().get(iron).unwrap().position(), TIP);
    assert_eq!(
        current_junction_scrap_lever(&guarded, guarded.kobold_state().colony.as_ref().unwrap())
            .unwrap()
            .kind(),
        KoboldPryHeadKind::StoneWedge
    );
    advance_to(&registries, &mut state, 185);
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        TIP,
        "the scout must pass exposed bulk for a reachable upgrade"
    );
    advance_to(&registries, &mut state, 190);
    assert_eq!(state.surface().get(iron).unwrap().position(), JUNCTION);
    assert_eq!(
        current_junction_scrap_lever(&state, state.kobold_state().colony.as_ref().unwrap())
            .unwrap()
            .head(),
        stone,
        "loose iron is not a fitted tool"
    );
    let snapshot = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&snapshot)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    advance_to(&registries, &mut state, 195);
    advance_to(&registries, &mut replay, 195);
    assert_eq!(state, replay);
    let work = state
        .kobold_state()
        .colony
        .as_ref()
        .unwrap()
        .pry_fitting
        .unwrap();
    assert_eq!(
        (work.handle, work.head, work.completed_at),
        (handle, iron, None)
    );
    let mut witness = state.clone();
    validate_admit_player(&registries, &witness, DEN)
        .unwrap()
        .commit(&mut witness)
        .unwrap();
    assert_eq!(
        available_nearby_kobold_pry_work(&registries, &witness)
            .unwrap()
            .head(),
        iron
    );
    advance_to(&registries, &mut state, 200);
    advance_to(&registries, &mut replay, 200);
    assert_eq!(state, replay);
    let shoe = current_junction_scrap_lever(&state, state.kobold_state().colony.as_ref().unwrap())
        .unwrap();
    assert_eq!(
        (shoe.handle(), shoe.head(), shoe.kind()),
        (handle, iron, KoboldPryHeadKind::IronShoe)
    );
    assert_eq!(
        shoe.kind().abrasion_per_split(),
        Mass::from_milligrams(2_000)
    );
    assert_eq!(shoe.kind().abrasion_form(), FORM_SCRAP);
    advance_to(&registries, &mut state, 205);
    advance_to(&registries, &mut replay, 205);
    assert_eq!(state, replay);
    assert_eq!(
        state.surface().get(iron).unwrap().remaining_mass(),
        Mass::from_milligrams(48_000)
    );
    assert_eq!(
        state.surface().get(stone).unwrap().remaining_mass(),
        Mass::from_milligrams(50_000)
    );
    assert_eq!(
        state.surface().get(bulk).unwrap().remaining_mass(),
        Mass::from_milligrams(1_500_000)
    );
    assert!(
        state
            .surface()
            .available_resources_at(Some(JUNCTION))
            .any(|resource| {
                resource.commodity() == CommodityKey::new(MATERIAL_IRON, FORM_SCRAP)
                    && resource.remaining_mass() == Mass::from_milligrams(2_000)
                    && resource.material_provenance()
                        == state.surface().get(iron).unwrap().material_provenance()
            })
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        initial
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &guarded), Ok(()));
}

#[test]
fn forged_iron_pry_fitting_rejects_wrong_form_and_stale_work_cannot_rebind() {
    let registries = build_registries();
    let mut state = AppState::new();
    for (a, b) in [(DEN, JUNCTION), (JUNCTION, TIP)] {
        add_traversable_edge_for_test(&mut state, a, b);
    }
    state
        .kobold_state_mut()
        .apply_opening_colony([DEN, JUNCTION, TIP]);
    drop_at(
        &registries,
        &mut state,
        JUNCTION,
        MATERIAL_WOOD,
        FORM_HANDLE,
        Mass::from_milligrams(50_000),
    );
    let iron = drop_at(
        &registries,
        &mut state,
        JUNCTION,
        MATERIAL_IRON,
        FORM_INGOT,
        Mass::from_milligrams(50_000),
    );
    let _copper = drop_at(
        &registries,
        &mut state,
        JUNCTION,
        MATERIAL_COPPER,
        FORM_INGOT,
        Mass::from_milligrams(50_000),
    );
    let unsuitable = drop_at(
        &registries,
        &mut state,
        JUNCTION,
        MATERIAL_IRON,
        FORM_SCRAP,
        Mass::from_milligrams(20_000),
    );
    advance_to(&registries, &mut state, 190);
    assert_eq!(
        current_junction_scrap_lever(&state, state.kobold_state().colony.as_ref().unwrap())
            .unwrap()
            .head(),
        iron
    );
    let saved = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let mut forged = saved.clone();
    forged["state"]["systems"]["kobolds"]["colony"]["pry_fitting"]["head"] =
        serde_json::to_value(unsuitable).unwrap();
    assert!(
        serde_json::from_value::<LoadedSaveEnvelope>(forged)
            .unwrap()
            .into_state(&registries)
            .is_err(),
        "a loose iron scrap clapper cannot be forged into a fitted solid pry shoe"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    assert!(crate::surface::relocate_scavenged_drop_step(
        &registries,
        &mut state,
        iron,
        JUNCTION,
        TIP
    ));
    assert!(
        current_junction_scrap_lever(&state, state.kobold_state().colony.as_ref().unwrap())
            .is_none(),
        "the remaining unfitted copper must not inherit iron's completed labor"
    );
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
    assert_eq!((tool.handle(), tool.head()), (handle, wedge));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn only_active_tool_parts_and_one_stone_backup_are_reserved_from_scavenging() {
    let registries = build_registries();
    let mut state = AppState::new();
    add_traversable_edge_for_test(&mut state, DEN, JUNCTION);
    add_traversable_edge_for_test(&mut state, JUNCTION, TIP);
    state
        .kobold_state_mut()
        .apply_opening_colony([DEN, JUNCTION, TIP]);
    let parts = [
        (MATERIAL_WOOD, FORM_HANDLE),
        (MATERIAL_STONE, FORM_CHIP),
        (MATERIAL_STONE, FORM_CHIP),
        (MATERIAL_COPPER, FORM_INGOT),
        (MATERIAL_COPPER, FORM_INGOT),
    ]
    .map(|(material, form)| {
        drop_at(
            &registries,
            &mut state,
            JUNCTION,
            material,
            form,
            Mass::from_milligrams(50_000),
        )
    });
    assert_eq!(
        current_junction_scrap_lever(&state, state.kobold_state().colony.as_ref().unwrap())
            .unwrap()
            .head(),
        parts[1],
        "unfitted metal is only a staged component, not a finished pry head"
    );
    for (index, id) in parts.into_iter().enumerate() {
        let record = state.surface().get(id).unwrap();
        assert_eq!(
            is_staged_scrap_tool_part(&state, record),
            [true, true, false, true, false][index],
            "only one currently useful head, one fallback, and the handle should be staged"
        );
    }
    advance_to(&registries, &mut state, 190);
    assert_eq!(
        current_junction_scrap_lever(&state, state.kobold_state().colony.as_ref().unwrap())
            .unwrap()
            .head(),
        parts[3],
        "completed real fitting activates only the first selected ingot"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn copper_fulcrum_beats_older_stone_but_loose_copper_scrap_does_not_make_a_head() {
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
    let stone = drop_at(
        &registries,
        &mut state,
        JUNCTION,
        MATERIAL_STONE,
        FORM_CHIP,
        Mass::from_milligrams(50_000),
    );
    let loose_copper = drop_at(
        &registries,
        &mut state,
        JUNCTION,
        MATERIAL_COPPER,
        FORM_SCRAP,
        Mass::from_milligrams(50_000),
    );
    let stone_tool =
        current_junction_scrap_lever(&state, state.kobold_state().colony.as_ref().unwrap())
            .unwrap();
    assert_eq!(
        (stone_tool.head(), stone_tool.kind()),
        (stone, KoboldPryHeadKind::StoneWedge)
    );
    let _inert_relay_core = drop_at(
        &registries,
        &mut state,
        JUNCTION,
        MATERIAL_COPPER,
        FORM_REINFORCEMENT,
        Mass::from_milligrams(50_000),
    );
    assert_eq!(
        current_junction_scrap_lever(&state, state.kobold_state().colony.as_ref().unwrap())
            .unwrap()
            .head(),
        stone,
        "precision copper without a working den alarm does not disable local work"
    );
    let copper = drop_at(
        &registries,
        &mut state,
        JUNCTION,
        MATERIAL_COPPER,
        FORM_INGOT,
        Mass::from_milligrams(50_000),
    );
    assert_eq!(
        current_junction_scrap_lever(&state, state.kobold_state().colony.as_ref().unwrap())
            .unwrap()
            .head(),
        stone,
        "merely placing copper cannot immediately grant a fitted tool"
    );
    advance_to(&registries, &mut state, 190);
    let durable_tool =
        current_junction_scrap_lever(&state, state.kobold_state().colony.as_ref().unwrap())
            .unwrap();
    assert_eq!(
        (
            durable_tool.handle(),
            durable_tool.head(),
            durable_tool.kind()
        ),
        (handle, copper, KoboldPryHeadKind::CopperFulcrum)
    );
    assert_ne!(durable_tool.head(), loose_copper);
    assert_eq!(
        durable_tool.kind().abrasion_per_split(),
        Mass::from_milligrams(5_000)
    );
    assert_eq!(
        stone_tool.kind().abrasion_per_split(),
        Mass::from_milligrams(10_000)
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn copper_fitting_uses_two_interrupted_or_completed_local_work_slots() {
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
    let stone = drop_at(
        &registries,
        &mut state,
        JUNCTION,
        MATERIAL_STONE,
        FORM_CHIP,
        Mass::from_milligrams(50_000),
    );
    let head = drop_at(
        &registries,
        &mut state,
        JUNCTION,
        MATERIAL_COPPER,
        FORM_INGOT,
        Mass::from_milligrams(50_000),
    );
    let total = calculate_matter_accounting(&state).unwrap().total();
    advance_to(&registries, &mut state, 180);
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        JUNCTION
    );
    assert_eq!(
        state.kobold_state().colony.as_ref().unwrap().pry_fitting,
        None
    );
    advance_to(&registries, &mut state, 185);
    let work = state
        .kobold_state()
        .colony
        .as_ref()
        .unwrap()
        .pry_fitting
        .unwrap();
    assert_eq!(
        (
            work.handle,
            work.head,
            work.started_at.value(),
            work.completed_at
        ),
        (handle, head, 185, None)
    );
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        JUNCTION
    );
    assert_eq!(
        current_junction_scrap_lever(&state, state.kobold_state().colony.as_ref().unwrap())
            .unwrap()
            .head(),
        stone
    );
    assert!(
        available_nearby_kobold_pry_work(&registries, &state).is_none(),
        "no player present means no remotely disclosed fitting process"
    );

    let mut observer = state.clone();
    validate_admit_player(&registries, &observer, VoxelCoord::new(-2, 0, 0))
        .unwrap()
        .commit(&mut observer)
        .unwrap();
    let sighting = available_nearby_kobold_pry_work(&registries, &observer)
        .expect("nearby observers should see the scout fitting a real material pair");
    assert_eq!(
        (sighting.position(), sighting.handle(), sighting.head()),
        (JUNCTION, handle, head)
    );
    assert_eq!(sighting.started_at().value(), 185);

    let saved = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_value::<LoadedSaveEnvelope>(saved.clone())
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let mut tampered = saved.clone();
    tampered["state"]["systems"]["kobolds"]["colony"]["pry_fitting"]["started_at"] =
        serde_json::json!(0_u64);
    let corrupted = serde_json::from_value::<LoadedSaveEnvelope>(tampered).unwrap();
    assert!(
        corrupted.into_state(&registries).is_err(),
        "a forged impossible start must be rejected at the load boundary"
    );
    let mut missing = saved;
    missing["state"]["systems"]["kobolds"]["colony"]
        .as_object_mut()
        .unwrap()
        .remove("pry_fitting");
    assert!(
        serde_json::from_value::<LoadedSaveEnvelope>(missing).is_err(),
        "the current save schema requires an explicit fitting state"
    );

    let mut guarded = state.clone();
    validate_admit_player(&registries, &guarded, TIP)
        .unwrap()
        .commit(&mut guarded)
        .unwrap();
    advance_to(&registries, &mut guarded, 190);
    assert!(
        guarded
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .pry_fitting
            .is_none(),
        "a living player near the worksite should cancel unfinished labor"
    );
    assert_eq!(
        current_junction_scrap_lever(&guarded, guarded.kobold_state().colony.as_ref().unwrap())
            .unwrap()
            .head(),
        stone
    );
    assert_eq!(
        calculate_matter_accounting(&guarded).unwrap().total(),
        total
    );

    advance_to(&registries, &mut state, 190);
    advance_to(&registries, &mut replay, 190);
    assert_eq!(state, replay);
    let assembled =
        current_junction_scrap_lever(&state, state.kobold_state().colony.as_ref().unwrap())
            .unwrap();
    assert_eq!(
        (assembled.handle(), assembled.head(), assembled.kind()),
        (handle, head, KoboldPryHeadKind::CopperFulcrum)
    );
    assert!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .pry_fitting
            .unwrap()
            .completed_at
            .is_some()
    );
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        JUNCTION
    );
    assert!(available_nearby_kobold_pry_work(&registries, &observer).is_some());
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), total);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &guarded), Ok(()));

    relocate_player_for_test(&mut guarded, VoxelCoord::new(15, 0, 15));
    advance_to(&registries, &mut guarded, 240);
    assert_eq!(
        current_junction_scrap_lever(&guarded, guarded.kobold_state().colony.as_ref().unwrap())
            .unwrap()
            .kind(),
        KoboldPryHeadKind::CopperFulcrum,
        "once the player leaves, the scout can return and perform the lost work"
    );
    assert_eq!(
        calculate_matter_accounting(&guarded).unwrap().total(),
        total
    );
}

#[test]
fn spent_copper_fulcrum_falls_back_to_the_existing_stone_head() {
    let registries = build_registries();
    let mut state = AppState::new();
    add_traversable_edge_for_test(&mut state, DEN, JUNCTION);
    add_traversable_edge_for_test(&mut state, JUNCTION, TIP);
    state
        .kobold_state_mut()
        .apply_opening_colony([DEN, JUNCTION, TIP]);
    let _handle = drop_at(
        &registries,
        &mut state,
        JUNCTION,
        MATERIAL_WOOD,
        FORM_HANDLE,
        Mass::from_milligrams(50_000),
    );
    let stone = drop_at(
        &registries,
        &mut state,
        JUNCTION,
        MATERIAL_STONE,
        FORM_CHIP,
        Mass::from_milligrams(50_000),
    );
    let ingot = drop_at(
        &registries,
        &mut state,
        JUNCTION,
        MATERIAL_COPPER,
        FORM_INGOT,
        Mass::from_milligrams(50_000),
    );
    let spare_ingot = drop_at(
        &registries,
        &mut state,
        JUNCTION,
        MATERIAL_COPPER,
        FORM_INGOT,
        Mass::from_milligrams(50_000),
    );
    let bulk = drop_at(
        &registries,
        &mut state,
        JUNCTION,
        MATERIAL_STONE,
        FORM_LUMP,
        Mass::from_milligrams(750_000),
    );
    let initial = calculate_matter_accounting(&state).unwrap().total();
    advance_to(&registries, &mut state, 190);
    for _ in 0..7 {
        let tool =
            current_junction_scrap_lever(&state, state.kobold_state().colony.as_ref().unwrap())
                .unwrap();
        assert_eq!(
            (tool.head(), tool.kind()),
            (ingot, KoboldPryHeadKind::CopperFulcrum)
        );
        let at = state.tick();
        assert!(
            crate::surface::split_bulk_scavenged_drop_with_head_wear(
                &registries,
                &mut state,
                bulk,
                JUNCTION,
                DEN,
                at,
                Mass::from_milligrams(50_000),
                tool.head(),
                tool.kind().abrasion_per_split(),
                tool.kind().abrasion_form(),
            )
            .is_some()
        );
    }
    let after = current_junction_scrap_lever(&state, state.kobold_state().colony.as_ref().unwrap())
        .expect("the original stone head remains useful after copper deforms");
    assert_eq!(
        (after.head(), after.kind()),
        (stone, KoboldPryHeadKind::StoneWedge),
        "the physically staged replacement cannot inherit the old ingot's fitting"
    );
    assert_eq!(
        state.surface().get(ingot).unwrap().remaining_mass(),
        Mass::from_milligrams(15_000)
    );
    assert_eq!(
        state.surface().get(stone).unwrap().remaining_mass(),
        Mass::from_milligrams(50_000)
    );
    let debris = state
        .surface()
        .available_resources_at(Some(JUNCTION))
        .filter(|resource| resource.commodity() == CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP))
        .collect::<Vec<_>>();
    assert_eq!(debris.len(), 7);
    assert!(debris.iter().all(|resource| {
        resource.remaining_mass() == Mass::from_milligrams(5_000)
            && resource.material_provenance()
                == state.surface().get(ingot).unwrap().material_provenance()
    }));
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        initial
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(loaded, state);
    advance_to(&registries, &mut state, 195);
    assert_eq!(
        current_junction_scrap_lever(&state, state.kobold_state().colony.as_ref().unwrap())
            .unwrap()
            .head(),
        stone,
        "replacement work begins without immediately upgrading"
    );
    advance_to(&registries, &mut state, 200);
    assert_eq!(
        current_junction_scrap_lever(&state, state.kobold_state().colony.as_ref().unwrap())
            .unwrap()
            .head(),
        spare_ingot,
        "the replacement earns its own two-step installation"
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        initial
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn scout_upgrades_a_stone_kit_from_a_reachable_copper_fulcrum_and_wears_only_the_metal() {
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
    let original_stone = drop_at(
        &registries,
        &mut state,
        JUNCTION,
        MATERIAL_STONE,
        FORM_CHIP,
        Mass::from_milligrams(50_000),
    );
    let bulk = drop_at(
        &registries,
        &mut state,
        JUNCTION,
        MATERIAL_STONE,
        FORM_LUMP,
        Mass::from_milligrams(2_000_000),
    );
    let ingot = drop_at(
        &registries,
        &mut state,
        TIP,
        MATERIAL_COPPER,
        FORM_INGOT,
        Mass::from_milligrams(50_000),
    );
    let before = calculate_matter_accounting(&state).unwrap().total();
    advance_to(&registries, &mut state, 180);
    let mut guarded = state.clone();
    validate_admit_player(&registries, &guarded, TIP)
        .unwrap()
        .commit(&mut guarded)
        .unwrap();
    advance_to(&registries, &mut guarded, 195);
    assert_eq!(guarded.surface().get(ingot).unwrap().position(), TIP);
    assert_ne!(
        current_junction_scrap_lever(&guarded, guarded.kobold_state().colony.as_ref().unwrap())
            .unwrap()
            .kind(),
        KoboldPryHeadKind::CopperFulcrum,
    );
    assert_eq!(
        calculate_matter_accounting(&guarded).unwrap().total(),
        before
    );

    advance_to(&registries, &mut state, 185);
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        TIP,
        "a fed scout should bypass nearby bulk for a visible, useful copper upgrade"
    );
    assert_eq!(
        state.surface().get(bulk).unwrap().remaining_mass(),
        Mass::from_milligrams(2_000_000),
        "the scout must not split bulk without performing that work at the junction"
    );
    let save = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&save)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    advance_to(&registries, &mut state, 195);
    advance_to(&registries, &mut replay, 195);
    assert_eq!(state, replay);
    assert_eq!(
        current_junction_scrap_lever(&state, state.kobold_state().colony.as_ref().unwrap())
            .unwrap()
            .kind(),
        KoboldPryHeadKind::StoneWedge,
        "first fitting slot cannot instantly activate copper"
    );
    advance_to(&registries, &mut state, 200);
    advance_to(&registries, &mut replay, 200);
    assert_eq!(state, replay);
    assert_eq!(
        current_junction_scrap_lever(&state, state.kobold_state().colony.as_ref().unwrap())
            .unwrap()
            .kind(),
        KoboldPryHeadKind::CopperFulcrum,
    );
    assert_eq!(
        state.surface().get(ingot).unwrap().remaining_mass(),
        Mass::from_milligrams(50_000)
    );
    advance_to(&registries, &mut state, 205);
    advance_to(&registries, &mut replay, 205);
    assert_eq!(state, replay);
    let upgraded =
        current_junction_scrap_lever(&state, state.kobold_state().colony.as_ref().unwrap())
            .expect("the scout physically delivered the recovered ingot to the worksite");
    assert_eq!(
        (upgraded.handle(), upgraded.head(), upgraded.kind()),
        (handle, ingot, KoboldPryHeadKind::CopperFulcrum)
    );
    assert_eq!(
        state.surface().get(ingot).unwrap().remaining_mass(),
        Mass::from_milligrams(45_000),
        "the ingot head must abrade by its own 5 g cost"
    );
    assert_eq!(
        state
            .surface()
            .get(original_stone)
            .unwrap()
            .remaining_mass(),
        Mass::from_milligrams(50_000),
        "the unused stone wedge must stay intact as a fallback"
    );
    assert_eq!(
        state.surface().get(bulk).unwrap().remaining_mass(),
        Mass::from_milligrams(1_500_000),
        "the improved worksite must still haul only a den-fitting batch"
    );
    let copper_residue = state
        .surface()
        .available_resources_at(Some(JUNCTION))
        .find(|resource| {
            resource.id() != ingot
                && resource.commodity() == CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP)
                && resource.remaining_mass() == Mass::from_milligrams(5_000)
        })
        .expect("spent copper must remain conserved at the worksite");
    assert_eq!(
        copper_residue.material_provenance(),
        state.surface().get(ingot).unwrap().material_provenance()
    );
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), before);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn worn_out_wedge_prompts_a_real_repair_trip_past_nearby_bulk() {
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
    let old_wedge = drop_at(
        &registries,
        &mut state,
        JUNCTION,
        MATERIAL_STONE,
        FORM_CHIP,
        Mass::from_milligrams(50_000),
    );
    let bulk = drop_at(
        &registries,
        &mut state,
        JUNCTION,
        MATERIAL_STONE,
        FORM_LUMP,
        Mass::from_milligrams(1_000_000),
    );
    let total = calculate_matter_accounting(&state).unwrap().total();
    for _ in 0..4 {
        let at = state.tick();
        assert!(
            crate::surface::split_bulk_scavenged_drop_with_head_wear(
                &registries,
                &mut state,
                bulk,
                JUNCTION,
                DEN,
                at,
                Mass::from_milligrams(100_000),
                old_wedge,
                WEDGE_ABRASION_PER_SPLIT,
                FORM_CHIP,
            )
            .is_some()
        );
    }
    assert_eq!(
        state.surface().get(old_wedge).unwrap().remaining_mass(),
        Mass::from_milligrams(10_000)
    );
    assert_eq!(
        state.surface().get(bulk).unwrap().remaining_mass(),
        Mass::from_milligrams(600_000)
    );
    assert!(
        current_junction_scrap_lever(&state, state.kobold_state().colony.as_ref().unwrap())
            .is_none(),
        "a wedge below the work threshold must stop granting its advantage"
    );
    let spare = drop_at(
        &registries,
        &mut state,
        TIP,
        MATERIAL_STONE,
        FORM_CHIP,
        Mass::from_milligrams(50_000),
    );
    let with_replacement = calculate_matter_accounting(&state).unwrap().total();
    assert_eq!(
        with_replacement,
        total
            .checked_add(crate::core::quantity::AggregateMass::from_milligrams(
                50_000
            ))
            .unwrap()
    );
    advance_to(&registries, &mut state, 180);
    let mut guarded = state.clone();
    validate_admit_player(&registries, &guarded, TIP)
        .unwrap()
        .commit(&mut guarded)
        .unwrap();
    advance_to(&registries, &mut guarded, 190);
    assert_eq!(
        guarded.surface().get(spare).unwrap().position(),
        TIP,
        "a real player holding the outer route must prevent the repair delivery"
    );
    assert!(
        current_junction_scrap_lever(&guarded, guarded.kobold_state().colony.as_ref().unwrap())
            .is_none()
    );
    assert_eq!(
        calculate_matter_accounting(&guarded).unwrap().total(),
        with_replacement
    );
    assert_eq!(validate_loaded_state(&registries, &guarded), Ok(()));

    advance_to(&registries, &mut state, 185);
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        TIP,
        "the scout must bypass available nearby bulk to fetch a visible repair part"
    );
    let save = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&save)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    advance_to(&registries, &mut state, 190);
    advance_to(&registries, &mut replay, 190);
    assert_eq!(replay, state);
    assert_eq!(state.surface().get(spare).unwrap().position(), JUNCTION);
    let repaired =
        current_junction_scrap_lever(&state, state.kobold_state().colony.as_ref().unwrap())
            .expect("a genuinely delivered replacement must restore the same physical worksite");
    assert_eq!((repaired.handle(), repaired.head()), (handle, spare));
    assert_eq!(
        state.surface().get(old_wedge).unwrap().remaining_mass(),
        Mass::from_milligrams(10_000),
        "wear debris and the old worn wedge remain recoverable, not silently repaired"
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        with_replacement
    );
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
        (lever.position(), lever.handle(), lever.head()),
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
        world.surface().get(wedge).unwrap().remaining_mass(),
        Mass::from_milligrams(40_000),
        "each assisted split wears ten grams off the original wedge"
    );
    let debris = world
        .surface()
        .available_resources_at(Some(JUNCTION))
        .find(|resource| {
            resource.id() != wedge
                && resource.id() != handle
                && resource.id() != source_with
                && resource.commodity() == CommodityKey::new(MATERIAL_STONE, FORM_CHIP)
                && resource.remaining_mass() == Mass::from_milligrams(10_000)
        })
        .expect("the wedge's abrasion must remain recoverable at the real worksite");
    assert_eq!(
        debris.material_profile(),
        world.surface().get(wedge).unwrap().material_profile()
    );
    assert_eq!(
        debris.material_provenance(),
        world.surface().get(wedge).unwrap().material_provenance()
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
    let persisted = serde_json::to_vec(&SaveEnvelope::new(&registries, &world)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&persisted)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(
        loaded, world,
        "tool abrasion and its debris must persist together"
    );
}
