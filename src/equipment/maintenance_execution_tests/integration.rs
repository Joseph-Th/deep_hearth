//! Maintenance soak, occupancy races, production integration, and reservation contracts.

use super::*;

#[cfg(feature = "test-soak")]
#[test]
#[ignore = "long-horizon soak"]
fn equipment_maintenance_soak_preserves_timed_service_resources_and_replay() {
    let registries = registries();
    let mut first = AppState::new(WorldSeed::new(0x8120_0007));
    initialize_service_player(&registries, &mut first);
    let equipment =
        match add_equipment(&registries, &mut first, TEST_DEFINITION, condition(700_000)) {
            Ok(equipment) => equipment,
            Err(error) => panic!("maintenance soak equipment fixture failed: {error}"),
        };
    let source = match add_solid_stockpile_for_test(&mut first, Mass::from_milligrams(500)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("maintenance soak source fixture failed: {error}"),
    };
    let spent = match add_solid_stockpile_for_test(&mut first, Mass::from_milligrams(500)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("maintenance soak spent fixture failed: {error}"),
    };
    add_material(&registries, &mut first, source, Mass::from_milligrams(500));
    let initial_matter = match calculate_matter_accounting(&first) {
        Ok(accounting) => accounting.total(),
        Err(error) => panic!("maintenance soak initial matter accounting failed: {error}"),
    };
    let initial_energy = explicit_energy(&registries, &first);
    let mut second = first.clone();

    for cycle in 0..500_u64 {
        for state in [&mut first, &mut second] {
            degrade_equipment_condition_for_test(state, equipment, 1_000);
            assert_eq!(
                state
                    .equipment()
                    .get_equipment(equipment)
                    .map(|record| record.condition()),
                Some(condition(699_000))
            );
            let resolution = resolve_equipment_maintenance(
                &registries,
                state,
                EquipmentMaintenanceRequest::new(equipment, source, spent),
            )
            .unwrap_or_else(|error| {
                panic!("maintenance soak resolution failed at {cycle}: {error}")
            });
            assert_eq!(resolution.material_mass(), Mass::from_milligrams(1));
            assert_eq!(resolution.condition_before(), condition(699_000));
            assert_eq!(resolution.condition_after(), condition(700_000));
            assert_eq!(resolution.duration(), TickSpan::new(1));
            let maintenance = match validate_equipment_maintenance(&registries, state, resolution) {
                Ok(token) => token,
                Err(error) => panic!("maintenance soak validation failed at {cycle}: {error}"),
            };
            let outcome = maintenance.commit(state).unwrap_or_else(|error| {
                panic!("maintenance soak commit failed at {cycle}: {error}")
            });
            assert_eq!(outcome.target_condition(), condition(700_000));
            let completion = finish_service(&registries, state, outcome.completes_at());
            assert_eq!(completion.condition_before(), condition(699_000));
            assert_eq!(completion.condition_after(), condition(700_000));
        }
        if cycle % 53 == 0 {
            assert_eq!(validate_loaded_state(&registries, &first), Ok(()));
            assert_eq!(
                calculate_matter_accounting(&first).map(|accounting| accounting.total()),
                Ok(initial_matter)
            );
            assert_eq!(explicit_energy(&registries, &first), initial_energy);
        }
    }

    assert_eq!(first, second);
    assert_eq!(validate_loaded_state(&registries, &first), Ok(()));
    assert_eq!(
        calculate_matter_accounting(&first).map(|accounting| accounting.total()),
        Ok(initial_matter)
    );
    assert_eq!(explicit_energy(&registries, &first), initial_energy);
    assert_eq!(
        first
            .equipment()
            .get_equipment(equipment)
            .map(|record| record.condition()),
        Some(condition(700_000))
    );
    assert_eq!(
        first
            .inventory()
            .get_stockpile(source)
            .map(|record| record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_LOG))),
        Some(Mass::ZERO)
    );
    assert_eq!(
        first
            .inventory()
            .get_stockpile(spent)
            .map(|record| record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_CHIP))),
        Some(Mass::from_milligrams(500))
    );
}

