//! Suspension, support races, occupancy, and stale-commit contracts.

use super::*;

#[test]
fn supported_heating_suspends_on_collapse_and_resumes_after_relocation() {
    let registries = make_registries_with_energy_output_power(
        EnergyCarrier::Electrical,
        Temperature::from_millikelvin(400_000),
        Power::from_microwatts(5_000),
    );
    let (registries, mut state, source, destination, equipment, energy_store) =
        make_loaded_fixture_with_registries(
            registries,
            Condition::PRISTINE,
            Temperature::from_millikelvin(300_000),
            Energy::from_nanojoules(500_000_000),
        );
    let failed_support = add_active_support(&registries, &mut state, 0);
    let recovery_support = add_active_support(&registries, &mut state, 2);
    let _ = validate_mount_equipment(&registries, &state, equipment, failed_support)
        .unwrap_or_else(|error| panic!("suspension fixture mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("suspension fixture mount commit failed: {error}"));
    let _ = validate_mount_stockpile(&registries, &state, destination, failed_support)
        .unwrap_or_else(|error| panic!("suspension destination mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("suspension destination mount commit failed: {error}"));

    let resolved = resolve_test_sensible_heating_process(
        &registries,
        &state,
        PROCESS,
        source,
        equipment,
        energy_store,
        Temperature::from_millikelvin(303_000),
    )
    .unwrap_or_else(|error| panic!("suspension fixture resolution failed: {error}"));
    let active_duration = resolved.process_resolution().duration();
    assert!(active_duration.value() > 2);
    let start = validate_start_process(
        &registries,
        &state,
        resolved.process_resolution(),
        source,
        destination,
    )
    .unwrap_or_else(|error| panic!("suspension fixture start failed: {error}"));
    let job = start
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("suspension fixture start commit failed: {error}"));
    let original_due = state
        .production()
        .get_job(job)
        .map(|record| record.completes_at())
        .unwrap_or_else(|| panic!("suspension fixture job disappeared"));
    let reserved_output_mass = state
        .inventory()
        .get_stockpile(destination)
        .map(|stockpile| stockpile.reserved_inbound())
        .unwrap_or_else(|| panic!("suspension fixture destination disappeared"));
    assert!(!reserved_output_mass.is_zero());

    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("suspension fixture first active tick failed: {error}"));
    let suspended_at = state.tick();
    fail_support(&registries, &mut state, failed_support);
    let expected_remaining = original_due
        .checked_duration_since(suspended_at)
        .unwrap_or_else(|| panic!("suspension fixture due tick precedes suspension"));
    let outcome = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("suspension transition tick failed: {error}"));
    assert_eq!(
        outcome.production_availability_changes(),
        &[ProductionAvailabilityChange::Suspended {
            job,
            reason: ProductionSuspensionReason::EquipmentSupportUnavailable { equipment },
            suspended_at,
            remaining_active_time: expected_remaining,
        }]
    );
    let suspension = state
        .production()
        .get_job(job)
        .and_then(|record| record.suspension())
        .unwrap_or_else(|| panic!("collapsed supported job did not suspend"));
    assert_eq!(suspension.remaining_active_time(), expected_remaining);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| stockpile.reserved_inbound()),
        Some(reserved_output_mass),
        "suspension must retain the job's output capacity reservation"
    );
    assert_eq!(
        state
            .equipment()
            .get_equipment(equipment)
            .map(|record| record.condition()),
        Some(Condition::PRISTINE)
    );
    assert_eq!(
        validate_energy_supply(
            &registries,
            &state,
            energy_store,
            Energy::from_nanojoules(1),
        ),
        Err(EnergySupplyError::StoreBusy {
            store: energy_store,
            job,
            release: ProductionOccupancyRelease::AwaitingResume,
        })
    );
    let _source_mount = validate_mount_stockpile(&registries, &state, source, recovery_support)
        .unwrap_or_else(|error| {
            panic!("released production source remained spuriously relocation-locked: {error}")
        });
    let _destination_unmount = validate_unmount_stockpile(&registries, &state, destination)
        .unwrap_or_else(|error| {
            panic!(
                "suspended production destination remained spuriously relocation-locked: {error}"
            )
        });
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("suspended heating save failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("suspended heating save decode failed: {error}"));
    let loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("suspended heating save validation failed: {error}"));
    assert_eq!(loaded, state);
    assert_eq!(
        loaded
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| stockpile.reserved_inbound()),
        Some(reserved_output_mass),
        "save/reload must preserve suspended output reservation ownership"
    );

    let mut tampered =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("powered player-labor tamper serialization failed: {error}")
        });
    tampered["state"]["systems"]["production"]["jobs"][job.value().to_string()]["schedule"]["suspension"]
        ["reason"] = serde_json::json!("PlayerLaborUnavailable");
    let tampered: LoadedSaveEnvelope = serde_json::from_value(tampered)
        .unwrap_or_else(|error| panic!("powered player-labor tamper decode failed: {error}"));
    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::NonManualJobSuspendedForPlayerLabor {
                job,
                process: PROCESS,
            }
        ))
    );

    let tampered_due = SimulationTick::new(original_due.value() + 1);
    let mut tampered = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("suspended schedule tamper serialization failed: {error}"));
    tampered["state"]["systems"]["production"]["jobs"][job.value().to_string()]["schedule"]["completes_at"] =
        serde_json::json!(tampered_due.value());
    let tampered: LoadedSaveEnvelope = serde_json::from_value(tampered)
        .unwrap_or_else(|error| panic!("suspended schedule tamper decode failed: {error}"));
    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Production(
            ProductionValidationError::SuspensionScheduleMismatch {
                job,
                expected_due: original_due,
                actual_due: tampered_due,
            }
        )))
    );

    let mut tampered =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("suspended remaining-time tamper serialization failed: {error}")
        });
    let excessive_remaining = TickSpan::new(active_duration.value() + 1);
    tampered["state"]["systems"]["production"]["jobs"][job.value().to_string()]["schedule"]["suspension"]
        ["remaining_active_time"] = serde_json::json!(excessive_remaining.value());
    let tampered: LoadedSaveEnvelope = serde_json::from_value(tampered)
        .unwrap_or_else(|error| panic!("suspended remaining-time tamper decode failed: {error}"));
    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Production(
            ProductionValidationError::SuspensionRemainingExceedsActiveDuration {
                job,
                remaining: excessive_remaining,
                active_duration,
            }
        )))
    );

    let future_suspended_at = SimulationTick::new(state.tick().value() + 1);
    let future_due = future_suspended_at
        .checked_add_span(expected_remaining)
        .unwrap_or_else(|| panic!("future-suspension tamper due tick overflowed"));
    let mut tampered = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("future-suspension tamper serialization failed: {error}"));
    tampered["state"]["systems"]["production"]["jobs"][job.value().to_string()]["schedule"]["suspension"]
        ["suspended_at"] = serde_json::json!(future_suspended_at.value());
    tampered["state"]["systems"]["production"]["jobs"][job.value().to_string()]["schedule"]["completes_at"] =
        serde_json::json!(future_due.value());
    let tampered: LoadedSaveEnvelope = serde_json::from_value(tampered)
        .unwrap_or_else(|error| panic!("future-suspension tamper decode failed: {error}"));
    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Production(
            ProductionValidationError::SuspensionInFuture {
                job,
                current: state.tick(),
                suspended_at: future_suspended_at,
            }
        )))
    );
    state = loaded;

    let _ = validate_unmount_equipment(&registries, &state, equipment)
        .unwrap_or_else(|error| panic!("suspended equipment unmount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("suspended equipment unmount commit failed: {error}"));
    let _ = validate_mount_equipment(&registries, &state, equipment, recovery_support)
        .unwrap_or_else(|error| panic!("suspended equipment remount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("suspended equipment remount commit failed: {error}"));

    let reason_change = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("suspension reason transition tick failed: {error}"));
    assert_eq!(
        reason_change.production_availability_changes(),
        &[ProductionAvailabilityChange::SuspensionReasonChanged {
            job,
            previous: ProductionSuspensionReason::EquipmentSupportUnavailable { equipment },
            reason: ProductionSuspensionReason::OutputSupportUnavailable {
                stockpile: destination,
            },
        }]
    );
    assert_eq!(
        state
            .production()
            .get_job(job)
            .and_then(|record| record.suspension())
            .map(|suspension| (suspension.remaining_active_time(), suspension.reason())),
        Some((
            expected_remaining,
            ProductionSuspensionReason::OutputSupportUnavailable {
                stockpile: destination,
            }
        ))
    );

    let _ = validate_unmount_stockpile(&registries, &state, destination)
        .unwrap_or_else(|error| panic!("suspended destination unmount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("suspended destination unmount commit failed: {error}"));
    let _ = validate_mount_stockpile(&registries, &state, destination, recovery_support)
        .unwrap_or_else(|error| panic!("suspended destination remount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("suspended destination remount commit failed: {error}"));

    let resumed_at = state.tick();
    let resumed_due = resumed_at
        .checked_add_span(expected_remaining)
        .unwrap_or_else(|| panic!("suspension fixture resumed due tick overflowed"));
    let outcome = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("resume transition tick failed: {error}"));
    assert_eq!(
        outcome.production_availability_changes(),
        &[ProductionAvailabilityChange::Resumed {
            job,
            reason: ProductionSuspensionReason::OutputSupportUnavailable {
                stockpile: destination,
            },
            resumed_at,
            scheduled_completion: resumed_due,
        }]
    );
    assert_eq!(
        state.production().get_job(job).map(|record| (
            record.active_duration(),
            record.completes_at(),
            record.suspension()
        )),
        Some((active_duration, resumed_due, None))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| stockpile.reserved_inbound()),
        Some(reserved_output_mass),
        "resume must not release reserved output capacity before completion"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let forged_due = resumed_due
        .checked_add_span(TickSpan::new(1))
        .unwrap_or_else(|| panic!("resumed schedule tamper due tick overflowed"));
    let mut tampered = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("resumed schedule tamper serialization failed: {error}"));
    tampered["state"]["systems"]["production"]["jobs"][job.value().to_string()]["schedule"]["completes_at"] =
        serde_json::json!(forged_due.value());
    let tampered: LoadedSaveEnvelope = serde_json::from_value(tampered)
        .unwrap_or_else(|error| panic!("resumed schedule tamper decode failed: {error}"));
    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Production(
            ProductionValidationError::CompletionScheduleMismatch {
                job,
                expected_due: resumed_due,
                actual_due: forged_due,
            }
        )))
    );

    let started_at = state
        .production()
        .get_job(job)
        .map(|record| record.started_at())
        .unwrap_or_else(|| panic!("resumed schedule job disappeared before history tamper"));
    let elapsed = state
        .tick()
        .checked_duration_since(started_at)
        .unwrap_or_else(|| panic!("resumed schedule started after current tick"));
    let forged_completed = TickSpan::new(elapsed.value() + 1);
    let mut tampered = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("suspension history tamper serialization failed: {error}"));
    tampered["state"]["systems"]["production"]["jobs"][job.value().to_string()]["schedule"]["completed_suspension_time"] =
        serde_json::json!(forged_completed.value());
    let tampered: LoadedSaveEnvelope = serde_json::from_value(tampered)
        .unwrap_or_else(|error| panic!("suspension history tamper decode failed: {error}"));
    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Production(
            ProductionValidationError::CompletedSuspensionTimeExceedsElapsed {
                job,
                completed: forged_completed,
                elapsed,
            }
        )))
    );

    while state.production().get_job(job).is_some() {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("resumed heating completion failed: {error}"));
    }
    assert_eq!(state.tick(), resumed_due);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::from_milligrams(10))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| stockpile.reserved_inbound()),
        Some(Mass::ZERO),
        "completion must release exactly the reservation it materializes"
    );
    assert_eq!(
        state
            .equipment()
            .get_equipment(equipment)
            .map(|record| record.condition()),
        Some(condition(997_000))
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn resolved_heating_becomes_stale_when_support_changes_before_start_validation() {
    let (registries, mut state, source, destination, equipment, energy_store) =
        make_loaded_fixture(EnergyCarrier::Electrical);
    let support = add_active_support(&registries, &mut state, 0);
    let mount = match validate_mount_equipment(&registries, &state, equipment, support) {
        Ok(token) => token,
        Err(error) => panic!("stale-support mount validation failed: {error}"),
    };
    if let Err(error) = mount.commit(&mut state) {
        panic!("stale-support mount commit failed: {error}");
    }
    let resolved = match resolve_test_sensible_heating_process(
        &registries,
        &state,
        PROCESS,
        source,
        equipment,
        energy_store,
        Temperature::from_millikelvin(303_000),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("stale-support heating resolution failed: {error}"),
    };
    let expected_structure_revision = state.structures().revision();
    fail_support(&registries, &mut state, support);

    assert_eq!(
        validate_start_process(
            &registries,
            &state,
            resolved.process_resolution(),
            source,
            destination,
        ),
        Err(StartProcessError::StaleResolvedStructure {
            expected_structure_revision,
            actual_structure_revision: expected_structure_revision + 1,
        })
    );
}

#[test]
fn validated_heating_start_rejects_support_change_before_commit_without_consuming_resources() {
    let (registries, mut state, source, destination, equipment, energy_store) =
        make_loaded_fixture(EnergyCarrier::Electrical);
    let support = add_active_support(&registries, &mut state, 0);
    let mount = match validate_mount_equipment(&registries, &state, equipment, support) {
        Ok(token) => token,
        Err(error) => panic!("commit-race mount validation failed: {error}"),
    };
    if let Err(error) = mount.commit(&mut state) {
        panic!("commit-race mount commit failed: {error}");
    }
    let resolved = match resolve_test_sensible_heating_process(
        &registries,
        &state,
        PROCESS,
        source,
        equipment,
        energy_store,
        Temperature::from_millikelvin(303_000),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("commit-race heating resolution failed: {error}"),
    };
    let start = match validate_start_process(
        &registries,
        &state,
        resolved.process_resolution(),
        source,
        destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("commit-race start validation failed: {error}"),
    };
    let expected_structure_revision = state.structures().revision();
    fail_support(&registries, &mut state, support);
    let before = state.clone();

    assert_eq!(
        start.commit(&mut state),
        Err(StartProcessCommitError::StaleStructureRevision {
            expected: expected_structure_revision,
            actual: expected_structure_revision + 1,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn prevalidated_mount_is_blocked_if_job_starts_first() {
    let (registries, mut state, source, destination, equipment, energy_store) =
        make_loaded_fixture(EnergyCarrier::Electrical);
    let support = add_active_support(&registries, &mut state, 0);
    let mount = match validate_mount_equipment(&registries, &state, equipment, support) {
        Ok(token) => token,
        Err(error) => panic!("occupancy-race mount validation failed: {error}"),
    };
    let resolved = match resolve_test_sensible_heating_process(
        &registries,
        &state,
        PROCESS,
        source,
        equipment,
        energy_store,
        Temperature::from_millikelvin(303_000),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("occupancy-race heating resolution failed: {error}"),
    };
    let start = match validate_start_process(
        &registries,
        &state,
        resolved.process_resolution(),
        source,
        destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("occupancy-race start validation failed: {error}"),
    };
    let job = match start.commit(&mut state) {
        Ok(job) => job,
        Err(error) => panic!("occupancy-race start commit failed: {error}"),
    };
    let completes_at = match state.production().get_job(job) {
        Some(record) => record.completes_at(),
        None => panic!("occupancy-race job disappeared"),
    };

    let before_mount = state.clone();
    assert_eq!(
        mount.commit(&mut state),
        Err(EquipmentSupportCommitError::EquipmentBusy {
            equipment,
            job,
            completes_at,
        })
    );
    assert_eq!(state, before_mount);
}

#[test]
fn heater_is_exclusive_while_job_runs_and_releases_on_completion() {
    let (registries, mut state, source, destination, equipment, energy_store) =
        make_loaded_fixture(EnergyCarrier::Electrical);
    if let Err(error) = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(300_000),
    ) {
        panic!("second heater occupancy input failed: {error}");
    }
    let second_energy_store = match add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        BATTERY,
        Energy::from_nanojoules(500_000_000),
    ) {
        Ok(store) => store,
        Err(error) => panic!("second heater occupancy energy fixture failed: {error}"),
    };
    let target = Temperature::from_millikelvin(303_000);
    let first = match resolve_test_sensible_heating_process(
        &registries,
        &state,
        PROCESS,
        source,
        equipment,
        energy_store,
        target,
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("first heater occupancy resolution failed: {error}"),
    };
    let duration = first.process_resolution().duration();
    let first_token = match validate_start_process(
        &registries,
        &state,
        first.process_resolution(),
        source,
        destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("first heater occupancy validation failed: {error}"),
    };
    let first_job = match first_token.commit(&mut state) {
        Ok(job) => job,
        Err(error) => panic!("first heater occupancy commit failed: {error}"),
    };
    let completes_at = match state.production().get_job(first_job) {
        Some(job) => job.completes_at(),
        None => panic!("first heater occupancy job disappeared"),
    };
    assert_eq!(
        state
            .production()
            .get_job(first_job)
            .and_then(|job| job.equipment_provider()),
        first.process_resolution().equipment_input()
    );

    let second = match resolve_test_sensible_heating_process(
        &registries,
        &state,
        PROCESS,
        source,
        equipment,
        second_energy_store,
        target,
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("second heater occupancy resolution failed: {error}"),
    };
    assert_eq!(
        validate_start_process(
            &registries,
            &state,
            second.process_resolution(),
            source,
            destination,
        ),
        Err(crate::production::StartProcessError::EquipmentBusy {
            equipment,
            job: first_job,
            release: ProductionOccupancyRelease::Scheduled(completes_at),
        })
    );
    for _ in 0..duration.value() {
        if let Err(error) = advance_tick(&registries, &mut state) {
            panic!("heater occupancy completion failed: {error}");
        }
    }
    assert!(state.production().get_job(first_job).is_none());

    let after_release = match resolve_test_sensible_heating_process(
        &registries,
        &state,
        PROCESS,
        source,
        equipment,
        energy_store,
        target,
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("post-release heater resolution failed: {error}"),
    };
    let token = match validate_start_process(
        &registries,
        &state,
        after_release.process_resolution(),
        source,
        destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("post-release heater start failed: {error}"),
    };
    if let Err(error) = token.commit(&mut state) {
        panic!("post-release heater commit failed: {error}");
    }
}

#[test]
fn finite_energy_store_is_exclusive_while_its_discharge_power_is_reserved() {
    let (registries, mut state, source, destination, first_heater, energy_store) =
        make_loaded_fixture(EnergyCarrier::Electrical);
    if let Err(error) = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(300_000),
    ) {
        panic!("energy occupancy second input failed: {error}");
    }
    let second_heater = match add_equipment(&registries, &mut state, HEATER, Condition::PRISTINE) {
        Ok(equipment) => equipment,
        Err(error) => panic!("energy occupancy second heater failed: {error}"),
    };
    let target = Temperature::from_millikelvin(303_000);
    let first = match resolve_test_sensible_heating_process(
        &registries,
        &state,
        PROCESS,
        source,
        first_heater,
        energy_store,
        target,
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("energy occupancy first resolution failed: {error}"),
    };
    let duration = first.process_resolution().duration();
    let token = match validate_start_process(
        &registries,
        &state,
        first.process_resolution(),
        source,
        destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("energy occupancy first validation failed: {error}"),
    };
    let first_job = match token.commit(&mut state) {
        Ok(job) => job,
        Err(error) => panic!("energy occupancy first commit failed: {error}"),
    };
    let completes_at = match state.production().get_job(first_job) {
        Some(job) => job.completes_at(),
        None => panic!("energy occupancy first job disappeared"),
    };

    assert_eq!(
        resolve_test_sensible_heating_process(
            &registries,
            &state,
            PROCESS,
            source,
            second_heater,
            energy_store,
            target,
        ),
        Err(SensibleHeatingResolutionError::Energy(
            EnergySupplyError::StoreBusy {
                store: energy_store,
                job: first_job,
                release: ProductionOccupancyRelease::Scheduled(completes_at),
            }
        ))
    );

    for _ in 0..duration.value() {
        if let Err(error) = advance_tick(&registries, &mut state) {
            panic!("energy occupancy completion failed: {error}");
        }
    }
    assert!(state.production().get_job(first_job).is_none());
    assert!(
        resolve_test_sensible_heating_process(
            &registries,
            &state,
            PROCESS,
            source,
            second_heater,
            energy_store,
            target,
        )
        .is_ok()
    );
}

#[test]
fn due_heating_completion_rejects_stale_equipment_revision_atomically() {
    let (registries, mut state, source, destination, equipment, energy_store) =
        make_loaded_fixture(EnergyCarrier::Electrical);
    let resolved = match resolve_test_sensible_heating_process(
        &registries,
        &state,
        PROCESS,
        source,
        equipment,
        energy_store,
        Temperature::from_millikelvin(303_000),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("completion-race heating resolution failed: {error}"),
    };
    let duration = resolved.process_resolution().duration();
    let token = match validate_start_process(
        &registries,
        &state,
        resolved.process_resolution(),
        source,
        destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("completion-race start validation failed: {error}"),
    };
    let job = match token.commit(&mut state) {
        Ok(job) => job,
        Err(error) => panic!("completion-race start commit failed: {error}"),
    };
    for _ in 1..duration.value() {
        if let Err(error) = advance_tick(&registries, &mut state) {
            panic!("completion-race pre-due tick failed: {error}");
        }
    }
    assert_eq!(
        state.tick().checked_duration_since(SimulationTick::ZERO),
        duration.checked_sub(TickSpan::new(1))
    );
    let due = match state.production().get_job(job) {
        Some(record) => record.completes_at(),
        None => panic!("completion-race job disappeared before due planning"),
    };
    let plan = match decide_due_completions(&registries, &state, due) {
        Ok(plan) => plan,
        Err(error) => panic!("completion-race due planning failed: {error:?}"),
    };
    let expected = state.equipment().revision();
    if let Err(error) = add_equipment(&registries, &mut state, HEATER, Condition::PRISTINE) {
        panic!("completion-race independent equipment mutation failed: {error}");
    }
    let before = state.clone();

    assert_eq!(
        apply_completion_plan(&mut state, plan),
        Err(CompletionCommitError::EquipmentRevisionConflict {
            expected,
            actual: expected + 1,
        })
    );
    assert_eq!(state, before);
    assert!(state.production().get_job(job).is_some());
}

#[test]
fn validated_heating_start_rejects_stale_equipment_before_consuming_other_resources() {
    let (registries, mut state, source, destination, equipment, energy_store) =
        make_loaded_fixture(EnergyCarrier::Electrical);
    let resolved = match resolve_test_sensible_heating_process(
        &registries,
        &state,
        PROCESS,
        source,
        equipment,
        energy_store,
        Temperature::from_millikelvin(303_000),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("stale equipment fixture resolution failed: {error}"),
    };
    let token = match validate_start_process(
        &registries,
        &state,
        resolved.process_resolution(),
        source,
        destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("stale equipment fixture validation failed: {error}"),
    };
    let expected = state.equipment().revision();
    if let Err(error) = add_equipment(&registries, &mut state, HEATER, Condition::PRISTINE) {
        panic!("independent equipment mutation failed: {error}");
    }
    let before = state.clone();

    assert_eq!(
        token.commit(&mut state),
        Err(
            crate::production::StartProcessCommitError::StaleEquipmentRevision {
                expected,
                actual: expected + 1,
            }
        )
    );
    assert_eq!(state, before);
    assert_eq!(state.production().jobs().count(), 0);
}
