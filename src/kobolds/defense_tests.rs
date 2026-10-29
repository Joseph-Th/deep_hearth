//! A fitted, reported worksite tool makes cornering a scout materially risky.

use super::*;
use crate::content::{
    ENERGY_ELECTRICAL_BUFFER, FORM_HANDLE, FORM_INGOT, FORM_SCRAP, MATERIAL_IRON, MATERIAL_WOOD,
    build_registries,
};
use crate::core::quantity::{Energy, Mass};
use crate::core::state::validate_loaded_state;
use crate::energy::add_energy_store_with_initial_for_fixture;
use crate::kobolds::KoboldPryHeadKind;
use crate::logistics::{add_traversable_edge_for_test, place_energy_store_for_fixture};
use crate::material::{CommodityKey, MaterialComposition};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::{advance_control_tick, advance_tick};
use crate::spatial::VoxelCoord;
use crate::surface::{SurfaceResourceKind, SurfaceResourceSpec, validate_create_surface_resources};

const DEN: VoxelCoord = VoxelCoord::new(-1, 0, 0);
const WORKSITE: VoxelCoord = VoxelCoord::new(0, 0, 0);
const TIP: VoxelCoord = VoxelCoord::new(1, 0, 0);
const APPROACH: VoxelCoord = VoxelCoord::new(0, 0, 1);

fn to_tick(
    registries: &Registries,
    state: &mut AppState,
    target: u64,
) -> Vec<(SurfaceResourceId, u32)> {
    let mut blows = Vec::new();
    while state.tick().value() < target {
        if state.logistics().player().is_some() {
            let outcome = advance_control_tick(registries, state).unwrap();
            if let Some(contact) = outcome
                .simulation()
                .and_then(|tick| tick.kobold_scout())
                .and_then(|scout| scout.defensive_contact())
            {
                blows.push(contact);
            }
        } else {
            let _ = advance_tick(registries, state).unwrap();
        }
    }
    blows
}

fn drop_part(
    registries: &Registries,
    state: &mut AppState,
    commodity: CommodityKey,
    mass: Mass,
) -> SurfaceResourceId {
    let source = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        WORKSITE,
        commodity,
        mass,
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(commodity.material()),
    )
    .unwrap();
    let planned = validate_create_surface_resources(registries, state, [source]).unwrap();
    let id = planned.first_resource_id();
    planned.commit(state).unwrap();
    id
}

/// The machine is physically at the worksite. Its discovery still has to
/// travel through the real scout's den route before fitting is defended.
fn prepared_worksite(
    reported: bool,
) -> (Registries, AppState, SurfaceResourceId, SurfaceResourceId) {
    let registries = build_registries();
    let mut state = AppState::new();
    for (a, b) in [(DEN, WORKSITE), (WORKSITE, TIP), (WORKSITE, APPROACH)] {
        add_traversable_edge_for_test(&mut state, a, b);
    }
    state
        .kobold_state_mut()
        .apply_opening_colony([DEN, WORKSITE, TIP]);
    if reported {
        let machine = add_energy_store_with_initial_for_fixture(
            &registries,
            &mut state,
            ENERGY_ELECTRICAL_BUFFER,
            Energy::ZERO,
        )
        .unwrap();
        place_energy_store_for_fixture(&mut state, machine, WORKSITE);
    }
    to_tick(&registries, &mut state, 200);
    assert_eq!(
        state.kobold_state().colony.as_ref().unwrap().mobilized_at,
        reported.then_some(SimulationTick::new(195)),
        "mobilization requires a scout to deliver a physical machinery report",
    );
    let handle = drop_part(
        &registries,
        &mut state,
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
        Mass::from_milligrams(60_000),
    );
    let head = drop_part(
        &registries,
        &mut state,
        CommodityKey::new(MATERIAL_IRON, FORM_INGOT),
        Mass::from_milligrams(50_000),
    );
    (registries, state, handle, head)
}

