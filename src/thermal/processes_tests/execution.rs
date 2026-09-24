//! Sensible-heating resolution, execution, wear, energy, and soak contracts.

use super::*;

#[test]
fn sensible_heating_consumes_exact_energy_and_completes_with_target_temperature() {
    let (registries, mut state, source, destination, equipment, energy_store) =
        make_loaded_fixture(EnergyCarrier::Electrical);
    let initial_explicit_energy = match calculate_explicit_energy_accounting(&registries, &state)
        .and_then(|accounting| {
            accounting
                .total()
                .ok_or(crate::energy::ExplicitEnergyAccountingError::Overflow)
        }) {
        Ok(total) => total,
        Err(error) => panic!("initial explicit energy accounting failed: {error}"),
    };
    let target = Temperature::from_millikelvin(303_000);
    let expected_heat = match calculate_phase_sensible_heat(
        registries.materials(),
        Mass::from_milligrams(10),
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        &MaterialComposition::pure(MATERIAL_WOOD),
        Temperature::from_millikelvin(300_000),
        target,
    ) {
        Ok(heat) => heat.energy(),
        Err(error) => panic!("expected heat fixture failed: {error}"),
    };
    let resolved = match resolve_test_sensible_heating_process(
        &registries,
        &state,
        PROCESS,
        source,
        equipment,
        energy_store,
        target,
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("sensible heating resolution failed: {error}"),
    };
    assert_eq!(resolved.required_energy(), expected_heat);
    assert_eq!(resolved.transfer_power(), Power::from_microwatts(500_000));
    let expected_duration = match calculate_power_duration_ceiling(
        resolved.transfer_power(),
        expected_heat,
        registries.core().physical_tick_duration(),
    ) {
        Ok(duration) => duration,
        Err(error) => panic!("thermal duration fixture failed: {error}"),
    };
    assert_eq!(resolved.process_resolution().duration(), expected_duration);
    assert_eq!(
        resolved.process_resolution().equipment_condition_after(),
        Some(condition(999_000))
    );

    let before_energy = state
        .energy()
        .get_store(energy_store)
        .map(|store| store.stored());
    let token = match validate_start_process(
        &registries,
        &state,
        resolved.process_resolution(),
        source,
        destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("heated process start validation failed: {error}"),
    };
    let job = match token.commit(&mut state) {
        Ok(job) => job,
        Err(error) => panic!("heated process start commit failed: {error}"),
    };
    assert_eq!(
        state
            .energy()
            .get_store(energy_store)
            .map(|store| store.stored()),
        before_energy.and_then(|energy| energy.checked_sub(expected_heat))
    );
    assert_eq!(
        state
            .production()
            .get_job(job)
            .and_then(|record| record.consumed_energy()),
        resolved.process_resolution().energy_input()
    );
    assert_eq!(
        state
            .production()
            .get_job(job)
            .and_then(|record| record.equipment_condition_after()),
        Some(condition(999_000))
    );
    assert_eq!(
        state
            .equipment()
            .get_equipment(equipment)
            .map(|record| record.condition()),
        Some(Condition::PRISTINE)
    );
    let in_flight_explicit_energy = match calculate_explicit_energy_accounting(&registries, &state)
        .and_then(|accounting| {
            accounting
                .total()
                .ok_or(crate::energy::ExplicitEnergyAccountingError::Overflow)
        }) {
        Ok(total) => total,
        Err(error) => panic!("in-flight explicit energy accounting failed: {error}"),
    };
    assert_eq!(in_flight_explicit_energy, initial_explicit_energy);

    for _ in 0..expected_duration.value() {
        if let Err(error) = advance_tick(&registries, &mut state) {
            panic!("heated process completion tick failed: {error}");
        }
    }
    assert!(state.production().get_job(job).is_none());
    assert_eq!(
        state
            .equipment()
            .get_equipment(equipment)
            .map(|record| record.condition()),
        Some(condition(999_000))
    );
    let output = match state
        .inventory()
        .lots()
        .find(|lot| lot.stockpile() == destination)
    {
        Some(output) => output,
        None => panic!("heated output lot missing after completion"),
    };
    assert_eq!(output.mass(), Mass::from_milligrams(10));
    assert_eq!(output.temperature(), target);
    assert_eq!(
        output.composition(),
        &MaterialComposition::pure(MATERIAL_WOOD)
    );
    let final_explicit_energy = match calculate_explicit_energy_accounting(&registries, &state)
        .and_then(|accounting| {
            accounting
                .total()
                .ok_or(crate::energy::ExplicitEnergyAccountingError::Overflow)
        }) {
        Ok(total) => total,
        Err(error) => panic!("final explicit energy accounting failed: {error}"),
    };
    assert_eq!(final_explicit_energy, initial_explicit_energy);
}

