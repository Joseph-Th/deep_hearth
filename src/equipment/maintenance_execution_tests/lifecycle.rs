//! Maintenance admission, death interruption, revision headroom, and continuation contracts.

use super::*;

#[test]
fn authored_maintenance_resolution_binds_exact_replacement_stock_and_service_target() {
    let registries = registries();
    let mut state = AppState::new();
    initialize_service_player(&registries, &mut state);
    let equipment = add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000))
        .unwrap_or_else(|error| panic!("maintenance resolver equipment fixture failed: {error}"));
    let second_equipment =
        add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000)).unwrap_or_else(
            |error| panic!("second maintenance resolver equipment fixture failed: {error}"),
        );
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("maintenance resolver source fixture failed: {error}"));
    let spent = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("maintenance resolver spent fixture failed: {error}"));
    add_material(&registries, &mut state, source, Mass::from_milligrams(20));

    let resolution = resolve_equipment_maintenance(
        &registries,
        &state,
        EquipmentMaintenanceRequest::new(equipment, source, spent),
    )
    .unwrap_or_else(|error| panic!("maintenance resolution failed: {error}"));
    assert_eq!(resolution.equipment(), equipment);
    assert_eq!(resolution.material_source(), source);
    assert_eq!(resolution.spent_destination(), spent);
    assert_eq!(
        resolution.spent_commodity(),
        CommodityKey::new(MATERIAL_WOOD, FORM_CHIP)
    );
    assert_eq!(
        resolution.material_mass(),
        Mass::from_milligrams(2),
        "partial maintenance must scale replacement stock with restored condition"
    );
    assert_eq!(resolution.condition_before(), condition(500_000));
    assert_eq!(resolution.condition_after(), condition(700_000));

    let outcome = validate_equipment_maintenance(&registries, &state, resolution)
        .unwrap_or_else(|error| panic!("maintenance transaction validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("maintenance transaction commit failed: {error}"));
    assert_eq!(outcome.condition_before(), condition(500_000));
    assert_eq!(outcome.target_condition(), condition(700_000));
    assert_eq!(outcome.material_mass(), Mass::from_milligrams(2));
    assert_eq!(
        state
            .equipment()
            .get_equipment(equipment)
            .map(|record| record.condition()),
        Some(condition(500_000)),
        "service admission must not grant condition recovery before labor completes"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .map(|record| record.stored_mass()),
        Some(Mass::from_milligrams(18))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(spent)
            .map(|record| record.stored_mass()),
        Some(Mass::from_milligrams(2))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(spent)
            .map(|record| record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_LOG))),
        Some(Mass::ZERO),
        "spent maintenance output must not remain reusable replacement stock"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(spent)
            .map(|record| record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_CHIP))),
        Some(Mass::from_milligrams(2)),
        "maintenance must conserve the selected matter in the authored spent form"
    );
    assert_eq!(
        resolve_equipment_maintenance(
            &registries,
            &state,
            EquipmentMaintenanceRequest::new(second_equipment, spent, source),
        ),
        Err(
            EquipmentMaintenanceResolutionError::InsufficientReplacementMaterial {
                stockpile: spent,
                commodity: CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
                available: Mass::ZERO,
                required: Mass::from_milligrams(2),
            }
        ),
        "spent maintenance output must not service another worn machine"
    );
    let completion = finish_service(&registries, &mut state, outcome.completes_at());
    assert_eq!(completion.condition_after(), condition(700_000));
}

