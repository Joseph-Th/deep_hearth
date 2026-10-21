//! Reclaiming a full finite kobold den opens capacity for future scavenging.
use super::*;

use deep_hearth::logistics::project_player_walk_path;
use deep_hearth::surface::{available_adjacent_surface_resources, available_player_world_drops};
use std::collections::{BTreeSet, VecDeque};

fn distance_from_clearing(position: VoxelCoord, clearing: VoxelCoord) -> u64 {
    position
        .x()
        .abs_diff(clearing.x())
        .max(position.z().abs_diff(clearing.z()))
}

/// Walk beyond the visible foraging clearing using only walkable adjacent-body projections.
/// The actor cannot use the hidden den/route as a navigation oracle. This also exercises
/// off-graph world exploration rather than treating the authored opening edges as walls.
fn find_unattended_observation_camp(
    registries: &Registries,
    state: &mut AppState,
    source: VoxelCoord,
    water: VoxelCoord,
    clay: VoxelCoord,
) -> Vec<VoxelCoord> {
    let mut path = vec![water, clay];
    if distance_from_clearing(clay, source) >= 3 {
        return path;
    }
    // A ridge may block every directly outward step despite a short, walkable detour.
    // Discover reachable same-height cells with the production locomotion projection;
    // do not inspect generated terrain blocks or derive a route from hidden scout state.
    let mut pending = VecDeque::from([Vec::<VoxelCoord>::new()]);
    let mut observed = BTreeSet::from([clay]);
    let mut chosen = None;
    while let Some(steps) = pending.pop_front() {
        if steps.len() >= 5 {
            continue;
        }
        let position = steps.last().copied().unwrap_or(clay);
        for (dx, dz) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
            let (Some(x), Some(z)) = (position.x().checked_add(dx), position.z().checked_add(dz))
            else {
                continue;
            };
            let next = VoxelCoord::new(x, clay.y(), z);
            if observed.contains(&next) {
                continue;
            }
            let mut next_steps = steps.clone();
            next_steps.push(next);
            if project_player_walk_path(registries, state, &next_steps).is_err() {
                continue;
            }
            observed.insert(next);
            if distance_from_clearing(next, source) >= 3 {
                chosen = Some(next_steps);
                break;
            }
            pending.push_back(next_steps);
        }
        if chosen.is_some() {
            break;
        }
    }
    let chosen =
        chosen.expect("the generated opening needs a reachable off-trail observation camp");
    for next in chosen {
        walk_continuous_body_adjacent_and_settle(
            registries,
            state,
            next,
            "leave the scavenger patrol's immediate vicinity",
        );
        path.push(next);
    }
    path
}

