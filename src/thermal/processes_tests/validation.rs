//! Concurrent completion, support, and trusted-load validation contracts.

use super::*;

#[test]
fn same_tick_heating_completions_apply_all_wear_under_one_equipment_revision() {
    let (registries, mut state, source, destination, first_equipment, first_energy) =
        make_loaded_fixture(EnergyCarrier::Electrical);
    if let Err(error) = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(300_000),
    ) {
        panic!("same-tick wear second input fixture failed: {error}");
    }
    let second_equipment = match add_equipment(&registries, &mut state, HEATER, Condition::PRISTINE)
    {
        Ok(equipment) => equipment,
        Err(error) => panic!("same-tick wear second equipment fixture failed: {error}"),
    };
    let second_energy = match add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        BATTERY,
        Energy::from_nanojoules(500_000_000),
    ) {
        Ok(store) => store,
        Err(error) => panic!("same-tick wear second energy fixture failed: {error}"),
    };
    let target = Temperature::from_millikelvin(303_000);

    let first = match resolve_test_sensible_heating_process(
        &registries,
        &state,
        PROCESS,
        source,
        first_equipment,
        first_energy,
        target,
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("same-tick wear first resolution failed: {error}"),
    };
    let duration = first.process_resolution().duration();
    let first_start = match validate_start_process(
        &registries,
        &state,
        first.process_resolution(),
        source,
        destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("same-tick wear first start validation failed: {error}"),
    };
    if let Err(error) = first_start.commit(&mut state) {
        panic!("same-tick wear first start commit failed: {error}");
    }

    let second = match resolve_test_sensible_heating_process(
        &registries,
        &state,
        PROCESS,
        source,
        second_equipment,
        second_energy,
        target,
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("same-tick wear second resolution failed: {error}"),
    };
    assert_eq!(second.process_resolution().duration(), duration);
    let second_start = match validate_start_process(
        &registries,
        &state,
        second.process_resolution(),
        source,
        destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("same-tick wear second start validation failed: {error}"),
    };
    if let Err(error) = second_start.commit(&mut state) {
        panic!("same-tick wear second start commit failed: {error}");
    }

    let equipment_revision_before_completion = state.equipment().revision();
    for _ in 1..duration.value() {
        let outcome = match advance_tick(&registries, &mut state) {
            Ok(outcome) => outcome,
            Err(error) => panic!("same-tick wear pre-completion tick failed: {error}"),
        };
        assert!(outcome.production_completions().is_empty());
        assert_eq!(
            state.equipment().revision(),
            equipment_revision_before_completion
        );
    }

    let completion = match advance_tick(&registries, &mut state) {
        Ok(outcome) => outcome,
        Err(error) => panic!("same-tick wear completion tick failed: {error}"),
    };
    assert_eq!(completion.production_completions().len(), 2);
    assert_eq!(
        state.equipment().revision(),
        equipment_revision_before_completion + 1
    );
    for equipment in [first_equipment, second_equipment] {
        assert_eq!(
            state
                .equipment()
                .get_equipment(equipment)
                .map(|record| record.condition()),
            Some(condition(999_000))
        );
    }
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn sensible_heating_rejects_heater_after_mounted_support_fails() {
    let (registries, mut state, source, _, equipment, energy_store) =
        make_loaded_fixture(EnergyCarrier::Electrical);
    let support = add_active_support(&registries, &mut state, 0);
    let mount = match validate_mount_equipment(&registries, &state, equipment, support) {
        Ok(token) => token,
        Err(error) => panic!("heater-support mount validation failed: {error}"),
    };
    if let Err(error) = mount.commit(&mut state) {
        panic!("heater-support mount commit failed: {error}");
    }
    fail_support(&registries, &mut state, support);

    assert!(matches!(
        resolve_test_sensible_heating_process(
            &registries,
            &state,
            PROCESS,
            source,
            equipment,
            energy_store,
            Temperature::from_millikelvin(303_000),
        ),
        Err(SensibleHeatingResolutionError::Equipment(
            EquipmentProviderError::StructuralSupportNotActive {
                equipment: rejected_equipment,
                element,
                lifecycle: StructuralLifecycle::Failed,
            }
        )) if rejected_equipment == equipment && element == support
    ));
}

