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
    put_drop_at(registries, state, DEN, commodity)
}

fn put_drop_at(
    registries: &Registries,
    state: &mut AppState,
    position: VoxelCoord,
    commodity: CommodityKey,
) -> SurfaceResourceId {
    let spec = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        position,
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

/// Execute through the actual 20 Hz player-control boundary and extract the
/// one slow tick's externally observable acoustic events.
fn advance_to_with_pulses(
    registries: &Registries,
    state: &mut AppState,
    until: u64,
) -> (
    Vec<KoboldAlarmPulse>,
    Option<crate::kobolds::KoboldScoutEvent>,
    usize,
) {
    let mut observed = (Vec::new(), None, 0);
    while state.tick().value() < until {
        let outcome = advance_control_tick(registries, state).unwrap();
        if let Some(simulation) = outcome.simulation() {
            observed = (
                simulation.kobold_alarm_pulses().to_vec(),
                simulation.kobold_scout(),
                simulation.discrete_event_category_count(),
            );
        }
    }
    observed
}

fn two_hop_alarm_fixture() -> (Registries, AppState, SurfaceResourceId) {
    let (registries, mut state) = mobilized_den();
    advance_to(&registries, &mut state, 205);
    assert_eq!(state.kobold_state().colony.as_ref().unwrap().scout_step, 2);
    put_drop(
        &registries,
        &mut state,
        CommodityKey::new(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING),
    );
    put_drop(
        &registries,
        &mut state,
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
    );
    put_drop_at(
        &registries,
        &mut state,
        JUNCTION,
        CommodityKey::new(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING),
    );
    let clapper = put_drop_at(
        &registries,
        &mut state,
        JUNCTION,
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
    );
    validate_admit_player(&registries, &state, OBSERVER)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    (registries, state, clapper)
}

#[test]
fn scavenger_rebuilds_player_sabotaged_repeater_using_real_spare_parcels() {
    let (registries, mut state, _) = two_hop_alarm_fixture();
    relocate_player_for_test(&mut state, JUNCTION);
    let _ = advance_control_tick(&registries, &mut state).unwrap();
    assert!(
        current_junction_relay(&state, state.kobold_state().colony.as_ref().unwrap()).is_none(),
        "ordinary player pickup must actually disable the warning network"
    );
    // Depart before the watchman's next slow observation; the scout must
    // physically fetch replacements, not conjure them while the player watches.
    relocate_player_for_test(&mut state, VoxelCoord::new(15, 0, 15));
    let spare_copper = put_drop_at(
        &registries,
        &mut state,
        TIP,
        CommodityKey::new(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING),
    );
    let spare_striker = put_drop_at(
        &registries,
        &mut state,
        TIP,
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
    );
    let before = calculate_matter_accounting(&state).unwrap().total();
    let mut guarded = state.clone();
    relocate_player_for_test(&mut guarded, JUNCTION);
    advance_to(&registries, &mut guarded, 230);
    assert_eq!(
        guarded.surface().get(spare_copper).unwrap().position(),
        TIP,
        "a guarded junction is not a destination the scout may enter to repair its machine"
    );
    assert_eq!(
        guarded.surface().get(spare_striker).unwrap().position(),
        TIP
    );
    assert!(
        current_junction_relay(&guarded, guarded.kobold_state().colony.as_ref().unwrap()).is_none()
    );
    assert_eq!(
        calculate_matter_accounting(&guarded).unwrap().total(),
        before
    );
    assert_eq!(validate_loaded_state(&registries, &guarded), Ok(()));
    advance_to(&registries, &mut state, 215);
    assert!(
        state.surface().get(spare_copper).unwrap().position() == JUNCTION
            || state.surface().get(spare_striker).unwrap().position() == JUNCTION,
        "the first repair component must arrive on an actual returning route step"
    );
    let snapshot = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&snapshot)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    advance_to(&registries, &mut state, 230);
    advance_to(&registries, &mut replay, 230);
    assert_eq!(state, replay);
    let repaired = current_junction_relay(&state, state.kobold_state().colony.as_ref().unwrap())
        .expect("the den scout must restore its missing repeater through scavenging");
    assert!(
        [spare_copper, spare_striker].contains(&repaired.core())
            || [spare_copper, spare_striker].contains(&repaired.striker()),
        "at least one newly recovered spare must be part of the repaired apparatus"
    );
    assert_eq!(
        state.surface().get(spare_copper).unwrap().position(),
        JUNCTION
    );
    assert_eq!(
        state.surface().get(spare_striker).unwrap().position(),
        JUNCTION
    );
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), before);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn percussion_reports_its_real_local_source_without_revealing_the_distant_scout() {
    let (registries, mut near, _) = two_hop_alarm_fixture();
    let baseline = calculate_matter_accounting(&near).unwrap().total();
    let snapshot = serde_json::to_vec(&SaveEnvelope::new(&registries, &near)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&snapshot)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let (pulses, scout, count) = advance_to_with_pulses(&registries, &mut near, 210);
    let (replayed_pulses, replayed_scout, replayed_count) =
        advance_to_with_pulses(&registries, &mut replay, 210);
    assert_eq!(near, replay);
    assert_eq!(pulses, replayed_pulses);
    assert_eq!(scout, replayed_scout);
    assert_eq!(count, replayed_count);
    assert_eq!(
        pulses
            .iter()
            .map(|pulse| (pulse.position(), pulse.kind()))
            .collect::<Vec<_>>(),
        vec![
            (Some(DEN), KoboldJunkRigKind::TimberRatchet),
            (None, KoboldJunkRigKind::TimberRatchet),
        ],
        "the more distant repeater can be heard but must not reveal its hidden location"
    );
    assert_eq!(
        scout, None,
        "the source can be heard without exposing the remote scout as a sighting"
    );
    assert_eq!(
        count, 1,
        "one sound is one discrete observable event category"
    );
    assert_eq!(
        calculate_matter_accounting(&near).unwrap().total(),
        baseline
    );
    assert_eq!(validate_loaded_state(&registries, &near), Ok(()));
}