#[test]
fn maintenance_commit_rechecks_late_production_occupancy_before_moving_material() {
    let registries = occupied_registries();
    let mut state = AppState::new(WorldSeed::new(0x8120_0008));
    initialize_service_player(&registries, &mut state);
    let equipment =
        match add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000)) {
            Ok(equipment) => equipment,
            Err(error) => panic!("maintenance occupancy equipment fixture failed: {error}"),
        };
    let process_source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("maintenance occupancy process source failed: {error}"),
    };
    let process_destination =
        match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
            Ok(stockpile) => stockpile,
            Err(error) => panic!("maintenance occupancy process destination failed: {error}"),
        };
    let maintenance_source =
        match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2)) {
            Ok(stockpile) => stockpile,
            Err(error) => panic!("maintenance occupancy maintenance source failed: {error}"),
        };
    let spent = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("maintenance occupancy spent destination failed: {error}"),
    };
    let process_lot = add_material(
        &registries,
        &mut state,
        process_source,
        Mass::from_milligrams(10),
    );
    let maintenance_lot = add_material(
        &registries,
        &mut state,
        maintenance_source,
        Mass::from_milligrams(2),
    );
    let energy_store = match add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        ENERGY_DEFINITION,
        Energy::from_nanojoules(1_000_000_000),
    ) {
        Ok(store) => store,
        Err(error) => panic!("maintenance occupancy energy fixture failed: {error}"),
    };
    let maintenance_resolution = resolve_equipment_maintenance(
        &registries,
        &state,
        EquipmentMaintenanceRequest::new(equipment, maintenance_source, spent),
    )
    .unwrap_or_else(|error| panic!("maintenance occupancy resolution failed: {error}"));
    let maintenance =
        match validate_equipment_maintenance(&registries, &state, maintenance_resolution) {
            Ok(token) => token,
            Err(error) => panic!("maintenance occupancy validation failed: {error}"),
        };

    let selection = [MaterialLotSelection::new(
        process_lot,
        Mass::from_milligrams(10),
    )];
    let heating = match resolve_sensible_heating_process(
        &registries,
        &state,
        SensibleHeatingRequest::new(
            HEATING_PROCESS,
            process_source,
            &selection,
            equipment,
            energy_store,
            Temperature::from_millikelvin(301_000),
        ),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("maintenance occupancy heating resolution failed: {error}"),
    };
    let start = match validate_start_process(
        &registries,
        &state,
        heating.process_resolution(),
        process_source,
        process_destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("maintenance occupancy process validation failed: {error}"),
    };
    let job = match start.commit(&mut state) {
        Ok(job) => job,
        Err(error) => panic!("maintenance occupancy process commit failed: {error}"),
    };
    let job_record = match state.production().get_job(job) {
        Some(record) => record,
        None => panic!("maintenance occupancy process job missing after start"),
    };
    let expected_error = EquipmentMaintenanceCommitError::EquipmentBusy {
        equipment,
        job,
        release: job_record.occupancy_release(),
    };
    let maintenance_mass_before = state
        .inventory()
        .get_lot(maintenance_lot)
        .map(|lot| lot.mass());
    let condition_before = state
        .equipment()
        .get_equipment(equipment)
        .map(|record| record.condition());

    assert_eq!(maintenance.commit(&mut state), Err(expected_error));
    assert_eq!(
        state
            .inventory()
            .get_lot(maintenance_lot)
            .map(|lot| lot.mass()),
        maintenance_mass_before
    );
    assert_eq!(
        state
            .equipment()
            .get_equipment(equipment)
            .map(|record| record.condition()),
        condition_before
    );
}

#[test]
fn production_commit_reports_late_maintenance_occupancy_before_stale_revision() {
    let registries = occupied_registries();
    let mut state = AppState::new(WorldSeed::new(0x8120_000A));
    initialize_service_player(&registries, &mut state);
    let equipment =
        match add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000)) {
            Ok(equipment) => equipment,
            Err(error) => panic!("maintenance race equipment fixture failed: {error}"),
        };
    let process_source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("maintenance race process source failed: {error}"),
    };
    let process_destination =
        match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
            Ok(stockpile) => stockpile,
            Err(error) => panic!("maintenance race process destination failed: {error}"),
        };
    let maintenance_source =
        match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2)) {
            Ok(stockpile) => stockpile,
            Err(error) => panic!("maintenance race maintenance source failed: {error}"),
        };
    let spent = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("maintenance race spent destination failed: {error}"),
    };
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
    let energy_store = match add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        ENERGY_DEFINITION,
        Energy::from_nanojoules(1_000_000_000),
    ) {
        Ok(store) => store,
        Err(error) => panic!("maintenance race energy fixture failed: {error}"),
    };
    let selection = [MaterialLotSelection::new(
        process_lot,
        Mass::from_milligrams(10),
    )];
    let heating = match resolve_sensible_heating_process(
        &registries,
        &state,
        SensibleHeatingRequest::new(
            HEATING_PROCESS,
            process_source,
            &selection,
            equipment,
            energy_store,
            Temperature::from_millikelvin(301_000),
        ),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("maintenance race heating resolution failed: {error}"),
    };
    let process = match validate_start_process(
        &registries,
        &state,
        heating.process_resolution(),
        process_source,
        process_destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("maintenance race process validation failed: {error}"),
    };
    let maintenance_resolution = resolve_equipment_maintenance(
        &registries,
        &state,
        EquipmentMaintenanceRequest::new(equipment, maintenance_source, spent),
    )
    .unwrap_or_else(|error| panic!("maintenance race resolution failed: {error}"));
    let maintenance =
        match validate_equipment_maintenance(&registries, &state, maintenance_resolution) {
            Ok(token) => token,
            Err(error) => panic!("maintenance race validation failed: {error}"),
        };
    let service = match maintenance.commit(&mut state) {
        Ok(outcome) => outcome,
        Err(error) => panic!("maintenance race commit failed: {error}"),
    };
    let process_input_before = state.inventory().get_lot(process_lot).map(|lot| lot.mass());
    let production_jobs_before = state.production().jobs().count();

    assert_eq!(
        process.commit(&mut state),
        Err(StartProcessCommitError::EquipmentUnderMaintenance {
            equipment,
            completes_at: service.completes_at(),
        })
    );
    assert_eq!(
        state.inventory().get_lot(process_lot).map(|lot| lot.mass()),
        process_input_before,
        "late maintenance conflict must reject before moving process input"
    );
    assert_eq!(
        state.production().jobs().count(),
        production_jobs_before,
        "late maintenance conflict must not insert a production job"
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("maintenance race state audit failed: {error}"));
}

