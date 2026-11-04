//! Player hand custody cannot bypass a prepared melee strike.

use super::*;
use crate::combat::{validate_cancel_primitive_preparation, validate_prepare_primitive_strike};
use crate::content::{EQUIPMENT_WOODEN_CLUB, FORM_CHIP, MATERIAL_STONE, build_registries};
use crate::core::quantity::{Mass, Temperature};
use crate::equipment::validate_assemble_equipment;
use crate::inventory::deposit_lot_for_test;
use crate::logistics::{
    PlayerActiveSelection, PlayerEquipmentCarryCommitError, PlayerEquipmentCarryError,
    validate_drop_equipment, validate_pickup_equipment, validate_set_active_selection,
};
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::surface::{
    PlayerMaterialDropCommitError, PlayerMaterialDropError, validate_drop_player_material,
};

#[test]
fn prepared_strike_excludes_both_directions_of_equipment_custody_and_material_drop() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let carried = validate_admit_player(&registries, &state, position)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    let assembly = registries
        .equipment()
        .get_equipment(EQUIPMENT_WOODEN_CLUB)
        .unwrap()
        .assembly_profile()
        .unwrap();
    for input in assembly.inputs() {
        deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            input.commodity(),
            Mass::from_milligrams(input.mass().milligrams() * 2),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap();
    }
    let commodity = CommodityKey::new(MATERIAL_STONE, FORM_CHIP);
    let material_mass = Mass::from_milligrams(75_000);
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        commodity,
        material_mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let club = validate_assemble_equipment(&registries, &state, EQUIPMENT_WOODEN_CLUB, carried)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let spare = validate_assemble_equipment(&registries, &state, EQUIPMENT_WOODEN_CLUB, carried)
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

    let stale_drop = validate_drop_equipment(&registries, &state, club).unwrap();
    let stale_pickup = validate_pickup_equipment(&registries, &state, spare).unwrap();
    let stale_material = validate_drop_player_material(&registries, &state, commodity).unwrap();
    let matter_before = calculate_matter_accounting(&state).unwrap().total();
    let _ = validate_prepare_primitive_strike(&registries, &state, east())
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let prepared = state.clone();

    assert!(matches!(
        validate_drop_equipment(&registries, &state, club),
        Err(PlayerEquipmentCarryError::PreparedStrikeActive)
    ));
    assert!(matches!(
        validate_pickup_equipment(&registries, &state, spare),
        Err(PlayerEquipmentCarryError::PreparedStrikeActive)
    ));
    assert_eq!(
        validate_drop_player_material(&registries, &state, commodity).err(),
        Some(PlayerMaterialDropError::PreparedStrikeActive)
    );
    assert!(matches!(
        stale_drop.commit(&mut state),
        Err(PlayerEquipmentCarryCommitError::StaleCombatRevision { .. })
    ));
    assert!(matches!(
        stale_pickup.commit(&mut state),
        Err(PlayerEquipmentCarryCommitError::StaleCombatRevision { .. })
    ));
    assert!(matches!(
        stale_material.commit(&mut state),
        Err(PlayerMaterialDropCommitError::StaleCombat { .. })
    ));
    assert_eq!(state, prepared, "every rejected hand transfer is atomic");
    // The direct-control adapter reports the same authoritative rejection
    // instead of silently disposing of the selected weapon or carried material.
    use crate::simulation::{
        ActiveSelectionDropOutcome, PlayerDirectControlInput,
        advance_control_tick_with_direct_input,
    };
    let mut control = state.clone();
    let equipment_action = advance_control_tick_with_direct_input(
        &registries,
        &mut control,
        PlayerDirectControlInput::NEUTRAL.with_drop_active_selection(),
    )
    .unwrap();
    assert_eq!(
        equipment_action.active_selection_drop(),
        Some(&ActiveSelectionDropOutcome::EquipmentRejected(
            PlayerEquipmentCarryError::PreparedStrikeActive,
        ))
    );
    validate_set_active_selection(
        &registries,
        &control,
        Some(PlayerActiveSelection::material(commodity)),
    )
    .unwrap()
    .commit(&mut control)
    .unwrap();
    let material_action = advance_control_tick_with_direct_input(
        &registries,
        &mut control,
        PlayerDirectControlInput::NEUTRAL.with_drop_active_selection(),
    )
    .unwrap();
    assert_eq!(
        material_action.active_selection_drop(),
        Some(&ActiveSelectionDropOutcome::MaterialRejected(
            PlayerMaterialDropError::PreparedStrikeActive,
        ))
    );
    assert!(control.combat().has_prepared_strike());
    assert!(control.logistics().is_equipment_carried(club));
    assert_eq!(validate_loaded_state(&registries, &control), Ok(()));
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        matter_before
    );

    let serialized = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&serialized)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(state, replay);
    for game in [&mut state, &mut replay] {
        let _ = validate_cancel_primitive_preparation(game)
            .unwrap()
            .commit(game)
            .unwrap();
        validate_drop_equipment(&registries, game, club)
            .unwrap()
            .commit(game)
            .unwrap();
        validate_pickup_equipment(&registries, game, spare)
            .unwrap()
            .commit(game)
            .unwrap();
        let drop = validate_drop_player_material(&registries, game, commodity)
            .unwrap()
            .commit(game)
            .unwrap();
        assert_eq!(drop.dropped_mass(), material_mass);
        assert_eq!(validate_loaded_state(&registries, game), Ok(()));
        assert_eq!(
            calculate_matter_accounting(game).unwrap().total(),
            matter_before
        );
    }
    assert_eq!(state, replay);
}