#[test]
fn a_player_beside_the_junction_hears_both_real_synchronized_apparatuses() {
    let (registries, mut state, _) = two_hop_alarm_fixture();
    relocate_player_for_test(&mut state, VoxelCoord::new(-1, 0, 1));
    let (pulses, _, _) = advance_to_with_pulses(&registries, &mut state, 210);
    assert_eq!(
        pulses
            .iter()
            .map(|pulse| (pulse.position(), pulse.kind()))
            .collect::<Vec<_>>(),
        vec![
            (Some(DEN), KoboldJunkRigKind::TimberRatchet),
            (Some(JUNCTION), KoboldJunkRigKind::TimberRatchet)
        ]
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn walking_onto_a_repeater_recovers_its_parts_and_silences_the_remote_clatter() {
    let (registries, mut state, _) = two_hop_alarm_fixture();
    let before = calculate_matter_accounting(&state).unwrap().total();
    relocate_player_for_test(&mut state, JUNCTION);
    let (pulses, _, _) = advance_to_with_pulses(&registries, &mut state, 210);
    assert_eq!(
        pulses
            .iter()
            .map(|pulse| pulse.position())
            .collect::<Vec<_>>(),
        vec![Some(DEN)],
        "a player collected the loose junction rig before its synchronized beat"
    );
    assert!(
        current_junction_relay(&state, state.kobold_state().colony.as_ref().unwrap()).is_none(),
        "player pickup must dismantle the actual second apparatus"
    );
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), before);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn offbeat_missing_relay_and_remote_listener_emit_no_spurious_alarm_pulses() {
    let (registries, state, clapper) = two_hop_alarm_fixture();
    let mut disconnected = state.clone();
    assert!(crate::surface::relocate_scavenged_drop_step(
        &mut disconnected,
        clapper,
        JUNCTION,
        TIP
    ));
    let mut remote = state.clone();
    relocate_player_for_test(&mut remote, VoxelCoord::new(15, 0, 15));
    remote
        .kobold_state_mut()
        .colony
        .as_mut()
        .unwrap()
        .watch_alarm_pending = true;
    let mut offbeat = state;
    let den_striker = current_den_rig(&offbeat, offbeat.kobold_state().colony.as_ref().unwrap())
        .unwrap()
        .striker();
    assert!(crate::surface::relocate_scavenged_drop_step(
        &mut offbeat,
        den_striker,
        DEN,
        VoxelCoord::new(-2, 0, 0)
    ));
    put_drop(
        &registries,
        &mut offbeat,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
    );
    // 210 is a shared timber beat; the missing junction component prevents
    // the second clatter but not the physically witnessed den strike.
    let (disconnected_pulses, _, _) = advance_to_with_pulses(&registries, &mut disconnected, 210);
    assert_eq!(
        disconnected_pulses
            .iter()
            .map(|pulse| pulse.position())
            .collect::<Vec<_>>(),
        vec![Some(DEN)]
    );
    assert!(
        disconnected
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_alarm_pending,
        "a failed two-hop link retains the report for a later attempt"
    );
    let (remote_pulses, _, _) = advance_to_with_pulses(&registries, &mut remote, 210);
    assert!(
        remote_pulses.is_empty(),
        "a player out of local hearing range receives no remote alarm data"
    );
    // At 210, the stone source is between its slower beats even though the
    // timber junction resonator is ready. The local sound stays silent and
    // the watchman's warning remains pending for an actual future strike.
    let (offbeat_pulses, _, _) = advance_to_with_pulses(&registries, &mut offbeat, 210);
    assert!(offbeat_pulses.is_empty());
    assert!(
        offbeat
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_alarm_pending
    );
    assert_eq!(validate_loaded_state(&registries, &offbeat), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &disconnected), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &remote), Ok(()));
}