#[test]
fn fatal_tick_interrupts_unfinished_maintenance_without_refunding_committed_service_material() {
    let registries = registries_with_service_duration(TickSpan::new(6));
    let mut state = AppState::new();
    initialize_service_player(&registries, &mut state);
    let equipment = add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000))
        .unwrap_or_else(|error| panic!("fatal maintenance equipment fixture failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("fatal maintenance source fixture failed: {error}"));
    let spent = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("fatal maintenance spent fixture failed: {error}"));
    add_material(&registries, &mut state, source, Mass::from_milligrams(20));

    let resolution = resolve_equipment_maintenance(
        &registries,
        &state,
        EquipmentMaintenanceRequest::new(equipment, source, spent),
    )
    .unwrap_or_else(|error| panic!("fatal maintenance resolution failed: {error}"));
    let start = validate_equipment_maintenance(&registries, &state, resolution)
        .unwrap_or_else(|error| panic!("fatal maintenance validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("fatal maintenance commit failed: {error}"));
    assert!(start.completes_at().value() > state.tick().value() + 1);
    assert_eq!(
        state
            .equipment()
            .get_equipment(equipment)
            .map(|record| record.condition()),
        Some(start.condition_before())
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .map(|record| record.stored_mass()),
        Some(Mass::from_milligrams(18))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(spent)
            .map(|record| record.stored_mass()),
        Some(Mass::from_milligrams(2))
    );
    make_next_tick_fatal(&registries, &mut state);

    let outcome = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("fatal maintenance tick failed: {error}"));

    assert_eq!(outcome.equipment_maintenance(), None);
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state.survival().player().map(|player| player.vitality()),
        Some(Vitality::ZERO)
    );
    assert_eq!(
        state
            .equipment()
            .get_equipment(equipment)
            .map(|record| record.condition()),
        Some(start.condition_before()),
        "interrupted maintenance must not grant the deferred condition recovery"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .map(|record| record.stored_mass()),
        Some(Mass::from_milligrams(18)),
        "service material committed at admission is not refunded after interruption"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(spent)
            .map(|record| record.stored_mass()),
        Some(Mass::from_milligrams(2)),
        "represented spent material remains in custody after interrupted service"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let post_death = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("post-death maintenance tick failed: {error}"));
    assert_eq!(post_death.equipment_maintenance(), None);
    assert_eq!(
        state
            .equipment()
            .get_equipment(equipment)
            .map(|record| record.condition()),
        Some(start.condition_before())
    );
}

#[test]
fn maintenance_due_on_fatal_tick_completes_before_player_work_is_released() {
    let registries = registries();
    let mut state = AppState::new();
    initialize_service_player(&registries, &mut state);
    let equipment = add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000))
        .unwrap_or_else(|error| panic!("fatal due maintenance equipment fixture failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("fatal due maintenance source fixture failed: {error}"));
    let spent = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("fatal due maintenance spent fixture failed: {error}"));
    add_material(&registries, &mut state, source, Mass::from_milligrams(20));
    let resolution = resolve_equipment_maintenance(
        &registries,
        &state,
        EquipmentMaintenanceRequest::new(equipment, source, spent),
    )
    .unwrap_or_else(|error| panic!("fatal due maintenance resolution failed: {error}"));
    let start = validate_equipment_maintenance(&registries, &state, resolution)
        .unwrap_or_else(|error| panic!("fatal due maintenance validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("fatal due maintenance commit failed: {error}"));
    assert_eq!(start.completes_at().value(), state.tick().value() + 1);
    make_next_tick_fatal(&registries, &mut state);

    let outcome = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("fatal due maintenance tick failed: {error}"));

    assert_eq!(
        outcome
            .equipment_maintenance()
            .map(EquipmentMaintenanceOutcome::condition_after),
        Some(start.target_condition())
    );
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state.survival().player().map(|player| player.vitality()),
        Some(Vitality::ZERO)
    );
    assert_eq!(
        state
            .equipment()
            .get_equipment(equipment)
            .map(|record| record.condition()),
        Some(start.target_condition())
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn maintenance_rejects_revision_budget_that_cannot_complete_before_any_mutation() {
    let registries = registries();
    let mut state = AppState::new();
    initialize_service_player(&registries, &mut state);
    let equipment = add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000))
        .unwrap_or_else(|error| panic!("revision-budget maintenance equipment failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("revision-budget maintenance source failed: {error}"));
    let spent = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("revision-budget maintenance spent failed: {error}"));
    add_material(&registries, &mut state, source, Mass::from_milligrams(20));

    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("revision-budget maintenance serialization failed: {error}")
        });
    encoded["state"]["systems"]["equipment"]["revision"] = serde_json::json!(u64::MAX - 1);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("revision-budget maintenance decode failed: {error}"));
    let loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("near-exhausted maintenance revision fixture should load: {error}")
    });
    let resolution = resolve_equipment_maintenance(
        &registries,
        &loaded,
        EquipmentMaintenanceRequest::new(equipment, source, spent),
    )
    .unwrap_or_else(|error| panic!("revision-budget maintenance resolution failed: {error}"));
    let before = loaded.clone();

    assert_eq!(
        validate_equipment_maintenance(&registries, &loaded, resolution).err(),
        Some(EquipmentMaintenanceError::EquipmentRevisionExhausted)
    );
    assert_eq!(loaded, before);
}

