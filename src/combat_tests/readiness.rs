//! Focused combat readiness and cross-owner equipment revision reservation proofs.

use super::*;

#[test]
fn melee_readiness_does_not_advertise_an_unaffordable_shove() {
    let (registries, mut state, _, _, tip) = opening_contact_world();
    validate_admit_player(&registries, &state, tip)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let normal = state.survival().player().copied().unwrap();
    assert!(assess_player_melee(&registries, &state).unwrap().ready());

    set_player_melee_reserves(&mut state, Energy::ZERO, normal.hydration());
    assert!(!assess_player_melee(&registries, &state).unwrap().ready());
    assert_eq!(
        validate_primitive_shove(&registries, &state, east()).err(),
        Some(PrimitiveAttackError::InsufficientMetabolicEnergy)
    );

    set_player_melee_reserves(&mut state, normal.metabolic_energy(), Volume::ZERO);
    assert!(!assess_player_melee(&registries, &state).unwrap().ready());
    assert_eq!(
        validate_primitive_shove(&registries, &state, east()).err(),
        Some(PrimitiveAttackError::InsufficientHydration)
    );
    assert_eq!(state.combat().revision(), 0);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn melee_hud_never_advertises_an_attack_with_exhausted_owner_revisions() {
    use crate::content::EQUIPMENT_WOODEN_CLUB;
    use crate::simulation::advance_control_tick;

    let (registries, mut state, _, _, tip) = opening_contact_world();
    validate_admit_player(&registries, &state, tip)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let last = state.control_tick();
    state
        .combat_state_mut()
        .apply_attack(0, last, SHOVE_RECOVERY_TICKS);
    for _ in 0..SHOVE_RECOVERY_TICKS {
        let _ = advance_control_tick(&registries, &mut state).unwrap();
    }
    assert!(assess_player_melee(&registries, &state).unwrap().ready());

    // The save admission contract permits a completed attack with an exhausted
    // combat cursor. Such a player still has metabolic reserves, but may not
    // authorize another strike until there is revision headroom.
    let mut exhausted = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    exhausted["state"]["systems"]["combat"]["revision"] = serde_json::json!(u64::MAX);
    let exhausted = serde_json::from_value::<LoadedSaveEnvelope>(exhausted)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(validate_loaded_state(&registries, &exhausted), Ok(()));
    assert!(
        !assess_player_melee(&registries, &exhausted)
            .unwrap()
            .ready(),
        "unarmed readiness must agree with the rejected combat revision"
    );
    assert_eq!(
        validate_primitive_shove(&registries, &exhausted, east()).err(),
        Some(PrimitiveAttackError::RevisionExhausted)
    );

    // A carried and sound weapon has a separate owner revision reservation.
    // Combat may be ready while tool wear cannot be committed.
    let (registries, mut armed, _, _, tip) = opening_contact_world();
    validate_admit_player(&registries, &armed, tip)
        .unwrap()
        .commit(&mut armed)
        .unwrap();
    arm_player(&registries, &mut armed, EQUIPMENT_WOODEN_CLUB);
    assert!(assess_player_melee(&registries, &armed).unwrap().ready());
    let mut exhausted = serde_json::to_value(SaveEnvelope::new(&registries, &armed)).unwrap();
    exhausted["state"]["systems"]["equipment"]["revision"] = serde_json::json!(u64::MAX);
    let exhausted = serde_json::from_value::<LoadedSaveEnvelope>(exhausted)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(validate_loaded_state(&registries, &exhausted), Ok(()));
    assert!(
        !assess_player_melee(&registries, &exhausted)
            .unwrap()
            .ready(),
        "armed readiness must agree with the rejected tool-wear revision"
    );
    assert_eq!(
        validate_primitive_strike(&registries, &exhausted, east()).err(),
        Some(PrimitiveAttackError::EquipmentRevisionExhausted)
    );
}

#[test]
fn armed_strike_preserves_equipment_revision_promised_to_running_production() {
    use crate::content::{
        ENERGY_ELECTRICAL_BUFFER, EQUIPMENT_ELECTRIC_FURNACE, EQUIPMENT_WOODEN_CLUB, FORM_LOG,
        MATERIAL_WOOD, PROCESS_HEAT_MATERIAL_BATCH, STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
    };
    use crate::core::quantity::{Area, Length, Mass, Temperature};
    use crate::energy::add_energy_store_with_initial_for_fixture;
    use crate::equipment::{add_equipment, validate_mount_equipment};
    use crate::inventory::{
        MaterialLotSelection, add_solid_stockpile_for_test, deposit_lot_for_test,
    };
    use crate::material::CommodityKey;
    use crate::simulation::{
        PlayerDirectControlInput, PrimaryActionOutcome, PrimaryControlIntent,
        advance_control_tick_with_direct_input,
    };
    use crate::spatial::VoxelBounds;
    use crate::structural::{
        add_structural_element, materialize_structural_element_for_test,
        validate_activate_structural_element,
    };
    use crate::thermal::{SensibleHeatingRequest, resolve_sensible_heating_process};

    let (registries, mut state, _, _, tip) = opening_contact_world();
    let source =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000)).unwrap();
    let destination =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000)).unwrap();
    let mass = Mass::from_milligrams(10_000);
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let furnace = add_equipment(
        &registries,
        &mut state,
        EQUIPMENT_ELECTRIC_FURNACE,
        Condition::PRISTINE,
    )
    .unwrap();
    let support = add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        crate::structural::make_test_structural_geometry(
            VoxelBounds::new(VoxelCoord::new(100, 0, 0), VoxelCoord::new(101, 1, 1)).unwrap(),
            Length::from_micrometers(1),
            Area::from_square_millimeters(500_000),
        ),
        true,
    )
    .unwrap();
    materialize_structural_element_for_test(&registries, &mut state, support, FORM_LOG);
    let _ = validate_activate_structural_element(&registries, &state, support)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let _ = validate_mount_equipment(&registries, &state, furnace, support)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let power = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        ENERGY_ELECTRICAL_BUFFER,
        Energy::from_nanojoules(2_000_000_000_000_000),
    )
    .unwrap();
    let resolved = resolve_sensible_heating_process(
        &registries,
        &state,
        SensibleHeatingRequest::new(
            PROCESS_HEAT_MATERIAL_BATCH,
            source,
            &[MaterialLotSelection::new(lot, mass)],
            furnace,
            power,
            Temperature::from_millikelvin(303_150),
        ),
    )
    .unwrap();
    let job = crate::production::validate_start_process(
        &registries,
        &state,
        resolved.process_resolution(),
        source,
        destination,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert!(state.production().get_job(job).is_some());
    assert_eq!(state.checked_future_equipment_revision_demand(), Some(1));

    validate_admit_player(&registries, &state, tip)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let weapon = arm_player(&registries, &mut state, EQUIPMENT_WOODEN_CLUB);
    assert!(assess_player_melee(&registries, &state).unwrap().ready());
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    encoded["state"]["systems"]["equipment"]["revision"] = serde_json::json!(u64::MAX - 1);
    let mut constrained = serde_json::from_value::<LoadedSaveEnvelope>(encoded)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(validate_loaded_state(&registries, &constrained), Ok(()));
    assert!(constrained.can_spend_equipment_revisions(0));
    assert!(!constrained.can_spend_equipment_revisions(1));
    assert!(
        !assess_player_melee(&registries, &constrained)
            .unwrap()
            .ready()
    );

    let west = ViewDirection::new(-1_000_000, 0, 0).unwrap();
    assert_eq!(
        validate_primitive_strike(&registries, &constrained, west).err(),
        Some(PrimitiveAttackError::EquipmentRevisionExhausted)
    );
    let before = constrained.clone();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(
        &serde_json::to_vec(&SaveEnvelope::new(&registries, &constrained)).unwrap(),
    )
    .unwrap()
    .into_state(&registries)
    .unwrap();
    let command =
        PlayerDirectControlInput::NEUTRAL.with_primary(PrimaryControlIntent::strike(west));
    let outcome =
        advance_control_tick_with_direct_input(&registries, &mut constrained, command).unwrap();
    assert_eq!(
        outcome,
        advance_control_tick_with_direct_input(&registries, &mut replay, command).unwrap()
    );
    assert_eq!(
        outcome.primary_action(),
        Some(&PrimaryActionOutcome::StrikeRejected(
            PrimitiveAttackError::EquipmentRevisionExhausted
        ))
    );
    assert_eq!(constrained, replay);
    assert_eq!(
        constrained.equipment().revision(),
        before.equipment().revision()
    );
    assert_eq!(
        constrained
            .equipment()
            .get_equipment(weapon)
            .unwrap()
            .condition(),
        before
            .equipment()
            .get_equipment(weapon)
            .unwrap()
            .condition()
    );
    assert!(constrained.production().get_job(job).is_some());
    assert_eq!(
        constrained.checked_future_equipment_revision_demand(),
        Some(1)
    );
    assert_eq!(validate_loaded_state(&registries, &constrained), Ok(()));
}

