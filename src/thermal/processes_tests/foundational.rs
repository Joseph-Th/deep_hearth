//! Foundational sensible-heating energy integration and phase-state contracts.

use super::*;

#[test]
fn sensible_heating_sums_fractional_trace_energy_before_transaction_quantization() {
    let registries = make_registries_with_energy_output_power(
        EnergyCarrier::Electrical,
        Temperature::from_millikelvin(400_000),
        Power::from_microwatts(5_000),
    );
    let mut state = AppState::new();
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10))
        .unwrap_or_else(|error| panic!("fractional-batch source failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10))
        .unwrap_or_else(|error| panic!("fractional-batch destination failed: {error}"));
    let half_copper = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, 500_000),
        CompositionComponent::new(crate::content::MATERIAL_SLAG, 500_000),
    ])
    .unwrap_or_else(|error| panic!("fractional-batch half-copper composition failed: {error}"));
    let lean_copper = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, 100_000),
        CompositionComponent::new(crate::content::MATERIAL_SLAG, 900_000),
    ])
    .unwrap_or_else(|error| panic!("fractional-batch lean-copper composition failed: {error}"));
    let input_temperature = Temperature::from_millikelvin(300_000);
    let target = Temperature::from_millikelvin(300_001);
    let first = deposit_composed_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        Mass::from_milligrams(1),
        input_temperature,
        half_copper,
    )
    .unwrap_or_else(|error| panic!("fractional-batch first lot failed: {error}"));
    let second = deposit_composed_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        Mass::from_milligrams(1),
        input_temperature,
        lean_copper,
    )
    .unwrap_or_else(|error| panic!("fractional-batch second lot failed: {error}"));
    let equipment = add_equipment(&registries, &mut state, HEATER, Condition::PRISTINE)
        .unwrap_or_else(|error| panic!("fractional-batch equipment failed: {error}"));
    let energy_store = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        BATTERY,
        Energy::from_nanojoules(10_000),
    )
    .unwrap_or_else(|error| panic!("fractional-batch energy store failed: {error}"));

    let initial_explicit_energy = calculate_explicit_energy_accounting(&registries, &state)
        .and_then(|accounting| {
            accounting
                .total()
                .ok_or(crate::energy::ExplicitEnergyAccountingError::Overflow)
        })
        .unwrap_or_else(|error| {
            panic!("fractional-batch initial energy accounting failed: {error}")
        });
    let selections = [
        MaterialLotSelection::new(first, Mass::from_milligrams(1)),
        MaterialLotSelection::new(second, Mass::from_milligrams(1)),
    ];
    let resolved = resolve_sensible_heating_process(
        &registries,
        &state,
        SensibleHeatingRequest::new(
            PROCESS,
            source,
            &selections,
            equipment,
            energy_store,
            target,
        ),
    )
    .unwrap_or_else(|error| {
        panic!("complementary fractional trace energy should resolve as one batch: {error}")
    });
    assert_eq!(resolved.required_energy(), Energy::from_nanojoules(1_491));

    let job = validate_start_process(
        &registries,
        &state,
        resolved.process_resolution(),
        source,
        destination,
    )
    .unwrap_or_else(|error| panic!("fractional-batch production start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("fractional-batch production commit failed: {error}"));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let in_flight_explicit_energy = calculate_explicit_energy_accounting(&registries, &state)
        .and_then(|accounting| {
            accounting
                .total()
                .ok_or(crate::energy::ExplicitEnergyAccountingError::Overflow)
        })
        .unwrap_or_else(|error| panic!("fractional-batch in-flight accounting failed: {error}"));
    assert_eq!(in_flight_explicit_energy, initial_explicit_energy);

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("fractional-batch save failed: {error}"));
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("fractional-batch decode failed: {error}"));
    let mut loaded = loaded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("fractional-batch trusted replay failed: {error}"));
    assert_eq!(loaded, state);
    assert!(loaded.production().get_job(job).is_some());

    let duration = state
        .production()
        .get_job(job)
        .map(|record| record.completes_at().value() - state.tick().value())
        .unwrap_or_else(|| panic!("fractional-batch job disappeared before completion"));
    for _ in 0..duration {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("fractional-batch source completion failed: {error}"));
        let _ = advance_tick(&registries, &mut loaded)
            .unwrap_or_else(|error| panic!("fractional-batch loaded completion failed: {error}"));
    }
    assert_eq!(loaded, state);
    assert!(state.production().get_job(job).is_none());
    let final_explicit_energy = calculate_explicit_energy_accounting(&registries, &state)
        .and_then(|accounting| {
            accounting
                .total()
                .ok_or(crate::energy::ExplicitEnergyAccountingError::Overflow)
        })
        .unwrap_or_else(|error| panic!("fractional-batch final accounting failed: {error}"));
    assert_eq!(final_explicit_energy, initial_explicit_energy);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn sensible_heating_can_superheat_liquid_without_reapplying_fusion_energy() {
    let registries = make_registries_with_max_temperature(
        EnergyCarrier::Electrical,
        Temperature::from_millikelvin(1_500_000),
    );
    let mut state = AppState::new();
    let liquid_profile =
        match StockpileStorageProfile::new(false, true, Temperature::from_millikelvin(1_500_000)) {
            Ok(profile) => profile,
            Err(error) => panic!("liquid heating storage profile failed: {error}"),
        };
    let source = match add_stockpile(&mut state, Mass::from_milligrams(100), liquid_profile) {
        Ok(source) => source,
        Err(error) => panic!("liquid heating source failed: {error}"),
    };
    let destination = match add_stockpile(&mut state, Mass::from_milligrams(100), liquid_profile) {
        Ok(destination) => destination,
        Err(error) => panic!("liquid heating destination failed: {error}"),
    };
    let melting_point = Temperature::from_millikelvin(1_357_770);
    let target = Temperature::from_millikelvin(1_400_000);
    let lot = match deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN),
        Mass::from_milligrams(10),
        melting_point,
    ) {
        Ok(lot) => lot,
        Err(error) => panic!("liquid heating input failed: {error}"),
    };
    let equipment = match add_equipment(&registries, &mut state, HEATER, Condition::PRISTINE) {
        Ok(equipment) => equipment,
        Err(error) => panic!("liquid heating equipment failed: {error}"),
    };
    let energy_store = match add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        BATTERY,
        Energy::from_nanojoules(1_000_000_000),
    ) {
        Ok(store) => store,
        Err(error) => panic!("liquid heating energy store failed: {error}"),
    };
    let initial_energy =
        match calculate_explicit_energy_accounting(&registries, &state).and_then(|accounting| {
            accounting
                .total()
                .ok_or(crate::energy::ExplicitEnergyAccountingError::Overflow)
        }) {
            Ok(total) => total,
            Err(error) => panic!("liquid heating initial accounting failed: {error}"),
        };
    let expected_heat = match calculate_phase_sensible_heat(
        registries.materials(),
        Mass::from_milligrams(10),
        CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN),
        &MaterialComposition::pure(MATERIAL_COPPER),
        melting_point,
        target,
    ) {
        Ok(heat) => heat.energy(),
        Err(error) => panic!("liquid heating expected heat failed: {error}"),
    };

    let resolved = match resolve_sensible_heating_process(
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
        Err(error) => panic!("liquid sensible-heating resolution failed: {error}"),
    };
    assert_eq!(resolved.required_energy(), expected_heat);
    assert_eq!(
        resolved.process_resolution().outputs()[0].commodity(),
        CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN)
    );
    let duration = resolved.process_resolution().duration();
    let token = match validate_start_process(
        &registries,
        &state,
        resolved.process_resolution(),
        source,
        destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("liquid heating start validation failed: {error}"),
    };
    if let Err(error) = token.commit(&mut state) {
        panic!("liquid heating start commit failed: {error}");
    }
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    assert_eq!(
        calculate_explicit_energy_accounting(&registries, &state)
            .ok()
            .and_then(|accounting| accounting.total()),
        Some(initial_energy)
    );

    for _ in 0..duration.value() {
        if let Err(error) = advance_tick(&registries, &mut state) {
            panic!("liquid heating completion failed: {error}");
        }
    }
    let output = match state
        .inventory()
        .lots()
        .find(|candidate| candidate.stockpile() == destination)
    {
        Some(output) => output,
        None => panic!("liquid heating output missing"),
    };
    assert_eq!(
        output.commodity(),
        CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN)
    );
    assert_eq!(output.temperature(), target);
    assert_eq!(
        calculate_explicit_energy_accounting(&registries, &state)
            .ok()
            .and_then(|accounting| accounting.total()),
        Some(initial_energy)
    );
}