#[test]
fn informed_scout_defends_real_fitted_iron_tool_with_finite_blunt_injury_and_wear() {
    let (registries, mut state, handle, head) = prepared_worksite(true);
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    to_tick(&registries, &mut state, 205);
    assert!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .pry_fitting
            .is_some()
    );
    assert!(
        super::super::scrap_tools::current_junction_scrap_lever(
            &state,
            state.kobold_state().colony.as_ref().unwrap(),
        )
        .is_none(),
        "unfinished iron fitting must not count as a ready weapon"
    );
    let mut interrupted = state.clone();
    validate_admit_player(&registries, &interrupted, APPROACH)
        .unwrap()
        .commit(&mut interrupted)
        .unwrap();
    assert!(to_tick(&registries, &mut interrupted, 215).is_empty());
    assert_eq!(
        interrupted.surface().get(head).unwrap().remaining_mass(),
        Mass::from_milligrams(50_000),
        "arriving before fitting finishes blocks the real work and its weapon",
    );
    assert_eq!(
        interrupted
            .survival()
            .player()
            .unwrap()
            .vitality()
            .parts_per_million(),
        1_000_000,
    );

    to_tick(&registries, &mut state, 210);
    let colony = state.kobold_state().colony.as_ref().unwrap();
    assert_eq!(colony.scout_position(), WORKSITE);
    let tool = super::super::scrap_tools::current_junction_scrap_lever(&state, colony).unwrap();
    assert_eq!(
        (tool.handle(), tool.head(), tool.kind()),
        (handle, head, KoboldPryHeadKind::IronShoe)
    );

    // The ready head remains a normal world drop, not a virtual equipped slot.
    // An actor arriving at the junction can pick it up before the scout uses it.
    let mut disarmed = state.clone();
    validate_admit_player(&registries, &disarmed, WORKSITE)
        .unwrap()
        .commit(&mut disarmed)
        .unwrap();
    for _ in 0..3 {
        let _ = advance_control_tick(&registries, &mut disarmed).unwrap();
    }
    assert!(
        disarmed
            .surface()
            .get(head)
            .unwrap()
            .remaining_mass()
            .is_zero(),
        "ordinary local automatic pickup must take custody of the actual fitted head"
    );
    crate::logistics::relocate_player_for_test(&mut disarmed, APPROACH);
    assert!(
        !crate::kobolds::available_nearby_kobold_scouts(&registries, &disarmed)
            .any(|resident| resident.role() == KoboldRole::Scout && resident.is_defending()),
        "the armed warning must disappear when the real head is picked up",
    );
    assert!(to_tick(&registries, &mut disarmed, 215).is_empty());
    assert_eq!(
        disarmed
            .survival()
            .player()
            .unwrap()
            .vitality()
            .parts_per_million(),
        1_000_000,
        "disarmed scout cannot inflict phantom damage",
    );

    validate_admit_player(&registries, &state, APPROACH)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert!(
        crate::kobolds::available_nearby_kobold_scouts(&registries, &state)
            .any(|resident| resident.role() == KoboldRole::Scout && resident.is_defending()),
        "a reported, fitted scout must visibly warn the player before contact",
    );
    let mut distant = state.clone();
    crate::logistics::relocate_player_for_test(&mut distant, VoxelCoord::new(0, 0, 3));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(state, replay);

    let first = to_tick(&registries, &mut state, 215);
    assert_eq!(first, vec![(head, 12_000)]);
    assert_eq!(first, to_tick(&registries, &mut replay, 215));
    assert_eq!(state, replay);
    assert_eq!(
        state
            .survival()
            .player()
            .unwrap()
            .vitality()
            .parts_per_million(),
        988_000,
    );
    assert_eq!(
        state.surface().get(head).unwrap().remaining_mass(),
        Mass::from_milligrams(48_000),
    );
    assert!(
        state
            .surface()
            .available_resources_at(Some(WORKSITE))
            .any(|resource| {
                resource.commodity() == CommodityKey::new(MATERIAL_IRON, FORM_SCRAP)
                    && resource.remaining_mass() == Mass::from_milligrams(2_000)
                    && resource.material_provenance()
                        == state.surface().get(head).unwrap().material_provenance()
            })
    );
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        WORKSITE,
        "contact spends a full patrol work slot instead of teleporting away",
    );
    assert!(to_tick(&registries, &mut distant, 215).is_empty());
    assert_eq!(
        distant
            .survival()
            .player()
            .unwrap()
            .vitality()
            .parts_per_million(),
        1_000_000,
    );
    for candidate in [&state, &replay, &distant, &interrupted, &disarmed] {
        assert_eq!(
            calculate_matter_accounting(candidate).unwrap().total(),
            conserved
        );
        assert_eq!(validate_loaded_state(&registries, candidate), Ok(()));
    }
    let second = to_tick(&registries, &mut state, 220);
    assert_eq!(second, vec![(head, 12_000)]);
    assert_eq!(second, to_tick(&registries, &mut replay, 220));
    assert_eq!(state, replay);
    let vitality = state
        .survival()
        .player()
        .unwrap()
        .vitality()
        .parts_per_million();
    assert!(
        (976_000..977_000).contains(&vitality),
        "two real 12,000 ppm blows must reduce vitality even with normal dietary recovery"
    );
    assert_eq!(
        state.surface().get(head).unwrap().remaining_mass(),
        Mass::from_milligrams(46_000)
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn unalerted_scout_with_the_same_fitted_iron_tool_does_not_attack() {
    let (registries, mut state, handle, head) = prepared_worksite(false);
    to_tick(&registries, &mut state, 210);
    let tool = super::super::scrap_tools::current_junction_scrap_lever(
        &state,
        state.kobold_state().colony.as_ref().unwrap(),
    )
    .expect("real fitting can occur without industrial military knowledge");
    assert_eq!((tool.handle(), tool.head()), (handle, head));
    validate_admit_player(&registries, &state, APPROACH)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert!(
        !crate::kobolds::available_nearby_kobold_scouts(&registries, &state)
            .any(|resident| resident.role() == KoboldRole::Scout && resident.is_defending()),
        "unalerted equipment must not advertise a military posture",
    );
    let before = calculate_matter_accounting(&state).unwrap().total();
    assert!(to_tick(&registries, &mut state, 215).is_empty());
    assert_eq!(
        state
            .survival()
            .player()
            .unwrap()
            .vitality()
            .parts_per_million(),
        1_000_000,
    );
    assert_eq!(
        state.surface().get(head).unwrap().remaining_mass(),
        Mass::from_milligrams(50_000),
        "a fitted tool alone must not authorize an informed defensive tactic",
    );
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), before);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn improvised_counterstrokes_stop_when_the_real_head_wears_below_fitting_size() {
    let (registries, mut state, _handle, head) = prepared_worksite(true);
    to_tick(&registries, &mut state, 210);
    validate_admit_player(&registries, &state, APPROACH)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let mut contacts = 0;
    for step in 1..=17 {
        let strikes = to_tick(&registries, &mut state, 210 + step * 5);
        contacts += strikes.len();
        if step <= 16 {
            assert_eq!(strikes, [(head, 12_000)]);
        } else {
            assert!(
                strikes.is_empty(),
                "an exhausted head cannot deliver free blows"
            );
        }
    }
    assert_eq!(contacts, 16);
    assert_eq!(
        state.surface().get(head).unwrap().remaining_mass(),
        Mass::from_milligrams(18_000),
        "sixteen 2 g strikes must consume 32 g of the original 50 g iron head",
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved,
        "the missing head mass must remain recoverable as real iron scrap",
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