#[test]
fn trusted_load_rejects_fixed_equipment_job_with_erased_support_requirement() {
    let registries = make_registries_with_fixed_heater();
    let (registries, mut state, source, destination, equipment, energy_store) =
        make_loaded_fixture_with_registries(
            registries,
            Condition::PRISTINE,
            Temperature::from_millikelvin(300_000),
            Energy::from_nanojoules(500_000_000),
        );
    let support = add_active_support(&registries, &mut state, 0);
    let _ = validate_mount_equipment(&registries, &state, equipment, support)
        .unwrap_or_else(|error| panic!("fixed heating fixture mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("fixed heating fixture mount commit failed: {error}"));
    let resolved = resolve_test_sensible_heating_process(
        &registries,
        &state,
        PROCESS,
        source,
        equipment,
        energy_store,
        Temperature::from_millikelvin(303_000),
    )
    .unwrap_or_else(|error| panic!("fixed heating fixture resolution failed: {error}"));
    let job = validate_start_process(
        &registries,
        &state,
        resolved.process_resolution(),
        source,
        destination,
    )
    .unwrap_or_else(|error| panic!("fixed heating fixture start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("fixed heating fixture start commit failed: {error}"));
    assert!(
        state
            .production()
            .get_job(job)
            .is_some_and(|record| record.has_required_active_support())
    );

    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("fixed heating support tamper serialization failed: {error}")
        });
    encoded["state"]["systems"]["production"]["jobs"][job.value().to_string()]["equipment"]["requires_active_support"] =
        serde_json::json!(false);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("fixed heating support tamper decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::JobEquipmentSupportRequirementMissing {
                job,
                equipment,
                definition: HEATER,
            }
        ))
    );
}

#[test]
fn trusted_load_rejects_missing_required_process_energy_supply() {
    let (registries, mut state, source, destination, equipment, energy_store) =
        make_loaded_fixture(EnergyCarrier::Electrical);
    let resolved = resolve_test_sensible_heating_process(
        &registries,
        &state,
        PROCESS,
        source,
        equipment,
        energy_store,
        Temperature::from_millikelvin(303_000),
    )
    .unwrap_or_else(|error| panic!("topology-tamper heating resolution failed: {error}"));
    let job = validate_start_process(
        &registries,
        &state,
        resolved.process_resolution(),
        source,
        destination,
    )
    .unwrap_or_else(|error| panic!("topology-tamper process start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("topology-tamper process start commit failed: {error}"));

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("topology-tamper serialization failed: {error}"));
    let resources = &mut encoded["state"]["systems"]["production"]["jobs"][job.value().to_string()]
        ["resources"];
    resources["consumed_energy"] = serde_json::Value::Null;
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("topology tamper failed structural decode: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::JobEnergyTopologyMismatch {
                job,
                process: PROCESS,
            }
        ))
    );
}

#[test]
fn trusted_load_rejects_missing_required_process_equipment() {
    let (registries, mut state, source, destination, equipment, energy_store) =
        make_loaded_fixture(EnergyCarrier::Electrical);
    let resolved = resolve_test_sensible_heating_process(
        &registries,
        &state,
        PROCESS,
        source,
        equipment,
        energy_store,
        Temperature::from_millikelvin(303_000),
    )
    .unwrap_or_else(|error| panic!("equipment-topology heating resolution failed: {error}"));
    let job = validate_start_process(
        &registries,
        &state,
        resolved.process_resolution(),
        source,
        destination,
    )
    .unwrap_or_else(|error| panic!("equipment-topology process start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("equipment-topology process start commit failed: {error}"));

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("equipment-topology serialization failed: {error}"));
    let equipment_state = &mut encoded["state"]["systems"]["production"]["jobs"]
        [job.value().to_string()]["equipment"];
    equipment_state["provider"] = serde_json::Value::Null;
    equipment_state["condition_after"] = serde_json::Value::Null;
    equipment_state["requires_active_support"] = serde_json::json!(false);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded).unwrap_or_else(|error| {
        panic!("equipment-topology tamper failed structural decode: {error}")
    });

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::JobEquipmentTopologyMismatch {
                job,
                process: PROCESS,
            }
        ))
    );
}

