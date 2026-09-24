//! Transaction-sequence conservation, stale-token, reservation, and relocation contracts.

use super::*;

#[test]
fn transfer_split_sequence_preserves_inventory_quantity() {
    let registries = build_registries();
    let mut state = AppState::new();
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(id) => id,
        Err(error) => panic!("source fixture failed: {error}"),
    };
    let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(id) => id,
        Err(error) => panic!("destination fixture failed: {error}"),
    };
    if let Err(error) = deposit_bulk_for_test(
        &registries,
        &mut state,
        source,
        wood_log(),
        Mass::from_milligrams(10),
    ) {
        panic!("transfer source deposit failed: {error}");
    }
    let before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("initial accounting failed: {error:?}"))
        .total();

    for requested in [
        Mass::from_milligrams(3),
        Mass::from_milligrams(4),
        Mass::from_milligrams(3),
    ] {
        let token = validate_material_relocation_for_test(
            &registries,
            &state,
            source,
            destination,
            wood_log(),
            requested,
        )
        .unwrap_or_else(|error| panic!("transfer validation failed: {error}"));
        token
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("transfer commit failed: {error}"));
        assert_eq!(
            calculate_matter_accounting(&state)
                .unwrap_or_else(|error| panic!("accounting failed: {error:?}"))
                .total(),
            before,
            "partial transfer must conserve world matter"
        );
        assert_lot_aggregate_agreement(&registries, &state, "after partial transfer");
    }

    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .unwrap_or_else(|| panic!("conservation stockpile disappeared"))
            .stored_mass(),
        Mass::ZERO
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .unwrap_or_else(|| panic!("conservation stockpile disappeared"))
            .stored_mass(),
        Mass::from_milligrams(10)
    );
    assert_lot_aggregate_agreement(&registries, &state, "after transfer sequence");
}

#[test]
fn stale_transfer_commit_leaves_matter_accounting_unchanged() {
    let registries = build_registries();
    let mut state = AppState::new();
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(id) => id,
        Err(error) => panic!("source fixture failed: {error}"),
    };
    let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(5)) {
        Ok(id) => id,
        Err(error) => panic!("small destination fixture failed: {error}"),
    };
    if let Err(error) = deposit_bulk_for_test(
        &registries,
        &mut state,
        source,
        wood_log(),
        Mass::from_milligrams(10),
    ) {
        panic!("transfer source deposit failed: {error}");
    }
    let before = state.clone();
    let before_total = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("accounting failed: {error:?}"))
        .total();

    assert_eq!(
        validate_material_relocation_for_test(
            &registries,
            &state,
            source,
            destination,
            wood_log(),
            Mass::from_milligrams(11),
        ),
        Err(MaterialRelocationTestError::Selection(
            ConsumptionSelectionError::InsufficientMass {
                stockpile: source,
                commodity: wood_log(),
                available: Mass::from_milligrams(10),
                requested: Mass::from_milligrams(11),
            }
        ))
    );
    assert_eq!(
        validate_material_relocation_for_test(
            &registries,
            &state,
            source,
            destination,
            wood_log(),
            Mass::from_milligrams(9),
        ),
        Err(MaterialRelocationTestError::Relocation(
            MaterialRelocationError::DestinationCapacityExceeded {
                stockpile: destination,
                capacity: Mass::from_milligrams(5),
                committed: Mass::ZERO,
                requested: Mass::from_milligrams(9),
            }
        ))
    );
    assert_eq!(state, before, "failed validation must not mutate inventory");

    let valid = validate_material_relocation_for_test(
        &registries,
        &state,
        source,
        destination,
        wood_log(),
        Mass::from_milligrams(4),
    )
    .unwrap_or_else(|error| panic!("valid transfer validation failed: {error}"));
    add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(50))
        .unwrap_or_else(|error| panic!("revision bump failed: {error}"));
    let result = valid.commit(&mut state);
    assert!(
        matches!(
            result,
            Err(MaterialRelocationCommitError::StaleInventoryRevision {
                expected: _expected,
                actual: _actual,
            })
        ),
        "stale transfer commit must be rejected: {result:?}"
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("accounting failed: {error:?}"))
            .total(),
        before_total,
        "stale commit must not change world matter"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .unwrap_or_else(|| panic!("conservation stockpile disappeared"))
            .stored_mass(),
        Mass::from_milligrams(10),
        "stale commit must not withdraw from source"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .unwrap_or_else(|| panic!("conservation stockpile disappeared"))
            .stored_mass(),
        Mass::ZERO,
        "stale commit must not deposit into destination"
    );
    assert_lot_aggregate_agreement(&registries, &state, "after stale commit");
}