#[test]
fn maintenance_counts_reserved_inbound_as_capacity_but_not_structural_weight() {
    let registries = occupied_registries();
    let mut state = AppState::new(WorldSeed::new(0x8120_0009));
    initialize_service_player(&registries, &mut state);
    let process_equipment = match add_equipment(
        &registries,
        &mut state,
        TEST_DEFINITION,
        Condition::PRISTINE,
    ) {
        Ok(equipment) => equipment,
        Err(error) => panic!("reserved-weight process equipment fixture failed: {error}"),
    };
    let maintenance_equipment =
        match add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000)) {
            Ok(equipment) => equipment,
            Err(error) => panic!("reserved-weight maintenance equipment fixture failed: {error}"),
        };
    let process_source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(5)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("reserved-weight process source fixture failed: {error}"),
    };
    let maintenance_source =
        match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2)) {
            Ok(stockpile) => stockpile,
            Err(error) => panic!("reserved-weight maintenance source fixture failed: {error}"),
        };
    let spent = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("reserved-weight spent fixture failed: {error}"),
    };
    let process_lot = add_material(
        &registries,
        &mut state,
        process_source,
        Mass::from_milligrams(5),
    );
    add_material(
        &registries,
        &mut state,
        maintenance_source,
        Mass::from_milligrams(2),
    );
    let support = active_support(&registries, &mut state, 0);
    let mount = match validate_mount_stockpile(&registries, &state, spent, support) {
        Ok(token) => token,
        Err(error) => panic!("reserved-weight spent mount validation failed: {error}"),
    };
    if let Err(error) = mount.commit(&mut state) {
        panic!("reserved-weight spent mount commit failed: {error}");
    }
    let energy_store = match add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        ENERGY_DEFINITION,
        Energy::from_nanojoules(1_000_000_000),
    ) {
        Ok(store) => store,
        Err(error) => panic!("reserved-weight energy fixture failed: {error}"),
    };
    let process_selection = [MaterialLotSelection::new(
        process_lot,
        Mass::from_milligrams(5),
    )];
    let heating = match resolve_sensible_heating_process(
        &registries,
        &state,
        SensibleHeatingRequest::new(
            HEATING_PROCESS,
            process_source,
            &process_selection,
            process_equipment,
            energy_store,
            Temperature::from_millikelvin(301_000),
        ),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("reserved-weight heating resolution failed: {error}"),
    };
    let start = match validate_start_process(
        &registries,
        &state,
        heating.process_resolution(),
        process_source,
        spent,
    ) {
        Ok(token) => token,
        Err(error) => panic!("reserved-weight process validation failed: {error}"),
    };
    if let Err(error) = start.commit(&mut state) {
        panic!("reserved-weight process commit failed: {error}");
    }

    let spent_before = match state.inventory().get_stockpile(spent) {
        Some(record) => record,
        None => panic!("reserved-weight spent stockpile disappeared"),
    };
    assert_eq!(spent_before.reserved_inbound(), Mass::from_milligrams(5));
    assert_eq!(spent_before.stored_mass(), Mass::ZERO);
    assert_eq!(
        state
            .structures()
            .get_element(support)
            .map(|record| record.load(StructuralLoadKind::StoredMatter)),
        Some(Force::ZERO)
    );

    let maintenance_resolution = resolve_equipment_maintenance(
        &registries,
        &state,
        EquipmentMaintenanceRequest::new(maintenance_equipment, maintenance_source, spent),
    )
    .unwrap_or_else(|error| panic!("reserved-weight maintenance resolution failed: {error}"));
    let maintenance =
        match validate_equipment_maintenance(&registries, &state, maintenance_resolution) {
            Ok(token) => token,
            Err(error) => panic!("reserved-weight maintenance validation failed: {error}"),
        };
    if let Err(error) = maintenance.commit(&mut state) {
        panic!("reserved-weight maintenance commit failed: {error}");
    }

    let spent_after = match state.inventory().get_stockpile(spent) {
        Some(record) => record,
        None => panic!("reserved-weight spent stockpile disappeared after maintenance"),
    };
    assert_eq!(spent_after.reserved_inbound(), Mass::from_milligrams(5));
    assert_eq!(spent_after.stored_mass(), Mass::from_milligrams(2));
    let expected_weight = match calculate_aggregate_weight_force_ceiling(
        AggregateMass::from_mass(Mass::from_milligrams(2)),
        registries.core().gravity(),
    ) {
        Some(force) => force,
        None => panic!("reserved-weight expected load overflowed"),
    };
    assert_eq!(
        state
            .structures()
            .get_element(support)
            .map(|record| record.load(StructuralLoadKind::StoredMatter)),
        Some(expected_weight)
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