#[test]
fn worn_heater_derates_transfer_power_and_persisted_duration_contract() {
    let curve = CapabilityConditionCurve::new(
        HEATING_POWER,
        vec![
            CapabilityConditionPoint::new(
                Condition::FAILED,
                CapabilityValue::Power(Power::from_microwatts(1_000)),
            ),
            CapabilityConditionPoint::new(
                condition(500_000),
                CapabilityValue::Power(Power::from_microwatts(3_000)),
            ),
        ],
    );
    let registries = make_registries_with_condition_curves(
        EnergyCarrier::Electrical,
        Temperature::from_millikelvin(400_000),
        vec![curve],
    );
    let (registries, mut state, source, destination, equipment, energy_store) =
        make_loaded_fixture_with_registries(
            registries,
            condition(500_000),
            Temperature::from_millikelvin(300_000),
            Energy::from_nanojoules(500_000_000),
        );

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
        Err(error) => panic!("worn-heater resolution failed: {error}"),
    };
    assert_eq!(
        resolved.required_energy(),
        Energy::from_nanojoules(51_000_000)
    );
    assert_eq!(resolved.transfer_power(), Power::from_microwatts(3_000));
    assert_eq!(resolved.process_resolution().duration().value(), 5);

    let token = match validate_start_process(
        &registries,
        &state,
        resolved.process_resolution(),
        source,
        destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("worn-heater process start validation failed: {error}"),
    };
    let job = match token.commit(&mut state) {
        Ok(job) => job,
        Err(error) => panic!("worn-heater process start commit failed: {error}"),
    };
    let provider = match state
        .production()
        .get_job(job)
        .and_then(|record| record.equipment_provider())
    {
        Some(provider) => provider,
        None => panic!("worn-heater job lost its equipment trace"),
    };
    assert_eq!(provider.condition(), condition(500_000));
    assert_eq!(
        state
            .production()
            .get_job(job)
            .and_then(|record| record.equipment_condition_after()),
        Some(condition(495_000))
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    for _ in 0..5 {
        if let Err(error) = advance_tick(&registries, &mut state) {
            panic!("worn-heater completion failed: {error}");
        }
    }
    assert_eq!(
        state
            .equipment()
            .get_equipment(equipment)
            .map(|record| record.condition()),
        Some(condition(495_000))
    );
}

