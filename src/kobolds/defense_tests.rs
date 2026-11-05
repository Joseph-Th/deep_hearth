//! A fitted, reported worksite tool makes cornering a scout materially risky.

use super::*;
use crate::content::{
    ENERGY_ELECTRICAL_BUFFER, FORM_HANDLE, FORM_INGOT, FORM_REINFORCEMENT, FORM_SCRAP,
    MATERIAL_COPPER, MATERIAL_IRON, MATERIAL_WOOD, build_registries,
};
use crate::core::quantity::{Energy, Mass, Temperature};
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

#[test]
fn scout_contact_tick_reports_survival_after_the_committed_injury() {
    let (registries, mut state, _handle, head) = prepared_worksite(true);
    to_tick(&registries, &mut state, 210);
    validate_admit_player(&registries, &state, APPROACH)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();

    let mut confirmed_contact = false;
    while state.tick().value() < 215 {
        let outcome = advance_control_tick(&registries, &mut state).unwrap();
        assert_eq!(
            outcome,
            advance_control_tick(&registries, &mut replay).unwrap()
        );
        let Some(slow) = outcome.simulation() else {
            continue;
        };
        let reported = slow.survival().unwrap();
        assert_eq!(
            reported,
            crate::survival::assess_survival(&registries, &state).unwrap()
        );
        if slow
            .kobold_scout()
            .and_then(|scout| scout.defensive_contact())
            == Some((head, 12_000))
        {
            confirmed_contact = true;
            assert_eq!(
                reported.vitality(),
                state.survival().player().unwrap().vitality()
            );
            assert_eq!(reported.vitality().parts_per_million(), 988_000);
        }
    }
    assert!(
        confirmed_contact,
        "fixture must produce a real worksite counterstroke"
    );
    assert_eq!(state, replay);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
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
    prepared_worksite_with_approach_edge(reported, true)
}