#[test]
fn consumption_reservation_and_reserved_deposit_preserve_final_quantity() {
    let registries = build_registries();
    let mut state = AppState::new();
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(id) => id,
        Err(error) => panic!("source fixture failed: {error}"),
    };
    let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(id) => id,
        Err(error) => panic!("destination fixture failed: {error}"),
    };
    if let Err(error) = deposit_bulk_for_test(
        &registries,
        &mut state,
        source,
        wood_log(),
        Mass::from_milligrams(10),
    ) {
        panic!("reservation source deposit failed: {error}");
    }
    let before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("accounting failed: {error:?}"))
        .total();

    let inputs = vec![MaterialInputSpec::new(
        wood_log(),
        Mass::from_milligrams(10),
    )];
    let selection = validate_consumption_selection(state.inventory(), source, &inputs)
        .unwrap_or_else(|error| panic!("selection failed: {error:?}"));
    assert_eq!(
        selection.total_consumed(),
        Mass::from_milligrams(10),
        "selection must bind exactly the requested input mass"
    );
    let mut inbound_by_destination = BTreeMap::new();
    inbound_by_destination.insert(destination, Mass::from_milligrams(10));
    let reservation = validate_consumption_reservation_from_selection(
        state.inventory(),
        selection,
        inbound_by_destination,
    )
    .unwrap_or_else(|error| panic!("reservation failed: {error:?}"));
    assert_eq!(
        reservation.source_stored_mass_after(state.inventory()),
        Mass::ZERO,
        "consumption reservation must own source post-withdrawal mass projection"
    );
    apply_consumption_reservation(state.inventory_state_mut(), reservation)
        .unwrap_or_else(|error| panic!("reservation commit failed: {error:?}"));
    assert_lot_aggregate_agreement(&registries, &state, "after reservation");
    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .unwrap_or_else(|| panic!("conservation stockpile disappeared"))
            .stored_mass(),
        Mass::ZERO,
        "consumption must drain the source"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .unwrap_or_else(|| panic!("conservation stockpile disappeared"))
            .reserved_inbound(),
        Mass::from_milligrams(10),
        "reserved inbound must reflect the incoming output mass"
    );

    let output = MaterialLotSpec::new(
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(500_000),
    );
    let created_at = state.tick();
    let deposit_plan = decide_reserved_deposits(
        &registries,
        state.inventory(),
        created_at,
        created_at,
        vec![ReservedDepositRequest::new(destination, vec![output], 0)],
    )
    .unwrap_or_else(|error| panic!("reserved deposit planning failed: {error:?}"));
    apply_reserved_deposits(state.inventory_state_mut(), deposit_plan);
    assert_lot_aggregate_agreement(&registries, &state, "after reserved deposit");
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .unwrap_or_else(|| panic!("conservation stockpile disappeared"))
            .reserved_inbound(),
        Mass::ZERO,
        "reserved inbound must be consumed by the deposit"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .unwrap_or_else(|| panic!("conservation stockpile disappeared"))
            .stored_mass(),
        Mass::from_milligrams(10),
        "deposit must land the output mass in stored inventory"
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("accounting failed: {error:?}"))
            .total(),
        before,
        "reserved deposit must not change world matter"
    );
}