#[test]
fn trusted_load_rejects_fractional_sensible_heat_hidden_by_whole_nanojoule_trace() {
    let (registries, mut state, source, destination, equipment, energy_store) =
        make_loaded_fixture(EnergyCarrier::Electrical);
    let resolved = resolve_test_sensible_heating_process(
        &registries,
        &state,
        PROCESS,
        source,
        equipment,
        energy_store,
        Temperature::from_millikelvin(303_000),
    )
    .unwrap_or_else(|error| panic!("fractional-load heating fixture resolution failed: {error}"));
    let job = validate_start_process(
        &registries,
        &state,
        resolved.process_resolution(),
        source,
        destination,
    )
    .unwrap_or_else(|error| panic!("fractional-load heating fixture start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("fractional-load heating fixture commit failed: {error}"));

    let mixed = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, 1),
        CompositionComponent::new(MATERIAL_WOOD, 999_999),
    ])
    .unwrap_or_else(|error| panic!("fractional-load composition fixture failed: {error}"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("fractional-load serialization failed: {error}"));
    let trace = &mut encoded["state"]["systems"]["production"]["jobs"][job.value().to_string()]["resources"]
        ["consumed_inputs"][0]["profile"];
    trace["composition"] = serde_json::to_value(mixed).unwrap_or_else(|error| {
        panic!("fractional-load composition serialization failed: {error}")
    });
    trace["temperature"] = serde_json::json!(302_999_u32);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("fractional-load structural decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::ThermalJob(
            ThermalJobValidationError::Heat {
                job,
                error: PhaseSensibleHeatError::Heat(SensibleHeatError::FractionalNanojoule {
                    femtojoule_remainder: 986_850,
                }),
            }
        )))
    );
}

#[test]
fn trusted_load_rejects_running_job_whose_support_assignment_was_erased() {
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
    let support = add_active_support(&registries, &mut state, 0);
    let _ = validate_mount_equipment(&registries, &state, equipment, support)
        .unwrap_or_else(|error| panic!("support-state fixture mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("support-state fixture mount commit failed: {error}"));
    let resolved = resolve_test_sensible_heating_process(
        &registries,
        &state,
        PROCESS,
        source,
        equipment,
        energy_store,
        Temperature::from_millikelvin(303_000),
    )
    .unwrap_or_else(|error| panic!("support-state fixture resolution failed: {error}"));
    let job = validate_start_process(
        &registries,
        &state,
        resolved.process_resolution(),
        source,
        destination,
    )
    .unwrap_or_else(|error| panic!("support-state fixture start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("support-state fixture start commit failed: {error}"));
    assert!(
        state
            .production()
            .get_job(job)
            .is_some_and(|record| record.has_required_active_support() && !record.is_suspended())
    );

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("support-state tamper serialization failed: {error}"));
    encoded["state"]["systems"]["equipment"]["records"][equipment.value().to_string()]["supported_by"] =
        serde_json::Value::Null;
    let loads = encoded["state"]["systems"]["structures"]["elements"][support.value().to_string()]
        ["loads"]
        .as_object_mut()
        .unwrap_or_else(|| panic!("support-state structural loads were not an object"));
    assert!(loads.remove("Equipment").is_some());
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("support-state tamper decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::JobEquipmentSupportStateMismatch {
                job,
                equipment,
                requires_active_support: true,
                supported_by: None,
            }
        ))
    );
}
