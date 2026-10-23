//! A lower-caste scout builds useful work capability out of recoverable scrap.

use super::*;
use crate::content::{FORM_INGOT, FORM_LUMP, FORM_REINFORCEMENT, FORM_SCRAP, build_registries};
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
        parts[3],
    );
    for (index, id) in parts.into_iter().enumerate() {
        let record = state.surface().get(id).unwrap();
        assert_eq!(
            is_staged_scrap_tool_part(&state, record),
            [true, true, false, true, false][index],
            "only one currently useful head, one fallback, and the handle should be staged"
        );
    }
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
    let bulk = drop_at(
        &registries,
        &mut state,
        JUNCTION,
        MATERIAL_STONE,
        FORM_LUMP,
        Mass::from_milligrams(750_000),
    );
    let initial = calculate_matter_accounting(&state).unwrap().total();
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
        (stone, KoboldPryHeadKind::StoneWedge)
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