fn prepared_worksite_with_approach_edge(
    reported: bool,
    approach_edge: bool,
) -> (Registries, AppState, SurfaceResourceId, SurfaceResourceId) {
    let registries = build_registries();
    let mut state = AppState::new();
    for (a, b) in [(DEN, WORKSITE), (WORKSITE, TIP)] {
        add_traversable_edge_for_test(&mut state, a, b);
    }
    if approach_edge {
        add_traversable_edge_for_test(&mut state, WORKSITE, APPROACH);
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

/// Real generated floors and occupied air, not authored scout graph edges,
/// provide collision and movement authority around the existing worksite.
fn generate_worksite_terrain(registries: &Registries, state: &mut AppState) {
    use crate::content::BLOCK_STATE_STONE;
    use crate::world::{
        BlockStateId, ChunkCoord, WorldGenerationSeed, set_generated_block_for_test,
        validate_generate_chunk,
    };

    for chunk in [
        ChunkCoord::containing(DEN),
        ChunkCoord::containing(WORKSITE),
    ] {
        validate_generate_chunk(
            registries,
            state,
            WorldGenerationSeed::new(7),
            chunk,
            BLOCK_STATE_STONE,
        )
        .unwrap()
        .commit(state)
        .unwrap();
    }
    for voxel in [DEN, WORKSITE, TIP, APPROACH] {
        set_generated_block_for_test(
            registries.world(),
            state,
            VoxelCoord::new(voxel.x(), -1, voxel.z()),
            BLOCK_STATE_STONE,
        );
        for y in 0..=2 {
            set_generated_block_for_test(
                registries.world(),
                state,
                VoxelCoord::new(voxel.x(), y, voxel.z()),
                BlockStateId::AIR,
            );
        }
    }
}

/// Complete the normal resident's witness/report/fitting sequence in generated
/// terrain rather than moving a previously fitted authored opening into a world.
fn prepared_generated_worksite() -> (Registries, AppState, SurfaceResourceId, SurfaceResourceId) {
    let registries = build_registries();
    let mut state = AppState::new();
    for (first, second) in [(DEN, WORKSITE), (WORKSITE, TIP)] {
        add_traversable_edge_for_test(&mut state, first, second);
    }
    state
        .kobold_state_mut()
        .apply_opening_colony([DEN, WORKSITE, TIP]);
    let machine = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        ENERGY_ELECTRICAL_BUFFER,
        Energy::ZERO,
    )
    .unwrap();
    place_energy_store_for_fixture(&mut state, machine, WORKSITE);
    generate_worksite_terrain(&registries, &mut state);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    to_tick(&registries, &mut state, 200);
    assert_eq!(
        state.kobold_state().colony.as_ref().unwrap().mobilized_at,
        Some(SimulationTick::new(195)),
        "generated-world scout must actually report the worksite before fitting"
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
fn ordinary_generated_walk_and_pickup_disarm_fitted_scout_before_counterstroke() {
    use crate::logistics::{HorizontalControlInput, PlayerControlInput};
    use crate::simulation::advance_control_tick_with_input;

    let (registries, mut state, handle, head) = prepared_generated_worksite();
    to_tick(&registries, &mut state, 210);
    assert!(
        ready_worksite_tool(
            &registries,
            &state,
            state.kobold_state().colony.as_ref().unwrap(),
            state.tick()
        )
        .is_some(),
        "the scout must complete real fitting on generated ground"
    );
    validate_admit_player(&registries, &state, APPROACH)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    assert!(
        ready_worksite_tool(
            &registries,
            &state,
            state.kobold_state().colony.as_ref().unwrap(),
            state.tick()
        )
        .is_some()
    );
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let mut idle = state.clone();
    let mut packed = state.clone();
    let packed_carried = packed.logistics().player().unwrap().carried_stockpile();
    let full_capacity = packed
        .inventory()
        .get_stockpile(packed_carried)
        .unwrap()
        .capacity();
    crate::inventory::deposit_lot_for_test(
        &registries,
        &mut packed,
        packed_carried,
        CommodityKey::new(crate::content::MATERIAL_STONE, crate::content::FORM_LUMP),
        full_capacity,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    assert_eq!(
        packed
            .inventory()
            .get_stockpile(packed_carried)
            .unwrap()
            .available_capacity(),
        Mass::ZERO
    );
    let packed_matter = calculate_matter_accounting(&packed).unwrap().total();
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
        .unwrap()
        .into_state(&registries)
        .unwrap();

    // The real input-frame motion, not a test relocation or custody grant,
    // enters the scout's occupied junction in generated traversable air.
    let walk = PlayerControlInput::new(HorizontalControlInput::new(0, -1_000_000).unwrap());
    let mut entered = false;
    for _ in 0..45 {
        let outcome = advance_control_tick_with_input(&registries, &mut state, walk).unwrap();
        assert_eq!(
            outcome,
            advance_control_tick_with_input(&registries, &mut replay, walk).unwrap()
        );
        if state.logistics().player().unwrap().position() == WORKSITE {
            entered = true;
            break;
        }
    }
    assert!(
        entered,
        "normal 20 Hz walking must enter the real neighboring air cell"
    );

    // Two separate, source-local parcels require two actual pickup steps.
    // Both become carried matter; the old fitted identities do not disappear
    // from the save or become ghost equipment in the resident's hands.
    let mut picked_head = false;
    for _ in 0..32 {
        let outcome = advance_control_tick(&registries, &mut state).unwrap();
        assert_eq!(
            outcome,
            advance_control_tick(&registries, &mut replay).unwrap()
        );
        picked_head |= outcome
            .surface_drop_pickup()
            .is_some_and(|picked| picked.resource() == head);
        if picked_head {
            break;
        }
    }
    assert!(
        picked_head,
        "the player must physically recover the fitted metal head"
    );
    assert_eq!(
        state.surface().get(head).unwrap().remaining_mass(),
        Mass::ZERO
    );
    assert_eq!(
        state.surface().get(handle).unwrap().remaining_mass(),
        Mass::ZERO
    );
    let carried = state.logistics().player().unwrap().carried_stockpile();
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(CommodityKey::new(MATERIAL_IRON, FORM_INGOT)),
        Mass::from_milligrams(50_000)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE)),
        Mass::from_milligrams(60_000)
    );
    assert!(
        ready_worksite_tool(
            &registries,
            &state,
            state.kobold_state().colony.as_ref().unwrap(),
            state.tick()
        )
        .is_none()
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );

    // Return to the original face-adjacent threatened cell after disarming.
    // Remaining inside the scout's own cell would avoid the contact geometry
    // even with an intact head, so it cannot establish that sabotage mattered.
    let retreat = PlayerControlInput::new(HorizontalControlInput::new(0, 1_000_000).unwrap());
    let mut withdrew = false;
    for _ in 0..45 {
        let outcome = advance_control_tick_with_input(&registries, &mut state, retreat).unwrap();
        assert_eq!(
            outcome,
            advance_control_tick_with_input(&registries, &mut replay, retreat).unwrap()
        );
        if state.logistics().player().unwrap().position() == APPROACH {
            withdrew = true;
            break;
        }
    }
    assert!(
        withdrew,
        "disarming player must walk back to the exposed neighboring cell"
    );
    for _ in 0..20 {
        let outcome = advance_control_tick(&registries, &mut state).unwrap();
        assert_eq!(
            outcome,
            advance_control_tick(&registries, &mut replay).unwrap()
        );
    }
    assert_eq!(state.logistics().player().unwrap().position(), APPROACH);
    assert!(
        state
            .logistics()
            .player()
            .unwrap()
            .body()
            .horizontal_velocity()
            .z_nanometers_per_second()
            == 0
    );
    assert!(state.tick().value() < 215);

    // Control for movement alone: with a genuinely full pack the player can
    // walk through the same occupied worksite, but cannot collect the head.
    // Returning to the identical adjacent threat cell must still incur injury.
    let mut packed_entered = false;
    for _ in 0..100 {
        let _ = advance_control_tick_with_input(&registries, &mut packed, walk).unwrap();
        if packed.logistics().player().unwrap().position() == WORKSITE {
            packed_entered = true;
            break;
        }
    }
    assert!(
        packed_entered,
        "a heavy carried load slows but does not prohibit walking"
    );
    for _ in 0..20 {
        let _ = advance_control_tick(&registries, &mut packed).unwrap();
    }
    assert_eq!(
        packed.surface().get(head).unwrap().remaining_mass(),
        Mass::from_milligrams(50_000)
    );
    let mut packed_withdrew = false;
    for _ in 0..100 {
        let _ = advance_control_tick_with_input(&registries, &mut packed, retreat).unwrap();
        if packed.logistics().player().unwrap().position() == APPROACH {
            packed_withdrew = true;
            break;
        }
    }
    assert!(packed_withdrew);
    for _ in 0..25 {
        let _ = advance_control_tick(&registries, &mut packed).unwrap();
    }
    assert_eq!(packed.logistics().player().unwrap().position(), APPROACH);
    assert!(packed.tick().value() < 215);
    assert_eq!(to_tick(&registries, &mut packed, 215), [(head, 12_000)]);
    assert_eq!(
        packed.surface().get(head).unwrap().remaining_mass(),
        Mass::from_milligrams(48_000),
        "a walk without physical pickup cannot disarm the worksite"
    );
    assert_eq!(
        calculate_matter_accounting(&packed).unwrap().total(),
        packed_matter
    );
    assert_eq!(validate_loaded_state(&registries, &packed), Ok(()));

    // The same adjacent encounter without approaching also receives injury.
    // The disarmed route alone cannot spend a phantom head on contact.
    assert_eq!(to_tick(&registries, &mut idle, 215), [(head, 12_000)]);
    let mut contacts = Vec::new();
    while state.tick().value() < 215 {
        let result = advance_control_tick(&registries, &mut state).unwrap();
        assert_eq!(
            result,
            advance_control_tick(&registries, &mut replay).unwrap()
        );
        if let Some(contact) = result
            .simulation()
            .and_then(|slow| slow.kobold_scout())
            .and_then(|scout| scout.defensive_contact())
        {
            contacts.push(contact);
        }
    }
    assert!(contacts.is_empty());
    assert_eq!(
        state
            .survival()
            .player()
            .unwrap()
            .vitality()
            .parts_per_million(),
        1_000_000
    );
    assert_eq!(
        idle.survival()
            .player()
            .unwrap()
            .vitality()
            .parts_per_million(),
        988_000
    );
    assert_eq!(state, replay);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(
        calculate_matter_accounting(&idle).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &idle), Ok(()));
}

