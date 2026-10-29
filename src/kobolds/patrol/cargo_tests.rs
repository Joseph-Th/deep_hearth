//! A failed tool-assisted split leaves matter untouched and permits a smaller hand split.

use super::*;
use crate::content::{
    FORM_CHIP, FORM_HANDLE, FORM_LUMP, MATERIAL_STONE, MATERIAL_WOOD, build_registries,
};
use crate::core::quantity::Temperature;
use crate::core::state::validate_loaded_state;
use crate::logistics::add_traversable_edge_for_test;
use crate::material::{CommodityKey, MaterialComposition};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::surface::{SurfaceResourceKind, SurfaceResourceSpec, validate_create_surface_resources};

const DEN: VoxelCoord = VoxelCoord::new(-1, 0, 0);
const JUNCTION: VoxelCoord = VoxelCoord::new(0, 0, 0);
const TIP: VoxelCoord = VoxelCoord::new(1, 0, 0);
const TEMPERATURE: Temperature = Temperature::from_millikelvin(293_150);

fn staged_bulk_with_usable_wedge() -> (Registries, AppState, SurfaceResourceId, SurfaceResourceId) {
    let registries = build_registries();
    let mut state = AppState::new();
    add_traversable_edge_for_test(&mut state, DEN, JUNCTION);
    add_traversable_edge_for_test(&mut state, JUNCTION, TIP);
    state
        .kobold_state_mut()
        .apply_opening_colony([DEN, JUNCTION, TIP]);
    let spec = |material, form, mass| {
        SurfaceResourceSpec::new(
            SurfaceResourceKind::Drop,
            JUNCTION,
            CommodityKey::new(material, form),
            Mass::from_milligrams(mass),
            TEMPERATURE,
            MaterialComposition::pure(material),
        )
        .unwrap()
    };
    let planned = validate_create_surface_resources(
        &registries,
        &state,
        [
            spec(MATERIAL_WOOD, FORM_HANDLE, 50_000),
            spec(MATERIAL_STONE, FORM_CHIP, 50_000),
            spec(MATERIAL_STONE, FORM_LUMP, 2_000_000),
        ],
    )
    .unwrap();
    let ids = planned.resource_ids().collect::<Vec<_>>();
    planned.commit(&mut state).unwrap();
    assert_eq!(
        current_junction_scrap_lever(&state, state.kobold_state().colony.as_ref().unwrap())
            .unwrap()
            .head(),
        ids[1],
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    (registries, state, ids[2], ids[1])
}

fn with_surface_revision(registries: &Registries, state: &AppState, revision: u64) -> AppState {
    let mut saved = serde_json::to_value(SaveEnvelope::new(registries, state)).unwrap();
    let surface = &mut saved["state"]["systems"]["surface"]["revision"];
    assert!(
        surface.is_u64(),
        "surface revision must be an explicit persisted counter"
    );
    *surface = serde_json::json!(revision);
    serde_json::from_value::<LoadedSaveEnvelope>(saved)
        .unwrap()
        .into_state(registries)
        .unwrap()
}

#[test]
fn tool_split_falls_back_to_conserved_hand_split_when_wear_cannot_be_reserved() {
    for limited in [false, true] {
        let (registries, original, bulk, head) = staged_bulk_with_usable_wedge();
        let mut state = if limited {
            with_surface_revision(&registries, &original, u64::MAX - 2)
        } else {
            original
        };
        assert!(state.can_spend_surface_revisions(2));
        assert_eq!(state.can_spend_surface_revisions(3), !limited);
        let conserved = calculate_matter_accounting(&state).unwrap().total();
        let head_before = state.surface().get(head).unwrap().remaining_mass();
        let source_before = state.surface().get(bulk).unwrap().remaining_mass();

        let (daughter, moved) = split_scavenged_bulk_with_available_tool(
            &registries,
            &mut state,
            ScoutSplitRequest {
                source: bulk,
                from: JUNCTION,
                to: DEN,
                at: SimulationTick::ZERO,
                bare_batch: crate::kobolds::SCOUT_BULK_SPLIT_BATCH,
                den_space: crate::kobolds::DEN_LOOSE_CACHE_LIMIT,
            },
        )
        .expect("a physical hand split must remain legal when tool wear cannot be recorded");
        let expected = if limited {
            crate::kobolds::SCOUT_BULK_SPLIT_BATCH
        } else {
            LEVER_SPLIT_BATCH
        };
        assert_eq!(moved, expected);
        assert_eq!(state.surface().get(daughter).unwrap().position(), DEN);
        assert_eq!(
            state.surface().get(daughter).unwrap().remaining_mass(),
            moved
        );
        assert_eq!(
            state.surface().get(bulk).unwrap().remaining_mass(),
            source_before.checked_sub(moved).unwrap(),
        );
        let wear = if limited {
            Mass::ZERO
        } else {
            crate::kobolds::scrap_tools::WEDGE_ABRASION_PER_SPLIT
        };
        assert_eq!(
            state.surface().get(head).unwrap().remaining_mass(),
            head_before.checked_sub(wear).unwrap(),
        );
        assert_eq!(
            calculate_matter_accounting(&state).unwrap().total(),
            conserved
        );
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
        let replay = serde_json::from_slice::<LoadedSaveEnvelope>(
            &serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap(),
        )
        .unwrap()
        .into_state(&registries)
        .unwrap();
        assert_eq!(state, replay);
    }
}

#[test]
fn neither_split_path_can_mutate_when_both_revision_budgets_are_exhausted() {
    let (registries, original, bulk, _) = staged_bulk_with_usable_wedge();
    let mut state = with_surface_revision(&registries, &original, u64::MAX - 1);
    let before = state.clone();
    assert!(!state.can_spend_surface_revisions(2));
    assert_eq!(
        split_scavenged_bulk_with_available_tool(
            &registries,
            &mut state,
            ScoutSplitRequest {
                source: bulk,
                from: JUNCTION,
                to: DEN,
                at: SimulationTick::ZERO,
                bare_batch: crate::kobolds::SCOUT_BULK_SPLIT_BATCH,
                den_space: crate::kobolds::DEN_LOOSE_CACHE_LIMIT,
            },
        ),
        None
    );
    assert_eq!(state, before);
}
