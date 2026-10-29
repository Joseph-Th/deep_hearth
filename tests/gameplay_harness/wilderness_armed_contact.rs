//! An ordinary generated-world actor builds and uses one finite club against the live patrol.
//! Primitive contact is presently nonlethal: this proves custody, combat, and avoidance,
//! not a virtual injury or an equipped enemy.

use super::*;
use deep_hearth::combat::{PrimitiveAttackOutcome, assess_player_melee};
use deep_hearth::content::EQUIPMENT_WOODEN_CLUB;
use deep_hearth::core::quantity::Volume;
use deep_hearth::equipment::validate_assemble_equipment;
use deep_hearth::kobolds::{KoboldRole, available_nearby_kobold_scouts};
use deep_hearth::logistics::validate_set_active_selection;
use deep_hearth::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use deep_hearth::simulation::{
    PlayerDirectControlInput, PrimaryActionOutcome, PrimaryControlIntent,
    advance_control_tick_with_direct_input,
};
use deep_hearth::surface::{available_adjacent_surface_resources, validate_drop_player_material};

pub(super) fn exercise_generated_player_club_defense(print: bool) {
    let registries = build_registries();
    let mut state = AppState::new();
    let seed = WILDERNESS_WORLD_SEED;
    let spawn = generated_wilderness_surface_spawn(&registries, &mut state, seed);
    validate_generate_initial_wilderness(&registries, &state, seed, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let carried = validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    let matter_before = calculate_matter_accounting(&state).unwrap().total();
    let fork = adjacent_surface_resource(&state, CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD))
        .position();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        fork,
        "club scout approach via forage fork",
    );
    let junction =
        adjacent_surface_resource(&state, CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD))
            .position();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        junction,
        "club construction worksite",
    );
    let timber = adjacent_surface_resource(&state, CommodityKey::new(MATERIAL_WOOD, FORM_LOG));
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        timber.position(),
        "acquire real club timber",
    );
    gather_in_batches(
        &registries,
        &mut state,
        timber.resource(),
        Mass::from_milligrams(1_000_000),
        "gather club timber",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        junction,
        "assemble club at worksite",
    );
    let club = validate_assemble_equipment(&registries, &state, EQUIPMENT_WOODEN_CLUB, carried)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_pickup_equipment(&registries, &state, club)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::equipment(club)),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    // Actual threatened provisions make the confrontation an economic choice:
    // an unattended actor will lose this exact parcel to the same scout,
    // while mere physical presence can avert theft without needing a weapon.
    let nuts = CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD);
    let ration = Mass::from_milligrams(250_000);
    let local_nuts = local_surface_resource(&state, nuts);
    gather(
        &registries,
        &mut state,
        local_nuts,
        ration,
        "provision threatened scout cache",
    );
    let dropped = validate_drop_player_material(&registries, &state, nuts)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(dropped.dropped_mass(), ration);
    let parcel = dropped.resources()[0];
    assert!(assess_player_melee(&registries, &state).unwrap().ready());
    let pristine = state.equipment().get_equipment(club).unwrap().condition();
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        matter_before
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        fork,
        "leave scout approach clear",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        spawn,
        "await a physical scout visit",
    );
    assert!(
        state.tick().value() < 180,
        "club acquisition must finish before the opening scout's first patrol"
    );
    advance_to(
        &registries,
        &mut state,
        SimulationTick::new(180),
        TickEventAllowance::default(),
        "first real scout patrol",
    );
    let mut unattended = state.clone();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        fork,
        "approach live scout with club",
    );
    let observed = available_nearby_kobold_scouts(&registries, &state)
        .find(|resident| resident.role() == KoboldRole::Scout)
        .expect("player at the fork must observe the genuine scout");
    assert_eq!(observed.position(), junction);
    assert_eq!(observed.balance_ppm(), Some(1_000_000));
    let to_scout = ViewDirection::new(
        i32::try_from(junction.x() - fork.x()).unwrap() * VIEW_DIRECTION_PARTS_PER_MILLION,
        0,
        i32::try_from(junction.z() - fork.z()).unwrap() * VIEW_DIRECTION_PARTS_PER_MILLION,
    )
    .unwrap();

    // The two alternatives share every prior world fact, including the club's
    // custody. Changing the selected hand makes an ordinary unarmed shove,
    // not a second materialization of the weapon or a fixture repositioning.
    let mut unarmed = state.clone();
    validate_set_active_selection(&registries, &unarmed, None)
        .unwrap()
        .commit(&mut unarmed)
        .unwrap();
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let armed_input =
        PlayerDirectControlInput::NEUTRAL.with_primary(PrimaryControlIntent::strike(to_scout));
    let shove_input =
        PlayerDirectControlInput::NEUTRAL.with_primary(PrimaryControlIntent::shove(to_scout));
    let before_survival = state.survival().player().unwrap().clone();
    let armed =
        advance_control_tick_with_direct_input(&registries, &mut state, armed_input).unwrap();
    let replayed =
        advance_control_tick_with_direct_input(&registries, &mut replay, armed_input).unwrap();
    assert_eq!(
        armed, replayed,
        "prepared world must produce identical physical control outcome"
    );
    assert_eq!(state, replay);
    let shove =
        advance_control_tick_with_direct_input(&registries, &mut unarmed, shove_input).unwrap();
    assert!(
        matches!(
            armed.primary_action(),
            Some(PrimaryActionOutcome::Strike(PrimitiveAttackOutcome::Startled {
                resident: KoboldRole::Scout, position,
            })) if *position == junction
        ),
        "club needs real unobstructed contact: {:?}",
        armed.primary_action()
    );
    assert!(
        matches!(
            shove.primary_action(),
            Some(PrimaryActionOutcome::Shove(PrimitiveAttackOutcome::Startled {
                resident: KoboldRole::Scout, position,
            })) if *position == junction
        ),
        "bare hands need the same physically reachable resident: {:?}",
        shove.primary_action()
    );
    let armed_scout = available_nearby_kobold_scouts(&registries, &state)
        .find(|resident| resident.role() == KoboldRole::Scout)
        .unwrap();
    let unarmed_scout = available_nearby_kobold_scouts(&registries, &unarmed)
        .find(|resident| resident.role() == KoboldRole::Scout)
        .unwrap();
    assert_eq!(armed_scout.balance_ppm(), Some(720_000));
    assert_eq!(unarmed_scout.balance_ppm(), Some(920_000));
    assert_eq!(
        state
            .equipment()
            .get_equipment(club)
            .unwrap()
            .condition()
            .parts_per_million(),
        pristine.parts_per_million() - 3_000,
        "contact must expend exactly one real club's authored wear",
    );
    assert_eq!(
        unarmed.equipment().get_equipment(club).unwrap().condition(),
        pristine
    );
    assert_eq!(
        before_survival
            .metabolic_energy()
            .checked_sub(state.survival().player().unwrap().metabolic_energy()),
        Some(Energy::from_nanojoules(180_000_000_000)),
        "the player's club strike must spend its exact 180 J authored exertion",
    );
    assert_eq!(
        before_survival
            .hydration()
            .checked_sub(state.survival().player().unwrap().hydration()),
        Some(Volume::from_microliters(12)),
        "the player's club strike must spend its exact 12 microliters",
    );
    assert_eq!(
        before_survival
            .metabolic_energy()
            .checked_sub(unarmed.survival().player().unwrap().metabolic_energy()),
        Some(Energy::from_nanojoules(90_000_000_000)),
        "the matched unarmed shove must cost 90 J without wearing the club",
    );
    for candidate in [&state, &unarmed] {
        assert_eq!(
            calculate_matter_accounting(candidate).unwrap().total(),
            matter_before
        );
        assert_eq!(validate_loaded_state(&registries, candidate), Ok(()));
    }
    // The victim's real schedule must continue after the contact. Attack history,
    // recovery and the scout's balance must all persist across saved control ticks.
    for _ in 0..360 {
        let original = advance_control_tick(&registries, &mut state).unwrap();
        let saved = advance_control_tick(&registries, &mut replay).unwrap();
        assert_eq!(original, saved);
    }
    while !state.control_tick().is_simulation_boundary() {
        assert_eq!(
            advance_control_tick(&registries, &mut state).unwrap(),
            advance_control_tick(&registries, &mut replay).unwrap(),
        );
    }
    assert_eq!(state, replay);
    assert!(assess_player_melee(&registries, &state).unwrap().ready());
    assert!(
        available_adjacent_surface_resources(&state).any(|resource| resource.resource() == parcel),
        "armed presence must leave the original finite ration at the threatened junction",
    );
    assert!(
        available_adjacent_surface_resources(&unarmed)
            .any(|resource| resource.resource() == parcel),
        "unarmed presence must be enough to deter this opportunistic scout",
    );
    while unarmed.control_tick() < state.control_tick() {
        let _ = advance_control_tick(&registries, &mut unarmed).unwrap();
    }
    assert_eq!(unarmed.control_tick(), state.control_tick());
    advance_to(
        &registries,
        &mut unattended,
        state.tick(),
        TickEventAllowance::default(),
        "unattended scout theft counterfactual",
    );
    // Observe the *consequence* only on the same physical return route.
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        junction,
        "armed camp reclaims protected ration",
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(nuts),
        ration,
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut unarmed,
        junction,
        "bare-hand camp reclaims protected ration",
    );
    assert_eq!(
        unarmed
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(nuts),
        ration,
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut unattended,
        fork,
        "unattended player returns to abandoned cache",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut unattended,
        junction,
        "unattended player finds cache missing",
    );
    assert_eq!(
        unattended
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(nuts),
        Mass::ZERO,
        "the unattended player must not receive the stolen parcel remotely",
    );
    let den = available_adjacent_surface_resources(&unattended)
        .find(|resource| resource.resource() == parcel)
        .expect("same missing ration must be observable next to the worksite")
        .position();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut unattended,
        den,
        "recover stolen ration from den",
    );
    assert_eq!(
        unattended
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(nuts),
        ration,
    );
    for candidate in [&state, &unarmed, &unattended] {
        assert_eq!(
            calculate_matter_accounting(candidate).unwrap().total(),
            matter_before
        );
        assert_eq!(validate_loaded_state(&registries, candidate), Ok(()));
    }
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    if print {
        std::println!(
            "PLAYER CLUB DEFENSE seed=0x{:016X} episode=coverage/rested/scavenger-armed-contact evidence=ordinary-generated-world weapon=player-gathered-timber+assembled+carried+selected contact=direct-control+unobstructed result=[club-balance:720000ppm shove-balance:920000ppm club-wear:3000ppm] cache=[exposed:250000mg club-protected:250000mg shove-protected:250000mg unattended-stolen:250000mg reclaimed:250000mg] exertion=finite recovery=canonical replay=360-control-ticks matter=conserved injury=not-yet-implemented",
            seed.value(),
        );
    }
}