#[test]
fn mismatched_stone_and_timber_mechanisms_wait_for_a_shared_clatter_beat() {
    let (registries, mut state) = mobilized_den();
    advance_to(&registries, &mut state, 205);
    put_drop(
        &registries,
        &mut state,
        CommodityKey::new(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING),
    );
    put_drop(
        &registries,
        &mut state,
        CommodityKey::new(MATERIAL_STONE, crate::content::FORM_CHIP),
    );
    put_drop_at(
        &registries,
        &mut state,
        JUNCTION,
        CommodityKey::new(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING),
    );
    put_drop_at(
        &registries,
        &mut state,
        JUNCTION,
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
    );
    let colony = state.kobold_state().colony.as_ref().unwrap();
    let source = current_den_rig(&state, colony).unwrap();
    let relay = current_junction_relay(&state, colony).unwrap();
    assert_eq!(source.kind(), KoboldJunkRigKind::StoneClatter);
    assert_eq!(relay.kind(), KoboldJunkRigKind::TimberRatchet);
    assert!(!source.sounds_at(SimulationTick::new(210)));
    assert!(relay.sounds_at(SimulationTick::new(210)));
    assert!(source.sounds_at(SimulationTick::new(230)));
    assert!(!relay.sounds_at(SimulationTick::new(230)));
    assert!(source.sounds_at(SimulationTick::new(255)));
    assert!(relay.sounds_at(SimulationTick::new(255)));
    state
        .kobold_state_mut()
        .colony
        .as_mut()
        .unwrap()
        .watch_alarm_pending = true;
    assert!(!relay_watch_alarm(&mut state, SimulationTick::new(210)));
    assert!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_alarm_pending
    );
    assert!(!relay_watch_alarm(&mut state, SimulationTick::new(230)));
    assert!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_alarm_pending
    );
    assert!(relay_watch_alarm(&mut state, SimulationTick::new(255)));
    assert!(
        !state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_alarm_pending
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn disconnected_den_does_not_make_a_visible_junction_machine_disappear() {
    let (registries, mut state) = mobilized_den();
    let core = put_drop(
        &registries,
        &mut state,
        CommodityKey::new(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING),
    );
    let striker = put_drop(
        &registries,
        &mut state,
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
    );
    let relay_core = put_drop_at(
        &registries,
        &mut state,
        JUNCTION,
        CommodityKey::new(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING),
    );
    let relay_striker = put_drop_at(
        &registries,
        &mut state,
        JUNCTION,
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
    );
    let original = calculate_matter_accounting(&state).unwrap().total();
    validate_admit_player(&registries, &state, JUNCTION)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(
        available_nearby_kobold_junk_relay(&registries, &state)
            .map(|rig| (rig.core(), rig.striker())),
        Some((relay_core, relay_striker))
    );
    assert!(crate::surface::relocate_scavenged_drop_step(
        &mut state,
        striker,
        DEN,
        VoxelCoord::new(-2, 0, 0)
    ));
    assert_eq!(state.surface().get(core).unwrap().position(), DEN);
    assert!(
        current_junction_relay(&state, state.kobold_state().colony.as_ref().unwrap()).is_none(),
        "the two-hop warning cannot work without its den signal source"
    );
    assert_eq!(
        available_nearby_kobold_junk_relay(&registries, &state)
            .map(|rig| (rig.core(), rig.striker())),
        Some((relay_core, relay_striker)),
        "the same local visible machine remains when the upstream signal is lost"
    );
    assert!(
        !is_staged_relay_component(&state, state.surface().get(relay_core).unwrap()),
        "a disconnected relay is no longer reserved from the den scavenger"
    );
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(restored, state);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        original
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn scavenger_stages_two_real_salvaged_parts_at_junction_to_build_repeater() {
    let (registries, mut working) = mobilized_den();
    let mut without_den_rig = working.clone();
    let den_core = put_drop(
        &registries,
        &mut working,
        CommodityKey::new(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING),
    );
    let den_striker = put_drop(
        &registries,
        &mut working,
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
    );
    let copper = put_drop_at(
        &registries,
        &mut working,
        TIP,
        CommodityKey::new(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING),
    );
    let comparison_copper = put_drop_at(
        &registries,
        &mut without_den_rig,
        TIP,
        CommodityKey::new(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING),
    );
    let initial = calculate_matter_accounting(&working).unwrap().total();
    let comparison_initial = calculate_matter_accounting(&without_den_rig)
        .unwrap()
        .total();
    advance_to(&registries, &mut working, 215);
    advance_to(&registries, &mut without_den_rig, 215);
    assert_eq!(working.surface().get(copper).unwrap().position(), JUNCTION);
    assert_eq!(
        without_den_rig
            .surface()
            .get(comparison_copper)
            .unwrap()
            .position(),
        DEN,
        "a scout with no den source rig must still haul copper home normally"
    );
    assert!(
        current_junction_relay(&working, working.kobold_state().colony.as_ref().unwrap()).is_none(),
        "the single staged copper piece is not a functional repeater"
    );
    assert!(
        is_staged_relay_component(&working, working.surface().get(copper).unwrap()),
        "the first staged component remains at the remote post"
    );
    assert!(den_or_relay_needs_striker(&working, DEN));
    let handle = put_drop_at(
        &registries,
        &mut working,
        TIP,
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &working)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    advance_to(&registries, &mut working, 235);
    advance_to(&registries, &mut replay, 235);
    assert_eq!(working, replay);
    let relay = current_junction_relay(&working, working.kobold_state().colony.as_ref().unwrap())
        .expect("the returning scavenger assembled the two-part junction repeater");
    assert_eq!(relay.core(), copper);
    assert_eq!(relay.striker(), handle);
    assert_eq!(relay.kind(), KoboldJunkRigKind::TimberRatchet);
    assert_eq!(working.surface().get(copper).unwrap().position(), JUNCTION);
    assert_eq!(working.surface().get(handle).unwrap().position(), JUNCTION);
    assert_eq!(working.surface().get(den_core).unwrap().position(), DEN);
    assert_eq!(working.surface().get(den_striker).unwrap().position(), DEN);
    assert!(!den_or_relay_needs_striker(&working, DEN));
    assert_eq!(
        calculate_matter_accounting(&working).unwrap().total(),
        calculate_matter_accounting(&replay).unwrap().total()
    );
    assert_eq!(
        calculate_matter_accounting(&working).unwrap().total(),
        initial
            .checked_add(crate::core::quantity::AggregateMass::from_milligrams(
                50_000
            ))
            .unwrap()
    );
    assert_eq!(
        calculate_matter_accounting(&without_den_rig)
            .unwrap()
            .total(),
        comparison_initial
    );
    assert_eq!(validate_loaded_state(&registries, &working), Ok(()));
}

#[test]
fn real_junction_resonator_relays_a_den_warning_to_the_outer_scout() {
    let (registries, mut working) = mobilized_den();
    advance_to(&registries, &mut working, 205);
    assert_eq!(
        working.kobold_state().colony.as_ref().unwrap().scout_step,
        2
    );
    let salvage = put_drop_at(
        &registries,
        &mut working,
        TIP,
        CommodityKey::new(MATERIAL_STONE, crate::content::FORM_LUMP),
    );
    let den_core = put_drop(
        &registries,
        &mut working,
        CommodityKey::new(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING),
    );
    put_drop(
        &registries,
        &mut working,
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
    );
    let relay_core = put_drop_at(
        &registries,
        &mut working,
        JUNCTION,
        CommodityKey::new(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING),
    );
    let relay_striker = put_drop_at(
        &registries,
        &mut working,
        JUNCTION,
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
    );
    let mut broken = working.clone();
    assert!(crate::surface::relocate_scavenged_drop_step(
        &mut broken,
        relay_striker,
        JUNCTION,
        DEN,
    ));
    for state in [&mut working, &mut broken] {
        validate_admit_player(&registries, state, OBSERVER)
            .unwrap()
            .commit(state)
            .unwrap();
    }
    let relay = current_junction_relay(&working, working.kobold_state().colony.as_ref().unwrap())
        .expect("the second resonator is made of two physical loose parcels");
    assert_eq!(relay.core(), relay_core);
    assert_eq!(relay.striker(), relay_striker);
    assert_eq!(relay.position(), JUNCTION);
    assert!(relay.sounds_at(SimulationTick::new(210)));
    assert!(!relay.sounds_at(SimulationTick::new(205)));
    assert!(
        current_junction_relay(&broken, broken.kobold_state().colony.as_ref().unwrap()).is_none()
    );

    let baseline = calculate_matter_accounting(&working).unwrap().total();
    let broken_baseline = calculate_matter_accounting(&broken).unwrap().total();
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &working)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    advance_to(&registries, &mut working, 210);
    advance_to(&registries, &mut replay, 210);
    advance_to(&registries, &mut broken, 210);
    assert_eq!(working, replay);
    assert!(
        working
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .wary_until
            .is_some(),
        "the alarm must actually put a distant scout into caution"
    );
    assert_eq!(
        working.surface().get(salvage).unwrap().position(),
        TIP,
        "the alerted scout retreats without collecting dangerous scrap"
    );
    assert_eq!(
        broken.surface().get(salvage).unwrap().position(),
        JUNCTION,
        "without the missing junction part, the same scout physically carries the scrap"
    );
    assert_eq!(working.surface().get(den_core).unwrap().position(), DEN);
    assert_eq!(
        working.surface().get(relay_core).unwrap().position(),
        JUNCTION
    );
    assert_eq!(
        calculate_matter_accounting(&working).unwrap().total(),
        baseline
    );
    assert_eq!(
        calculate_matter_accounting(&broken).unwrap().total(),
        broken_baseline
    );
    assert_eq!(validate_loaded_state(&registries, &working), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &broken), Ok(()));
    assert_eq!(
        available_nearby_kobold_junk_relay(&registries, &working),
        None
    );
    relocate_player_for_test(&mut working, JUNCTION);
    assert_eq!(
        available_nearby_kobold_junk_relay(&registries, &working)
            .map(|rig| (rig.core(), rig.striker())),
        Some((relay_core, relay_striker))
    );
    relocate_player_for_test(&mut working, VoxelCoord::new(15, 0, 15));
    advance_to(&registries, &mut working, 215);
    assert!(
        current_junction_relay(&working, working.kobold_state().colony.as_ref().unwrap()).is_some(),
        "the scout must not dismantle its own alarm network on the return trip"
    );
    assert_eq!(
        working.surface().get(relay_core).unwrap().position(),
        JUNCTION
    );
    assert_eq!(
        working.surface().get(relay_striker).unwrap().position(),
        JUNCTION
    );
    assert_eq!(
        calculate_matter_accounting(&working).unwrap().total(),
        baseline
    );
    assert_eq!(validate_loaded_state(&registries, &working), Ok(()));
}

