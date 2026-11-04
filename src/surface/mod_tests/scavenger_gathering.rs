//! A pending player hand-harvest owns its exact surface source until completion.

use super::*;

#[test]
fn scavenger_cannot_relocate_split_or_abrade_a_reserved_hand_gathering_drop() {
    let registries = build_registries();
    let mut state = AppState::new();
    let player = VoxelCoord::new(0, 0, 0);
    let east = VoxelCoord::new(1, 0, 0);
    let remote = VoxelCoord::new(2, 0, 0);
    let remote_east = VoxelCoord::new(3, 0, 0);
    let make_drop = |position, mass| {
        SurfaceResourceSpec::new(
            SurfaceResourceKind::Drop,
            position,
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(mass),
            TEMPERATURE,
            MaterialComposition::pure(MATERIAL_STONE),
        )
        .unwrap()
    };
    let reserved =
        insert_surface_resource_for_test(&registries, &mut state, make_drop(player, 100_000))
            .unwrap();
    let unrelated =
        insert_surface_resource_for_test(&registries, &mut state, make_drop(remote, 40_000))
            .unwrap();
    let split_source =
        insert_surface_resource_for_test(&registries, &mut state, make_drop(player, 60_000))
            .unwrap();
    let carried = admit_player(&registries, &mut state, player);
    let promised = Mass::from_milligrams(25_000);
    validate_start_player_surface_gathering(
        &registries,
        &state,
        SURFACE_GATHERING_HAND_COLLECT_STONE,
        reserved,
        promised,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let due = match state.player_work().active() {
        Some(crate::labor::PlayerWork::SurfaceGathering { work }) => work.completes_at(),
        other => panic!("test should begin real player gathering: {other:?}"),
    };
    let before = state.clone();
    let total = calculate_matter_accounting(&state).unwrap().total();
    let at = state.tick();

    assert!(
        !relocate_scavenged_drop_step(&registries, &mut state, reserved, player, east),
        "scout cannot move the player's reserved surface source"
    );
    assert_eq!(state, before);
    assert!(
        split_bulk_scavenged_drop_step(
            &registries,
            &mut state,
            reserved,
            player,
            east,
            at,
            Mass::from_milligrams(10_000),
        )
        .is_none(),
        "scout cannot divide the material already promised to player gathering"
    );
    assert_eq!(state, before);
    assert!(
        !abrade_scavenged_tool_head(
            &registries,
            &mut state,
            reserved,
            player,
            at,
            Mass::from_milligrams(1_000),
            FORM_CHIP,
        ),
        "a scout cannot abrade the player's reserved source"
    );
    assert_eq!(state, before);
    assert!(
        split_bulk_scavenged_drop_with_head_wear(
            &registries,
            &mut state,
            ScavengedPortionRequest::new(
                split_source,
                player,
                east,
                at,
                Mass::from_milligrams(10_000),
            ),
            reserved,
            Mass::from_milligrams(1_000),
            FORM_CHIP,
        )
        .is_none(),
        "scout cannot abrade the reserved tool head while splitting different loose matter"
    );
    assert_eq!(state, before);

    assert!(
        relocate_scavenged_drop_step(&registries, &mut state, unrelated, remote, remote_east),
        "independent unreserved parcels remain physically movable"
    );
    assert_eq!(state.surface().get(reserved).unwrap().position(), player);
    assert_eq!(
        state.surface().get(reserved).unwrap().remaining_mass(),
        Mass::from_milligrams(100_000)
    );
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), total);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    while state.tick() < due {
        assert_eq!(
            advance_control_tick(&registries, &mut state).unwrap(),
            advance_control_tick(&registries, &mut replay).unwrap(),
        );
    }
    assert_eq!(state, replay);
    assert_eq!(state.player_work().active(), None);
    assert!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(CommodityKey::new(MATERIAL_STONE, FORM_LUMP))
            >= promised
    );
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), total);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn scavenger_cannot_forage_food_reserved_for_a_player_harvest() {
    let registries = build_registries();
    let mut state = AppState::new();
    let player = VoxelCoord::new(0, 0, 0);
    let east = VoxelCoord::new(1, 0, 0);
    let reserved = insert_surface_resource_for_test(
        &registries,
        &mut state,
        SurfaceResourceSpec::new(
            SurfaceResourceKind::Opportunity,
            player,
            CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
            Mass::from_milligrams(300_000),
            TEMPERATURE,
            MaterialComposition::pure(MATERIAL_BERRIES),
        )
        .unwrap(),
    )
    .unwrap();
    let _ = admit_player(&registries, &mut state, player);
    validate_start_player_surface_gathering(
        &registries,
        &state,
        SURFACE_GATHERING_HAND_FORAGE_BERRIES,
        reserved,
        Mass::from_milligrams(25_000),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let before = state.clone();
    let total = calculate_matter_accounting(&state).unwrap().total();
    let at = state.tick();
    assert!(
        forage_food_to_adjacent_den(
            &registries,
            &mut state,
            reserved,
            player,
            east,
            at,
            Mass::from_milligrams(10_000),
        )
        .is_none(),
        "scout forage cannot preempt a player's admitted harvest"
    );
    assert_eq!(state, before);
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), total);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