#[test]
fn egress_and_ingress_round_trip_preserves_exact_quantity() {
    let registries = build_registries();
    let mut state = AppState::new();
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(id) => id,
        Err(error) => panic!("source fixture failed: {error}"),
    };
    let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(id) => id,
        Err(error) => panic!("destination fixture failed: {error}"),
    };
    if let Err(error) = deposit_bulk_for_test(
        &registries,
        &mut state,
        source,
        wood_log(),
        Mass::from_milligrams(10),
    ) {
        panic!("egress source deposit failed: {error}");
    }
    let before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("accounting failed: {error:?}"))
        .total();

    let inputs = vec![MaterialInputSpec::new(wood_log(), Mass::from_milligrams(7))];
    let selection = validate_consumption_selection(state.inventory(), source, &inputs)
        .unwrap_or_else(|error| panic!("selection failed: {error:?}"));
    let egress = validate_material_egress_from_selection(state.inventory(), selection)
        .unwrap_or_else(|error| panic!("egress failed: {error:?}"));
    assert_eq!(egress.total_consumed(), Mass::from_milligrams(7));
    assert_eq!(
        egress.source_stored_mass_after(state.inventory()),
        Mass::from_milligrams(3),
        "egress must own the authoritative post-withdrawal stored-mass projection"
    );
    let traces = egress.consumed_inputs().to_vec();
    apply_material_egress(state.inventory_state_mut(), egress);
    assert_lot_aggregate_agreement(&registries, &state, "after egress");
    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .unwrap_or_else(|| panic!("conservation stockpile disappeared"))
            .stored_mass(),
        Mass::from_milligrams(3),
        "egress must remove exactly the selected mass"
    );

    let ingress = validate_material_ingress(
        &registries,
        state.inventory(),
        destination,
        traces.iter().map(MaterialIngressEntry::from_consumed_trace),
        state.tick(),
    )
    .unwrap_or_else(|error| panic!("ingress failed: {error:?}"));
    apply_material_ingress(state.inventory_state_mut(), ingress);
    assert_lot_aggregate_agreement(&registries, &state, "after ingress");
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .unwrap_or_else(|| panic!("conservation stockpile disappeared"))
            .stored_mass(),
        Mass::from_milligrams(7),
        "ingress must restore exactly the egressed mass"
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("accounting failed: {error:?}"))
            .total(),
        before,
        "egress plus ingress round trip must conserve world matter"
    );
}

#[test]
fn exact_relocation_preserves_inventory_quantity() {
    let registries = build_registries();
    let mut state = AppState::new();
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(id) => id,
        Err(error) => panic!("source fixture failed: {error}"),
    };
    let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(id) => id,
        Err(error) => panic!("destination fixture failed: {error}"),
    };
    if let Err(error) = deposit_bulk_for_test(
        &registries,
        &mut state,
        source,
        wood_log(),
        Mass::from_milligrams(10),
    ) {
        panic!("relocation source deposit failed: {error}");
    }
    let before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("accounting failed: {error:?}"))
        .total();
    let energy_before = calculate_explicit_energy_accounting(&registries, &state)
        .unwrap_or_else(|error| panic!("relocation energy-before accounting failed: {error}"))
        .total();

    let inputs = vec![MaterialInputSpec::new(wood_log(), Mass::from_milligrams(6))];
    let selection = validate_consumption_selection(state.inventory(), source, &inputs)
        .unwrap_or_else(|error| panic!("selection failed: {error:?}"));
    let relocation =
        validate_material_relocation_from_selection(&registries, &state, destination, selection)
            .unwrap_or_else(|error| panic!("relocation failed: {error:?}"));
    assert_eq!(relocation.total_mass(), Mass::from_milligrams(6));
    relocation
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("relocation commit failed: {error:?}"));
    assert_lot_aggregate_agreement(&registries, &state, "after relocation");
    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .unwrap_or_else(|| panic!("conservation stockpile disappeared"))
            .stored_mass(),
        Mass::from_milligrams(4),
        "relocation must leave the unselected mass in source"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .unwrap_or_else(|| panic!("conservation stockpile disappeared"))
            .stored_mass(),
        Mass::from_milligrams(6),
        "relocation must land the selected mass in destination"
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("accounting failed: {error:?}"))
            .total(),
        before,
        "relocation must conserve world matter"
    );
    assert_eq!(
        calculate_explicit_energy_accounting(&registries, &state)
            .unwrap_or_else(|error| panic!("relocation energy-after accounting failed: {error}"))
            .total(),
        energy_before,
        "relocation must preserve exact material thermal energy"
    );
}