#[test]
fn scavenger_seeks_a_missing_wooden_striker_beyond_nearer_scrap() {
    let (registries, mut purposeful) = mobilized_den();
    put_drop_at(
        &registries,
        &mut purposeful,
        JUNCTION,
        CommodityKey::new(MATERIAL_STONE, crate::content::FORM_LUMP),
    );
    let handle = put_drop_at(
        &registries,
        &mut purposeful,
        TIP,
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
    );
    let mut unmotivated = purposeful.clone();
    put_drop(
        &registries,
        &mut purposeful,
        CommodityKey::new(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING),
    );
    let total = calculate_matter_accounting(&purposeful).unwrap().total();
    assert!(den_needs_striker(&purposeful, DEN));
    assert!(!den_needs_striker(&unmotivated, DEN));

    advance_to(&registries, &mut purposeful, 205);
    advance_to(&registries, &mut unmotivated, 205);
    assert_eq!(
        purposeful
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_step,
        2,
        "the purposeful scout must actually walk toward the missing component"
    );
    assert_eq!(
        unmotivated
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_step,
        0,
        "without a copper core the nearby ordinary salvage should win"
    );
    let checkpoint = serde_json::to_vec(&SaveEnvelope::new(&registries, &purposeful)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&checkpoint)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    advance_to(&registries, &mut purposeful, 215);
    advance_to(&registries, &mut replay, 215);
    assert_eq!(purposeful, replay);
    let rig = current_den_rig(
        &purposeful,
        purposeful.kobold_state().colony.as_ref().unwrap(),
    )
    .expect("the scout physically delivered the missing part");
    assert_eq!(rig.striker(), handle);
    assert_eq!(rig.kind(), KoboldJunkRigKind::TimberRatchet);
    assert!(!den_needs_striker(&purposeful, DEN));
    assert_eq!(purposeful.surface().get(handle).unwrap().position(), DEN);
    assert_eq!(
        calculate_matter_accounting(&purposeful).unwrap().total(),
        total
    );
    assert_eq!(validate_loaded_state(&registries, &purposeful), Ok(()));
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
fn taking_a_real_component_disables_the_den_alarm_without_consuming_material() {
    let (registries, mut state) = mobilized_den();
    put_drop(
        &registries,
        &mut state,
        CommodityKey::new(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING),
    );
    let striker = put_drop(
        &registries,
        &mut state,
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
    );
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    assert!(current_den_rig(&state, state.kobold_state().colony.as_ref().unwrap()).is_some());
    assert!(crate::surface::relocate_scavenged_drop_step(
        &mut state, striker, DEN, JUNCTION
    ));
    assert!(den_needs_striker(&state, DEN));
    assert!(current_den_rig(&state, state.kobold_state().colony.as_ref().unwrap()).is_none());
    assert_eq!(state.surface().get(striker).unwrap().position(), JUNCTION);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(loaded, state);
    assert_eq!(validate_loaded_state(&registries, &loaded), Ok(()));
}

#[test]
fn exceptional_control_snapshot_isolates_real_surface_custody_and_indexes() {
    let (registries, mut state) = mobilized_den();
    let core = put_drop(
        &registries,
        &mut state,
        CommodityKey::new(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING),
    );
    let mut staged = state.snapshot_for_control_transaction();
    assert_eq!(
        staged, state,
        "transaction snapshot must preserve all owner indexes"
    );
    assert!(crate::surface::relocate_scavenged_drop_step(
        &mut staged,
        core,
        DEN,
        JUNCTION
    ));
    assert_eq!(state.surface().get(core).unwrap().position(), DEN);
    assert_eq!(staged.surface().get(core).unwrap().position(), JUNCTION);
    assert_eq!(validate_loaded_state(&registries, &staged), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
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