#[test]
fn trusted_load_rejects_active_maintenance_without_completion_equipment_revision() {
    let registries = registries_with_service_duration(TickSpan::new(6));
    let mut state = AppState::new();
    initialize_service_player(&registries, &mut state);
    let equipment = add_equipment(&registries, &mut state, TEST_DEFINITION, Condition::FAILED)
        .unwrap_or_else(|error| panic!("maintenance load-budget equipment failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("maintenance load-budget source failed: {error}"));
    let spent = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("maintenance load-budget spent failed: {error}"));
    add_material(&registries, &mut state, source, Mass::from_milligrams(7));
    let resolution = resolve_equipment_maintenance(
        &registries,
        &state,
        EquipmentMaintenanceRequest::new(equipment, source, spent),
    )
    .unwrap_or_else(|error| panic!("maintenance load-budget resolution failed: {error}"));
    let _ = validate_equipment_maintenance(&registries, &state, resolution)
        .unwrap_or_else(|error| panic!("maintenance load-budget validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("maintenance load-budget commit failed: {error}"));

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("maintenance load-budget serialization failed: {error}"));
    encoded["state"]["systems"]["equipment"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("maintenance load-budget decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::PlayerWork(
            PlayerWorkValidationError::EquipmentMaintenanceEquipmentRevisionExhausted,
        )))
    );
}

#[test]
fn maintenance_start_preserves_equipment_revision_owed_to_running_production() {
    let registries = occupied_registries();
    let mut state = AppState::new();
    initialize_service_player(&registries, &mut state);
    let production_equipment =
        add_equipment(&registries, &mut state, TEST_DEFINITION, condition(700_000))
            .unwrap_or_else(|error| panic!("shared-budget production equipment failed: {error}"));
    let maintenance_equipment =
        add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000))
            .unwrap_or_else(|error| panic!("shared-budget maintenance equipment failed: {error}"));
    let process_source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("shared-budget process source failed: {error}"));
    let process_destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("shared-budget process destination failed: {error}"));
    let maintenance_source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2))
        .unwrap_or_else(|error| panic!("shared-budget maintenance source failed: {error}"));
    let spent = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2))
        .unwrap_or_else(|error| panic!("shared-budget spent destination failed: {error}"));
    let process_lot = add_material(
        &registries,
        &mut state,
        process_source,
        Mass::from_milligrams(10),
    );
    add_material(
        &registries,
        &mut state,
        maintenance_source,
        Mass::from_milligrams(2),
    );
    let energy_store = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        ENERGY_DEFINITION,
        Energy::from_nanojoules(1_000_000_000),
    )
    .unwrap_or_else(|error| panic!("shared-budget production energy failed: {error}"));
    let heating = resolve_sensible_heating_process(
        &registries,
        &state,
        SensibleHeatingRequest::new(
            HEATING_PROCESS,
            process_source,
            &[MaterialLotSelection::new(
                process_lot,
                Mass::from_milligrams(10),
            )],
            production_equipment,
            energy_store,
            Temperature::from_millikelvin(301_000),
        ),
    )
    .unwrap_or_else(|error| panic!("shared-budget heating resolution failed: {error}"));
    validate_start_process(
        &registries,
        &state,
        heating.process_resolution(),
        process_source,
        process_destination,
    )
    .unwrap_or_else(|error| panic!("shared-budget production start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("shared-budget production commit failed: {error}"));

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("shared-budget maintenance serialization failed: {error}"));
    encoded["state"]["systems"]["equipment"]["revision"] = serde_json::json!(u64::MAX - 2);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("shared-budget maintenance decode failed: {error}"));
    let loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("one running production wear revision must fit at equipment MAX-2: {error}")
    });
    let resolution = resolve_equipment_maintenance(
        &registries,
        &loaded,
        EquipmentMaintenanceRequest::new(maintenance_equipment, maintenance_source, spent),
    )
    .unwrap_or_else(|error| panic!("shared-budget maintenance resolution failed: {error}"));
    let before = loaded.clone();

    assert_eq!(
        validate_equipment_maintenance(&registries, &loaded, resolution).err(),
        Some(EquipmentMaintenanceError::EquipmentRevisionExhausted)
    );
    assert_eq!(loaded, before);
}