#[test]
fn sensible_heating_rejects_wrong_energy_carrier_before_mutation() {
    let (registries, state, source, _, equipment, energy_store) =
        make_loaded_fixture(EnergyCarrier::Thermal);
    let before = state.clone();

    assert_eq!(
        resolve_test_sensible_heating_process(
            &registries,
            &state,
            PROCESS,
            source,
            equipment,
            energy_store,
            Temperature::from_millikelvin(303_000),
        ),
        Err(SensibleHeatingResolutionError::WrongEnergyCarrier {
            required: EnergyCarrier::Electrical,
            provided: EnergyCarrier::Thermal,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn sensible_heating_reports_wrong_carrier_before_insufficient_energy() {
    let (registries, state, source, _, equipment, energy_store) = make_loaded_fixture_at(
        EnergyCarrier::Thermal,
        Temperature::from_millikelvin(300_000),
        Energy::ZERO,
    );
    let before = state.clone();

    assert_eq!(
        resolve_test_sensible_heating_process(
            &registries,
            &state,
            PROCESS,
            source,
            equipment,
            energy_store,
            Temperature::from_millikelvin(303_000),
        ),
        Err(SensibleHeatingResolutionError::WrongEnergyCarrier {
            required: EnergyCarrier::Electrical,
            provided: EnergyCarrier::Thermal,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn sensible_heating_rejects_noop_target_before_consuming_resources() {
    let (registries, state, source, _, equipment, energy_store) =
        make_loaded_fixture(EnergyCarrier::Electrical);
    let before = state.clone();

    assert_eq!(
        resolve_test_sensible_heating_process(
            &registries,
            &state,
            PROCESS,
            source,
            equipment,
            energy_store,
            Temperature::from_millikelvin(300_000),
        ),
        Err(SensibleHeatingResolutionError::NoHeatingRequired)
    );
    assert_eq!(state, before);
}

#[test]
fn sensible_heating_rejects_target_above_equipment_limit() {
    let (registries, state, source, _, equipment, energy_store) =
        make_loaded_fixture(EnergyCarrier::Electrical);

    assert_eq!(
        resolve_test_sensible_heating_process(
            &registries,
            &state,
            PROCESS,
            source,
            equipment,
            energy_store,
            Temperature::from_millikelvin(401_000),
        ),
        Err(
            SensibleHeatingResolutionError::TargetExceedsEquipmentMaximum {
                target: Temperature::from_millikelvin(401_000),
                maximum: Temperature::from_millikelvin(400_000),
            }
        )
    );
}

#[test]
fn warmer_input_reduces_required_energy_and_duration() {
    let cold_registries = make_registries_with_energy_output_power(
        EnergyCarrier::Electrical,
        Temperature::from_millikelvin(400_000),
        Power::from_microwatts(5_000),
    );
    let (cold_registries, cold_state, cold_source, _, cold_equipment, cold_energy) =
        make_loaded_fixture_with_registries(
            cold_registries,
            Condition::PRISTINE,
            Temperature::from_millikelvin(300_000),
            Energy::from_nanojoules(500_000_000),
        );
    let warm_registries = make_registries_with_energy_output_power(
        EnergyCarrier::Electrical,
        Temperature::from_millikelvin(400_000),
        Power::from_microwatts(5_000),
    );
    let (warm_registries, warm_state, warm_source, _, warm_equipment, warm_energy) =
        make_loaded_fixture_with_registries(
            warm_registries,
            Condition::PRISTINE,
            Temperature::from_millikelvin(302_000),
            Energy::from_nanojoules(500_000_000),
        );
    let target = Temperature::from_millikelvin(303_000);
    let cold = match resolve_test_sensible_heating_process(
        &cold_registries,
        &cold_state,
        PROCESS,
        cold_source,
        cold_equipment,
        cold_energy,
        target,
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("cold heating resolution failed: {error}"),
    };
    let warm = match resolve_test_sensible_heating_process(
        &warm_registries,
        &warm_state,
        PROCESS,
        warm_source,
        warm_equipment,
        warm_energy,
        target,
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("warm heating resolution failed: {error}"),
    };

    assert_eq!(cold.required_energy(), Energy::from_nanojoules(51_000_000));
    assert_eq!(warm.required_energy(), Energy::from_nanojoules(17_000_000));
    assert!(cold.required_energy() > warm.required_energy());
    assert_eq!(cold.process_resolution().duration().value(), 3);
    assert_eq!(warm.process_resolution().duration().value(), 1);
}

#[test]
fn selected_batch_mass_changes_heating_energy_without_static_recipe_quantity() {
    let registries = make_registries_with_energy_output_power(
        EnergyCarrier::Electrical,
        Temperature::from_millikelvin(400_000),
        Power::from_microwatts(5_000),
    );
    let (registries, state, source, _, equipment, energy_store) =
        make_loaded_fixture_with_registries(
            registries,
            Condition::PRISTINE,
            Temperature::from_millikelvin(300_000),
            Energy::from_nanojoules(500_000_000),
        );
    let lot = match state
        .inventory()
        .lots()
        .find(|lot| lot.stockpile() == source)
    {
        Some(lot) => lot.id(),
        None => panic!("selected-batch fixture lot missing"),
    };
    let target = Temperature::from_millikelvin(303_000);
    let five = match resolve_sensible_heating_process(
        &registries,
        &state,
        SensibleHeatingRequest::new(
            PROCESS,
            source,
            &[MaterialLotSelection::new(lot, Mass::from_milligrams(5))],
            equipment,
            energy_store,
            target,
        ),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("5 mg selected-batch heating failed: {error}"),
    };
    let ten = match resolve_sensible_heating_process(
        &registries,
        &state,
        SensibleHeatingRequest::new(
            PROCESS,
            source,
            &[MaterialLotSelection::new(lot, Mass::from_milligrams(10))],
            equipment,
            energy_store,
            target,
        ),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("10 mg selected-batch heating failed: {error}"),
    };

    assert_eq!(
        five.process_resolution().input_mass(),
        Mass::from_milligrams(5)
    );
    assert_eq!(
        ten.process_resolution().input_mass(),
        Mass::from_milligrams(10)
    );
    assert_eq!(five.required_energy(), Energy::from_nanojoules(25_500_000));
    assert_eq!(ten.required_energy(), Energy::from_nanojoules(51_000_000));
    assert_eq!(five.process_resolution().duration().value(), 2);
    assert_eq!(ten.process_resolution().duration().value(), 3);
}

#[test]
fn selected_batch_heating_rejects_mass_above_equipment_capacity_without_mutation() {
    let (registries, mut state, _, _, equipment, energy_store) =
        make_loaded_fixture(EnergyCarrier::Electrical);
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(source) => source,
        Err(error) => panic!("batch-capacity source allocation failed: {error}"),
    };
    let lot = match deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(21),
        Temperature::from_millikelvin(300_000),
    ) {
        Ok(lot) => lot,
        Err(error) => panic!("batch-capacity material fixture failed: {error}"),
    };
    let before = state.clone();

    assert_eq!(
        resolve_sensible_heating_process(
            &registries,
            &state,
            SensibleHeatingRequest::new(
                PROCESS,
                source,
                &[MaterialLotSelection::new(lot, Mass::from_milligrams(21))],
                equipment,
                energy_store,
                Temperature::from_millikelvin(303_000),
            ),
        ),
        Err(
            SensibleHeatingResolutionError::BatchMassExceedsEquipmentCapacity {
                selected: Mass::from_milligrams(21),
                maximum: Mass::from_milligrams(20),
            }
        )
    );
    assert_eq!(state, before);
}

#[test]
fn selected_batch_heating_uses_actual_material_heat_capacity() {
    let registries = make_registries_with_energy_output_power(
        EnergyCarrier::Electrical,
        Temperature::from_millikelvin(400_000),
        Power::from_microwatts(5_000),
    );
    let (registries, mut state, wood_source, _, equipment, energy_store) =
        make_loaded_fixture_with_registries(
            registries,
            Condition::PRISTINE,
            Temperature::from_millikelvin(300_000),
            Energy::from_nanojoules(500_000_000),
        );
    let copper_source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(source) => source,
        Err(error) => panic!("copper heating source allocation failed: {error}"),
    };
    let copper_lot = match deposit_lot_for_test(
        &registries,
        &mut state,
        copper_source,
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(300_000),
    ) {
        Ok(lot) => lot,
        Err(error) => panic!("copper heating input failed: {error}"),
    };
    let wood_lot = match state
        .inventory()
        .lots()
        .find(|lot| lot.stockpile() == wood_source)
    {
        Some(lot) => lot.id(),
        None => panic!("wood heating input disappeared"),
    };
    let target = Temperature::from_millikelvin(303_000);
    let wood = match resolve_sensible_heating_process(
        &registries,
        &state,
        SensibleHeatingRequest::new(
            PROCESS,
            wood_source,
            &[MaterialLotSelection::new(
                wood_lot,
                Mass::from_milligrams(10),
            )],
            equipment,
            energy_store,
            target,
        ),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("wood property heating resolution failed: {error}"),
    };
    let copper = match resolve_sensible_heating_process(
        &registries,
        &state,
        SensibleHeatingRequest::new(
            PROCESS,
            copper_source,
            &[MaterialLotSelection::new(
                copper_lot,
                Mass::from_milligrams(10),
            )],
            equipment,
            energy_store,
            target,
        ),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("copper property heating resolution failed: {error}"),
    };

    assert_eq!(wood.required_energy(), Energy::from_nanojoules(51_000_000));
    assert_eq!(
        copper.required_energy(),
        Energy::from_nanojoules(11_550_000)
    );
    assert_eq!(wood.process_resolution().duration().value(), 3);
    assert_eq!(copper.process_resolution().duration().value(), 1);
}

#[test]
fn sensible_heating_stops_at_material_phase_boundary() {
    let registries = make_registries_with_max_temperature(
        EnergyCarrier::Electrical,
        Temperature::from_millikelvin(2_000_000),
    );
    let mut state = AppState::new();
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(source) => source,
        Err(error) => panic!("phase-boundary source allocation failed: {error}"),
    };
    let lot = match deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(300_000),
    ) {
        Ok(lot) => lot,
        Err(error) => panic!("phase-boundary copper input failed: {error}"),
    };
    let equipment = match add_equipment(&registries, &mut state, HEATER, Condition::PRISTINE) {
        Ok(equipment) => equipment,
        Err(error) => panic!("phase-boundary heater allocation failed: {error}"),
    };
    let energy_store = match add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        BATTERY,
        Energy::from_nanojoules(500_000_000),
    ) {
        Ok(store) => store,
        Err(error) => panic!("phase-boundary energy fixture failed: {error}"),
    };
    let before = state.clone();

    assert!(matches!(
        resolve_sensible_heating_process(
            &registries,
            &state,
            SensibleHeatingRequest::new(
                PROCESS,
                source,
                &[MaterialLotSelection::new(lot, Mass::from_milligrams(10))],
                equipment,
                energy_store,
                Temperature::from_millikelvin(1_400_000),
            ),
        ),
        Err(SensibleHeatingResolutionError::Heat(
            PhaseSensibleHeatError::InvalidTargetState(
                crate::material::MaterialPhaseStateError::SolidAboveMeltingPoint {
                    material: _material,
                    temperature: _temperature,
                    melting_point: _melting_point,
                }
            )
        ))
    ));
    assert_eq!(state, before);
}

#[test]
fn selected_batch_heating_rejects_empty_selection_without_mutation() {
    let (registries, state, source, _, equipment, energy_store) =
        make_loaded_fixture(EnergyCarrier::Electrical);
    let before = state.clone();

    assert_eq!(
        resolve_sensible_heating_process(
            &registries,
            &state,
            SensibleHeatingRequest::new(
                PROCESS,
                source,
                &[],
                equipment,
                energy_store,
                Temperature::from_millikelvin(303_000),
            ),
        ),
        Err(SensibleHeatingResolutionError::Input(
            ProcessInputError::EmptySelection
        ))
    );
    assert_eq!(state, before);
}

#[test]
fn sensible_heating_rejects_insufficient_finite_energy_without_mutation() {
    let (registries, state, source, _, equipment, energy_store) = make_loaded_fixture_at(
        EnergyCarrier::Electrical,
        Temperature::from_millikelvin(300_000),
        Energy::from_nanojoules(50_000_000),
    );
    let before = state.clone();

    assert_eq!(
        resolve_test_sensible_heating_process(
            &registries,
            &state,
            PROCESS,
            source,
            equipment,
            energy_store,
            Temperature::from_millikelvin(303_000),
        ),
        Err(SensibleHeatingResolutionError::Energy(
            EnergySupplyError::InsufficientEnergy {
                store: energy_store,
                available: Energy::from_nanojoules(50_000_000),
                requested: Energy::from_nanojoules(51_000_000),
            }
        ))
    );
    assert_eq!(state, before);
}

#[test]
fn resolved_heating_energy_becomes_stale_after_independent_energy_mutation() {
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
        Err(error) => panic!("stale heating fixture resolution failed: {error}"),
    };
    let expected_revision = state.energy().revision();
    if let Err(error) = add_energy_store(&registries, &mut state, BATTERY) {
        panic!("independent energy mutation failed: {error}");
    }
    let before = state.clone();

    assert_eq!(
        validate_start_process(
            &registries,
            &state,
            resolved.process_resolution(),
            source,
            destination,
        ),
        Err(crate::production::StartProcessError::StaleResolvedEnergy {
            expected_energy_revision: expected_revision,
            actual_energy_revision: expected_revision + 1,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn validated_heating_start_rejects_stale_energy_before_consuming_matter() {
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
        Err(error) => panic!("atomic heating fixture resolution failed: {error}"),
    };
    let token = match validate_start_process(
        &registries,
        &state,
        resolved.process_resolution(),
        source,
        destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("atomic heating start validation failed: {error}"),
    };
    let expected_revision = state.energy().revision();
    if let Err(error) = add_energy_store(&registries, &mut state, BATTERY) {
        panic!("independent energy mutation failed: {error}");
    }
    let before_commit = state.clone();

    assert_eq!(
        token.commit(&mut state),
        Err(
            crate::production::StartProcessCommitError::StaleEnergyRevision {
                expected: expected_revision,
                actual: expected_revision + 1,
            }
        )
    );
    assert_eq!(state, before_commit);
    assert_eq!(state.production().jobs().count(), 0);
}

#[cfg(feature = "test-soak")]
#[path = "execution/soak.rs"]
mod soak;