#[test]
fn generated_unrouted_neighbor_can_receive_a_real_worksite_counterstroke() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::world::{BlockStateId, set_generated_block_for_test};

    let (registries, mut opening, _, head) = prepared_worksite_with_approach_edge(true, false);
    to_tick(&registries, &mut opening, 210);
    assert!(!opening.logistics().has_traversal_edge(WORKSITE, APPROACH));
    assert_eq!(
        opening
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        WORKSITE
    );
    assert_eq!(validate_loaded_state(&registries, &opening), Ok(()));

    // The old authored opening needs a real access graph to establish safe
    // contact where no physical block-world geometry has yet been generated.
    let mut authored = opening.clone();
    validate_admit_player(&registries, &authored, APPROACH)
        .unwrap()
        .commit(&mut authored)
        .unwrap();
    assert!(to_tick(&registries, &mut authored, 215).is_empty());
    assert_eq!(
        authored
            .survival()
            .player()
            .unwrap()
            .vitality()
            .parts_per_million(),
        1_000_000
    );

    // The player is physically beside the fixed scout worksite in generated
    // terrain, but that side cell was never a kobold patrol destination.
    let mut generated = opening;
    generate_worksite_terrain(&registries, &mut generated);
    assert!(crate::world::has_clear_two_step_sightline(
        &registries,
        &generated,
        WORKSITE,
        APPROACH,
    ));
    assert!(!generated.logistics().has_traversal_edge(WORKSITE, APPROACH));
    validate_admit_player(&registries, &generated, APPROACH)
        .unwrap()
        .commit(&mut generated)
        .unwrap();
    assert_eq!(validate_loaded_state(&registries, &generated), Ok(()));

    let original_matter = calculate_matter_accounting(&generated).unwrap().total();
    // A scout stranded at its actual worksite retains the fitted head, but
    // cannot swing it at a neighboring player through an excavated floor.
    // Both the player-facing defending cue and authoritative contact must
    // agree. The old tool/ingot identities remain available after repair.
    let mut undermined = generated.clone();
    let floor = VoxelCoord::new(WORKSITE.x(), WORKSITE.y() - 1, WORKSITE.z());
    set_generated_block_for_test(
        registries.world(),
        &mut undermined,
        floor,
        BlockStateId::AIR,
    );
    assert!(
        !undermined
            .world()
            .has_standing_clearance(registries.world(), WORKSITE)
    );
    let unsupported_matter = calculate_matter_accounting(&undermined).unwrap().total();
    assert!(
        !crate::kobolds::available_nearby_kobold_scouts(&registries, &undermined)
            .any(|sighting| sighting.role() == KoboldRole::Scout && sighting.is_defending()),
        "unsupported weapon work must not advertise an active counterstroke"
    );
    let undermined_save = serde_json::to_vec(&SaveEnvelope::new(&registries, &undermined)).unwrap();
    let mut undermined_replay = serde_json::from_slice::<LoadedSaveEnvelope>(&undermined_save)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    // Restoring floor support before the threatened encounter restores the
    // actual blow. Once the unsupported encounter has already happened,
    // ordinary close-contact caution still applies independently of footing.
    let mut repaired_before_contact = undermined_replay.clone();
    set_generated_block_for_test(
        registries.world(),
        &mut repaired_before_contact,
        floor,
        BLOCK_STATE_STONE,
    );
    assert_eq!(
        to_tick(&registries, &mut repaired_before_contact, 215),
        [(head, 12_000)]
    );
    assert_eq!(
        calculate_matter_accounting(&repaired_before_contact)
            .unwrap()
            .total(),
        original_matter
    );
    assert_eq!(
        validate_loaded_state(&registries, &repaired_before_contact),
        Ok(())
    );
    assert_eq!(
        to_tick(&registries, &mut undermined, 215),
        Vec::<(SurfaceResourceId, u32)>::new(),
        "an unsupported scout cannot deliver a damaging counterstroke"
    );
    assert!(to_tick(&registries, &mut undermined_replay, 215).is_empty());
    assert_eq!(undermined, undermined_replay);
    assert_eq!(
        undermined
            .survival()
            .player()
            .unwrap()
            .vitality()
            .parts_per_million(),
        1_000_000,
        "unsupported scout cannot harm the adjacent player"
    );
    assert_eq!(
        undermined.surface().get(head).unwrap().remaining_mass(),
        Mass::from_milligrams(50_000),
        "blocked work must not abrade the real tool"
    );
    assert_eq!(
        calculate_matter_accounting(&undermined).unwrap().total(),
        unsupported_matter
    );
    assert_eq!(validate_loaded_state(&registries, &undermined), Ok(()));

    let mut away = generated.clone();
    crate::logistics::relocate_player_for_test(&mut away, VoxelCoord::new(0, 0, 2));
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &generated)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(generated, replay);

    let contact = to_tick(&registries, &mut generated, 215);
    assert_eq!(
        contact,
        [(head, 12_000)],
        "physical adjacent contact in generated air must not need an unrelated route edge"
    );
    assert_eq!(contact, to_tick(&registries, &mut replay, 215));
    assert_eq!(generated, replay);
    assert!(to_tick(&registries, &mut away, 215).is_empty());
    assert_eq!(
        generated
            .survival()
            .player()
            .unwrap()
            .vitality()
            .parts_per_million(),
        988_000
    );
    assert_eq!(
        generated.surface().get(head).unwrap().remaining_mass(),
        Mass::from_milligrams(48_000)
    );
    for state in [&generated, &away] {
        assert_eq!(
            calculate_matter_accounting(state).unwrap().total(),
            original_matter
        );
        assert_eq!(validate_loaded_state(&registries, state), Ok(()));
    }
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
fn audible_den_alarm_is_reported_even_when_the_fitted_scout_defends_its_worksite() {
    let (registries, mut state, _handle, head) = prepared_worksite(true);
    to_tick(&registries, &mut state, 210);
    // The actual paired copper salvage and iron striker produce a den chime;
    // its eight-step beat at tick 220 must not disappear behind a simultaneous
    // worksite counterstroke.
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
            Mass::from_milligrams(20_000),
        ),
        (
            CommodityKey::new(MATERIAL_IRON, FORM_SCRAP),
            Mass::from_milligrams(25_000),
        ),
    ] {
        let spec = SurfaceResourceSpec::new(
            SurfaceResourceKind::Drop,
            DEN,
            commodity,
            mass,
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(commodity.material()),
        )
        .unwrap();
        validate_create_surface_resources(&registries, &state, [spec])
            .unwrap()
            .commit(&mut state)
            .unwrap();
    }
    validate_admit_player(&registries, &state, APPROACH)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let serialized = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&serialized)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(state, replay);
    let original_matter = calculate_matter_accounting(&state).unwrap().total();
    let mut alarm_contact = None;
    while state.tick().value() < 220 {
        let frame = advance_control_tick(&registries, &mut state).unwrap();
        assert_eq!(
            frame,
            advance_control_tick(&registries, &mut replay).unwrap(),
            "observable alarm and real abrasion must replay identically after load",
        );
        if let Some(tick) = frame.simulation()
            && tick.tick() == SimulationTick::new(220)
        {
            let scout = tick.kobold_scout().expect("fitted scout must defend");
            alarm_contact = Some((scout, tick.kobold_alarm_pulses().to_vec()));
        }
    }
    let (contact, pulses) = alarm_contact.expect("tick 220 contact must be visible");
    assert_eq!(contact.defensive_contact(), Some((head, 12_000)));
    assert!(
        !pulses.is_empty(),
        "the real den alarm must produce an audible pulse"
    );
    assert!(
        contact.heard_junk_alarm(),
        "simultaneous defense must not erase a heard alarm"
    );
    assert_eq!(state, replay);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        original_matter
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