#[test]
fn trusted_load_rejects_combined_production_and_maintenance_equipment_revision_overcommit() {
    let registries = occupied_registries();
    let mut state = AppState::new();
    initialize_service_player(&registries, &mut state);
    let production_equipment =
        add_equipment(&registries, &mut state, TEST_DEFINITION, condition(700_000))
            .unwrap_or_else(|error| panic!("combined-load production equipment failed: {error}"));
    let maintenance_equipment =
        add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000))
            .unwrap_or_else(|error| panic!("combined-load maintenance equipment failed: {error}"));
    let process_source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("combined-load process source failed: {error}"));
    let process_destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("combined-load process destination failed: {error}"));
    let maintenance_source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2))
        .unwrap_or_else(|error| panic!("combined-load maintenance source failed: {error}"));
    let spent = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2))
        .unwrap_or_else(|error| panic!("combined-load spent destination failed: {error}"));
    let process_lot = add_material(
        &registries,
        &mut state,
        process_source,
        Mass::from_milligrams(10),
    );
    add_material(
        &registries,
        &mut state,
        maintenance_source,
        Mass::from_milligrams(2),
    );
    let energy_store = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        ENERGY_DEFINITION,
        Energy::from_nanojoules(1_000_000_000),
    )
    .unwrap_or_else(|error| panic!("combined-load production energy failed: {error}"));
    let heating = resolve_sensible_heating_process(
        &registries,
        &state,
        SensibleHeatingRequest::new(
            HEATING_PROCESS,
            process_source,
            &[MaterialLotSelection::new(
                process_lot,
                Mass::from_milligrams(10),
            )],
            production_equipment,
            energy_store,
            Temperature::from_millikelvin(301_000),
        ),
    )
    .unwrap_or_else(|error| panic!("combined-load heating resolution failed: {error}"));
    validate_start_process(
        &registries,
        &state,
        heating.process_resolution(),
        process_source,
        process_destination,
    )
    .unwrap_or_else(|error| panic!("combined-load production start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("combined-load production commit failed: {error}"));
    let maintenance = resolve_equipment_maintenance(
        &registries,
        &state,
        EquipmentMaintenanceRequest::new(maintenance_equipment, maintenance_source, spent),
    )
    .unwrap_or_else(|error| panic!("combined-load maintenance resolution failed: {error}"));
    let _ = validate_equipment_maintenance(&registries, &state, maintenance)
        .unwrap_or_else(|error| panic!("combined-load maintenance validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("combined-load maintenance commit failed: {error}"));

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("combined-load serialization failed: {error}"));
    encoded["state"]["systems"]["equipment"]["revision"] = serde_json::json!(u64::MAX - 1);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("combined-load decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::FutureEquipmentRevisionCapacityExhausted {
                revision: u64::MAX - 1,
                required: 2,
            }
        ))
    );
}

#[test]
fn in_progress_maintenance_round_trip_preserves_material_payment_and_continuation() {
    let registries = registries_with_service_duration(TickSpan::new(6));
    let mut state = AppState::new();
    initialize_service_player(&registries, &mut state);
    let equipment = add_equipment(&registries, &mut state, TEST_DEFINITION, Condition::FAILED)
        .unwrap_or_else(|error| panic!("maintenance continuation equipment failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("maintenance continuation source failed: {error}"));
    let spent = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("maintenance continuation spent failed: {error}"));
    add_material(&registries, &mut state, source, Mass::from_milligrams(7));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("maintenance continuation matter audit failed: {error}"))
        .total();

    let resolution = resolve_equipment_maintenance(
        &registries,
        &state,
        EquipmentMaintenanceRequest::new(equipment, source, spent),
    )
    .unwrap_or_else(|error| panic!("maintenance continuation resolution failed: {error}"));
    assert_eq!(resolution.duration(), TickSpan::new(6));
    assert_eq!(resolution.material_mass(), Mass::from_milligrams(7));
    let start = validate_equipment_maintenance(&registries, &state, resolution)
        .unwrap_or_else(|error| panic!("maintenance continuation validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("maintenance continuation commit failed: {error}"));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .map(|record| record.stored_mass()),
        Some(Mass::ZERO),
        "replacement stock must leave reusable inventory at maintenance admission"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(spent)
            .map(|record| record.stored_mass()),
        Some(Mass::from_milligrams(7)),
        "admitted maintenance must persist paid replacement matter in its spent form"
    );

    for _ in 0..2 {
        let outcome = advance_tick(&registries, &mut state).unwrap_or_else(|error| {
            panic!("maintenance continuation pre-save tick failed: {error}")
        });
        assert_eq!(outcome.equipment_maintenance(), None);
    }
    assert!(state.tick() < start.completes_at());
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("maintenance continuation serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("maintenance continuation decode failed: {error}"));
    let mut loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("maintenance continuation trusted load failed: {error}"));
    assert_eq!(loaded, state);

    while state.tick() < start.completes_at() {
        let expected = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("maintenance continuation source tick failed: {error}"));
        let actual = advance_tick(&registries, &mut loaded)
            .unwrap_or_else(|error| panic!("maintenance continuation loaded tick failed: {error}"));
        assert_eq!(actual, expected);
    }
    assert_eq!(loaded, state);
    assert_eq!(
        state
            .equipment()
            .get_equipment(equipment)
            .map(|record| record.condition()),
        Some(condition(700_000))
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!(
                "maintenance continuation final matter audit failed: {error}"
            ))
            .total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