pub(super) fn exercise_kobold_den_cache(print: bool) {
    let registries = build_registries();
    let mut state = AppState::new();
    let seed = WILDERNESS_WORLD_SEED;
    let spawn = generated_wilderness_surface_spawn(&registries, &mut state, seed);
    validate_generate_initial_wilderness(&registries, &state, seed, spawn)
        .unwrap_or_else(|error| panic!("den raid generation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("den raid opening commit failed: {error}"));
    let carried = validate_admit_player(&registries, &state, spawn)
        .unwrap_or_else(|error| panic!("den raid admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("den raid player commit failed: {error}"))
        .carried_stockpile();
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let nuts = CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD);
    let berry = adjacent_surface_resource(&state, CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD))
        .position();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        berry,
        "den raid scout route reconnaissance",
    );
    let fork = adjacent_surface_resource(&state, nuts).position();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        fork,
        "den raid inspect natural food",
    );
    let initial = state
        .available_surface_resources()
        .find(|source| source.commodity() == nuts)
        .expect("den raid relies on observed wild nuts")
        .remaining_mass();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        berry,
        "den raid withdraw from fork",
    );
    // Merely reaching the original spawn need not leave a foraging patch unattended:
    // the opening path can bend, leaving the player diagonally next to the scout.
    // Withdraw toward the observed water branch and its clay continuation instead.
    let water = state
        .available_adjacent_traversal_destinations()
        .find(|position| *position != fork && *position != spawn)
        .expect("forage crossing must disclose the alternative water branch");
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        water,
        "den raid withdraw beyond the immediate scout clearing",
    );
    let clay = state
        .available_adjacent_traversal_destinations()
        .find(|position| *position != berry)
        .expect("water branch must disclose its further clay outcrop");
    assert!(
        fork.x().abs_diff(clay.x()) > 1 || fork.z().abs_diff(clay.z()) > 1,
        "the observation camp must not inadvertently guard the forage source"
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        clay,
        "den raid leave the scout's clearing unguarded",
    );
    let retreat = find_unattended_observation_camp(&registries, &mut state, fork, water, clay);
    let unguarded_from = state.tick().value();
    // More than five successful six-minute forage intervals pass. The scout
    // remains finite even though the observed natural source still has food.
    advance_to(
        &registries,
        &mut state,
        SimulationTick::new(815),
        TickEventAllowance::default(),
        "den cache fills during unattended patrol",
    );
    for &destination in retreat.iter().rev().skip(1) {
        walk_continuous_body_adjacent_and_settle(
            &registries,
            &mut state,
            destination,
            "den raid return from the off-trail observation camp",
        );
    }
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        berry,
        "den raid approach from known route",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        fork,
        "den raid return to contested food",
    );
    let remaining_when_full = state
        .available_surface_resources()
        .find(|source| source.commodity() == nuts)
        .expect("full den cannot consume the entire source")
        .remaining_mass();
    let used_food = initial
        .checked_sub(remaining_when_full)
        .expect("unattended patch must not grow");

    // Only adjacent exposed world parcels reveal the den; the fixture must not
    // read the colony's remote route. Pick the goods up via the actual control owner.
    let den = available_adjacent_surface_resources(&state)
        .find(|item| item.commodity() == nuts && item.position() != fork)
        .expect("scout must leave foraged goods visible in an adjacent den")
        .position();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        den,
        "recover full den hoard",
    );
    for _ in 0..24 {
        if available_player_world_drops(&state).next().is_none() {
            break;
        }
        let _ = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("den cache ordinary pickup failed: {error}"));
    }
    assert!(available_player_world_drops(&state).next().is_none());
    let hoard = state
        .inventory()
        .get_stockpile(carried)
        .unwrap()
        .stored_mass();
    assert_eq!(
        hoard,
        Mass::from_milligrams(500_000),
        "player recovers only the physically staged den parcels"
    );
    let eaten = calculate_matter_accounting(&state).unwrap().consumed();
    assert_eq!(
        u128::from(used_food.milligrams()),
        u128::from(hoard.milligrams()) + eaten.milligrams(),
        "foraged food is either recoverable in the den or conserved as consumed matter"
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );

    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        fork,
        "withdraw from emptied den",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        berry,
        "withdraw toward original spawn",
    );
    for &destination in &retreat {
        walk_continuous_body_adjacent_and_settle(
            &registries,
            &mut state,
            destination,
            "leave the emptied den unguarded on the observed route",
        );
    }
    let withdrawn_at = state.tick().value();
    advance_to(
        &registries,
        &mut state,
        SimulationTick::new(withdrawn_at + 50),
        TickEventAllowance::default(),
        "den reopens after player removes its cache",
    );
    for &destination in retreat.iter().rev().skip(1) {
        walk_continuous_body_adjacent_and_settle(
            &registries,
            &mut state,
            destination,
            "return from the off-trail observation camp",
        );
    }
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        berry,
        "inspect forage after den raid",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        fork,
        "verify new forage after cache cleared",
    );
    let after_reopening = state
        .available_surface_resources()
        .find(|source| source.commodity() == nuts)
        .expect("raided source should remain locally observable")
        .remaining_mass();
    let new_foraging = remaining_when_full
        .checked_sub(after_reopening)
        .expect("source cannot gain mass from a raided den");
    assert_eq!(new_foraging, Mass::from_milligrams(100_000));
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    if print {
        std::println!(
            "PLAYER JOURNEY seed=0x{:016X} episode=coverage/rested/scavenger-den phase=raid-full-den observation-camp=[recorded-route:{}legs off-graph:{}legs separation:{}voxels passive:{}t] observed=[source:{}mg reduced-to:{}mg cache:{}mg] choice=[recover-cache:ordinary-pickup] consequence=[carried:{}mg source-further-depleted:{}mg] matter-conserved=true world=ordinary-generated-opening",
            seed.value(),
            retreat.len(),
            retreat.len() - 2,
            distance_from_clearing(*retreat.last().unwrap(), fork),
            815 - unguarded_from,
            initial.milligrams(),
            remaining_when_full.milligrams(),
            hoard.milligrams(),
            hoard.milligrams(),
            new_foraging.milligrams(),
        );
        std::println!(
            "KOBOLD CACHE EXPERIENCE seed=0x{:016X} episode=coverage/rested/scavenger-den source-before:{}mg source-full:{}mg hoard:{}mg reclaimed:{}mg eaten:{}mg reopened:{}mg finite-storage=true matter-conserved=true",
            seed.value(),
            initial.milligrams(),
            remaining_when_full.milligrams(),
            hoard.milligrams(),
            hoard.milligrams(),
            eaten.milligrams(),
            new_foraging.milligrams(),
        );
    }
}