#[test]
fn prepared_readiness_uses_doubled_exertion_not_an_ordinary_swing() {
    use crate::content::EQUIPMENT_WOODEN_CLUB;
    use crate::simulation::advance_control_tick;

    let (registries, mut state, _, _, tip) = opening_contact_world();
    validate_admit_player(&registries, &state, tip)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    arm_player(&registries, &mut state, EQUIPMENT_WOODEN_CLUB);
    let normal = state.survival().player().copied().unwrap();
    let normal_cost = registries
        .equipment()
        .get_equipment(EQUIPMENT_WOODEN_CLUB)
        .unwrap()
        .melee_profile()
        .unwrap()
        .exertion_per_swing();
    set_player_melee_reserves(
        &mut state,
        normal_cost.energy_cost_per_tick(),
        normal.hydration(),
    );
    let west = ViewDirection::new(-1_000_000, 0, 0).unwrap();
    assert!(assess_player_melee(&registries, &state).unwrap().ready());
    let _ = validate_prepare_primitive_strike(&registries, &state, west)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    for _ in 0..preparation::PREPARATION_TICKS {
        let _ = advance_control_tick(&registries, &mut state).unwrap();
    }
    let assessment = assess_player_melee(&registries, &state).unwrap();
    assert_eq!(assessment.preparation_remaining_ticks(), Some(0));
    assert!(!assessment.prepared_release_ready());
    assert_eq!(
        validate_release_prepared_strike(&registries, &state, west).err(),
        Some(PrimitiveAttackError::InsufficientMetabolicEnergy)
    );
    let prepared_cost = normal_cost
        .energy_cost_per_tick()
        .checked_add(normal_cost.energy_cost_per_tick())
        .unwrap();
    set_player_melee_reserves(&mut state, prepared_cost, normal.hydration());
    assert!(
        assess_player_melee(&registries, &state)
            .unwrap()
            .prepared_release_ready()
    );
    assert!(validate_release_prepared_strike(&registries, &state, west).is_ok());
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let restored = serde_json::from_slice::<LoadedSaveEnvelope>(
        &serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap(),
    )
    .unwrap()
    .into_state(&registries)
    .unwrap();
    assert_eq!(restored, state);
    assert!(
        assess_player_melee(&registries, &restored)
            .unwrap()
            .prepared_release_ready()
    );
}
