//! Relocation integrity, material reform, storage-history, and randomized conservation contracts.

use super::*;

#[test]
fn relocation_consolidates_repeated_slices_that_exhaust_one_source_lot() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1A70_2020));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("consolidated relocation source failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("consolidated relocation destination failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        wood_log(),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(300_000),
    )
    .unwrap_or_else(|error| panic!("consolidated relocation lot failed: {error}"));
    let cursor_before = state.inventory().next_lot_id();
    let inputs = [
        MaterialInputSpec::new(wood_log(), Mass::from_milligrams(4)),
        MaterialInputSpec::new(wood_log(), Mass::from_milligrams(6)),
    ];
    let selection = validate_consumption_selection(state.inventory(), source, &inputs)
        .unwrap_or_else(|error| panic!("consolidated relocation selection failed: {error:?}"));
    assert_eq!(selection.lot_slices.len(), 2);

    validate_material_relocation_from_selection(&registries, &state, destination, selection)
        .unwrap_or_else(|error| panic!("consolidated relocation validation failed: {error:?}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("consolidated relocation commit failed: {error:?}"));

    assert_eq!(
        state
            .inventory()
            .get_lot(lot)
            .map(MaterialLotRecord::stockpile),
        Some(destination)
    );
    assert_eq!(state.inventory().next_lot_id(), cursor_before);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::from_milligrams(10))
    );
}

#[test]
fn relocation_rejects_corrupt_source_lot_index_before_mutation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1A70_2021));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("corrupt relocation source failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("corrupt relocation destination failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        wood_log(),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(300_000),
    )
    .unwrap_or_else(|error| panic!("corrupt relocation lot failed: {error}"));
    let selection = validate_consumption_selection(
        state.inventory(),
        source,
        &[MaterialInputSpec::new(
            wood_log(),
            Mass::from_milligrams(10),
        )],
    )
    .unwrap_or_else(|error| panic!("corrupt relocation selection failed: {error:?}"));
    let relocation =
        validate_material_relocation_from_selection(&registries, &state, destination, selection)
            .unwrap_or_else(|error| panic!("corrupt relocation validation failed: {error:?}"));

    state
        .inventory_state_mut()
        .remove_lot_index(source, wood_log(), lot);
    let before = state.clone();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        relocation
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("corrupt relocation returned commit error: {error:?}"));
    }));

    assert!(result.is_err());
    assert_eq!(state, before);
}

#[test]
fn exact_reform_changes_only_physical_form_and_conserves_matter() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1A70_2007));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("reform source fixture failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("reform destination fixture failed: {error}"));
    deposit_bulk_for_test(
        &registries,
        &mut state,
        source,
        wood_log(),
        Mass::from_milligrams(10),
    )
    .unwrap_or_else(|error| panic!("reform source deposit failed: {error}"));
    let before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("reform accounting failed: {error:?}"))
        .total();
    let energy_before = calculate_explicit_energy_accounting(&registries, &state)
        .unwrap_or_else(|error| panic!("reform energy-before accounting failed: {error}"))
        .total();

    let inputs = [MaterialInputSpec::new(wood_log(), Mass::from_milligrams(6))];
    let invalid_selection = validate_consumption_selection(state.inventory(), source, &inputs)
        .unwrap_or_else(|error| panic!("reform selection failed: {error:?}"));
    assert_eq!(
        validate_material_reform_from_selection(
            &registries,
            &state,
            destination,
            CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
            invalid_selection,
        ),
        Err(MaterialReformError::MaterialChanged {
            source: MATERIAL_WOOD,
            target: MATERIAL_STONE,
        })
    );

    let selection = validate_consumption_selection(state.inventory(), source, &inputs)
        .unwrap_or_else(|error| panic!("reform selection failed: {error:?}"));
    let target = CommodityKey::new(MATERIAL_WOOD, FORM_CHIP);
    let reform = validate_material_reform_from_selection(
        &registries,
        &state,
        destination,
        target,
        selection,
    )
    .unwrap_or_else(|error| panic!("reform validation failed: {error:?}"));
    assert_eq!(reform.total_mass(), Mass::from_milligrams(6));
    reform
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("reform commit failed: {error:?}"));

    assert_lot_aggregate_agreement(&registries, &state, "after form reform");
    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .map(|stockpile| stockpile.get_mass(wood_log())),
        Some(Mass::from_milligrams(4))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| stockpile.get_mass(target)),
        Some(Mass::from_milligrams(6))
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("reform accounting failed: {error:?}"))
            .total(),
        before,
        "same-material form reform must conserve world matter"
    );
    assert_eq!(
        calculate_explicit_energy_accounting(&registries, &state)
            .unwrap_or_else(|error| panic!("reform energy-after accounting failed: {error}"))
            .total(),
        energy_before,
        "same-phase form reform must preserve exact material thermal energy"
    );
    validate_loaded_inventory(registries.materials(), state.inventory(), state.tick())
        .unwrap_or_else(|error| panic!("reformed inventory failed validation: {error}"));
}

#[test]
fn exact_reform_rejects_phase_change_at_shared_melting_boundary_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1A70_2013));
    let profile =
        StockpileStorageProfile::new(true, true, Temperature::from_millikelvin(2_000_000))
            .unwrap_or_else(|error| panic!("phase-change reform storage profile failed: {error}"));
    let stockpile = add_stockpile(&mut state, Mass::from_milligrams(20), profile)
        .unwrap_or_else(|error| panic!("phase-change reform stockpile failed: {error}"));
    let melting_point = registries
        .materials()
        .get_material(MATERIAL_COPPER)
        .and_then(|definition| definition.properties().thermal().melting_point())
        .unwrap_or_else(|| panic!("copper fixture lost its melting point"));
    let ingot = CommodityKey::new(MATERIAL_COPPER, FORM_INGOT);
    deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        ingot,
        Mass::from_milligrams(10),
        melting_point,
    )
    .unwrap_or_else(|error| panic!("phase-change reform source deposit failed: {error}"));
    let selection = validate_consumption_selection(
        state.inventory(),
        stockpile,
        &[MaterialInputSpec::new(ingot, Mass::from_milligrams(10))],
    )
    .unwrap_or_else(|error| panic!("phase-change reform selection failed: {error:?}"));
    let before = state.clone();

    assert_eq!(
        validate_material_reform_from_selection(
            &registries,
            &state,
            stockpile,
            CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN),
            selection,
        ),
        Err(MaterialReformError::PhaseChanged {
            source: FORM_INGOT,
            target: FORM_MOLTEN,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn exact_reform_rejects_unauthored_material_form_pair_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1A70_2012));
    let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10))
        .unwrap_or_else(|error| panic!("unsupported reform stockpile failed: {error}"));
    deposit_bulk_for_test(
        &registries,
        &mut state,
        stockpile,
        wood_log(),
        Mass::from_milligrams(10),
    )
    .unwrap_or_else(|error| panic!("unsupported reform source deposit failed: {error}"));
    let selection = validate_consumption_selection(
        state.inventory(),
        stockpile,
        &[MaterialInputSpec::new(wood_log(), Mass::from_milligrams(6))],
    )
    .unwrap_or_else(|error| panic!("unsupported reform selection failed: {error:?}"));
    let target = CommodityKey::new(MATERIAL_WOOD, FORM_LUMP);
    let before = state.clone();

    assert_eq!(
        validate_material_reform_from_selection(&registries, &state, stockpile, target, selection),
        Err(MaterialReformError::DestinationStorage(
            StockpileStorageError::UnsupportedCommodity { commodity: target }
        ))
    );
    assert_eq!(state, before);
}

#[test]
fn exact_reform_can_return_changed_form_to_the_source_stockpile() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1A70_2008));
    let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10))
        .unwrap_or_else(|error| panic!("in-place reform stockpile failed: {error}"));
    deposit_bulk_for_test(
        &registries,
        &mut state,
        stockpile,
        wood_log(),
        Mass::from_milligrams(10),
    )
    .unwrap_or_else(|error| panic!("in-place reform source deposit failed: {error}"));
    let before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("in-place reform accounting failed: {error:?}"))
        .total();
    let target = CommodityKey::new(MATERIAL_WOOD, FORM_CHIP);
    let selection = validate_consumption_selection(
        state.inventory(),
        stockpile,
        &[MaterialInputSpec::new(wood_log(), Mass::from_milligrams(6))],
    )
    .unwrap_or_else(|error| panic!("in-place reform selection failed: {error:?}"));

    validate_material_reform_from_selection(&registries, &state, stockpile, target, selection)
        .unwrap_or_else(|error| panic!("in-place reform validation failed: {error:?}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("in-place reform commit failed: {error:?}"));

    let record = state
        .inventory()
        .get_stockpile(stockpile)
        .unwrap_or_else(|| panic!("in-place reform stockpile disappeared"));
    assert_eq!(record.stored_mass(), Mass::from_milligrams(10));
    assert_eq!(record.get_mass(wood_log()), Mass::from_milligrams(4));
    assert_eq!(record.get_mass(target), Mass::from_milligrams(6));
    assert_eq!(
        calculate_matter_accounting(&state).map(|accounting| accounting.total()),
        Ok(before)
    );
    validate_loaded_inventory(registries.materials(), state.inventory(), state.tick())
        .unwrap_or_else(|error| panic!("in-place reform failed validation: {error}"));
}

#[test]
fn material_reform_rejects_a_noop_target_form_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1A70_2011));
    let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10))
        .unwrap_or_else(|error| panic!("noop reform stockpile failed: {error}"));
    deposit_bulk_for_test(
        &registries,
        &mut state,
        stockpile,
        wood_log(),
        Mass::from_milligrams(10),
    )
    .unwrap_or_else(|error| panic!("noop reform source deposit failed: {error}"));
    let selection = validate_consumption_selection(
        state.inventory(),
        stockpile,
        &[MaterialInputSpec::new(wood_log(), Mass::from_milligrams(6))],
    )
    .unwrap_or_else(|error| panic!("noop reform selection failed: {error:?}"));
    let before = state.clone();

    assert_eq!(
        validate_material_reform_from_selection(
            &registries,
            &state,
            stockpile,
            wood_log(),
            selection,
        ),
        Err(MaterialReformError::TargetUnchanged {
            commodity: wood_log(),
        })
    );
    assert_eq!(state, before);
}

#[test]
fn equal_preservation_relocations_do_not_accumulate_checkpoint_rounding() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1A70_2012));
    let source = add_stockpile(
        &mut state,
        Mass::from_milligrams(100),
        triple_preservation_profile(),
    )
    .unwrap_or_else(|error| panic!("same-rate relocation source failed: {error}"));
    let destination = add_stockpile(
        &mut state,
        Mass::from_milligrams(100),
        triple_preservation_profile(),
    )
    .unwrap_or_else(|error| panic!("same-rate relocation destination failed: {error}"));
    let control = add_stockpile(
        &mut state,
        Mass::from_milligrams(100),
        triple_preservation_profile(),
    )
    .unwrap_or_else(|error| panic!("same-rate relocation control failed: {error}"));
    let moved = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        wood_log(),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("same-rate relocation moved lot failed: {error}"));
    let stationary = deposit_lot_for_test(
        &registries,
        &mut state,
        control,
        wood_log(),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("same-rate relocation control lot failed: {error}"));

    apply_clock_advance(&mut state, SimulationTick::new(1));
    validate_material_transfer_for_test(
        &registries,
        &state,
        source,
        destination,
        wood_log(),
        Mass::from_milligrams(10),
    )
    .unwrap_or_else(|error| panic!("same-rate first relocation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("same-rate first relocation commit failed: {error}"));
    apply_clock_advance(&mut state, SimulationTick::new(2));
    validate_material_transfer_for_test(
        &registries,
        &state,
        destination,
        source,
        wood_log(),
        Mass::from_milligrams(10),
    )
    .unwrap_or_else(|error| panic!("same-rate second relocation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("same-rate second relocation commit failed: {error}"));
    apply_clock_advance(&mut state, SimulationTick::new(3));

    assert_eq!(projected_storage_age_parts(&state, stationary), 1_000_000);
    assert_eq!(
        projected_storage_age_parts(&state, moved),
        projected_storage_age_parts(&state, stationary),
        "equal-rate relocation must not age matter based on transaction segmentation"
    );
}

#[test]
fn equal_preservation_coalescing_does_not_reencode_storage_age() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1A70_2013));
    let source = add_stockpile(
        &mut state,
        Mass::from_milligrams(100),
        triple_preservation_profile(),
    )
    .unwrap_or_else(|error| panic!("same-rate merge source failed: {error}"));
    let destination = add_stockpile(
        &mut state,
        Mass::from_milligrams(100),
        triple_preservation_profile(),
    )
    .unwrap_or_else(|error| panic!("same-rate merge destination failed: {error}"));
    let control = add_stockpile(
        &mut state,
        Mass::from_milligrams(100),
        triple_preservation_profile(),
    )
    .unwrap_or_else(|error| panic!("same-rate merge control failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        wood_log(),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("same-rate merge incoming lot failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        destination,
        wood_log(),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("same-rate merge destination lot failed: {error}"));
    let stationary = deposit_lot_for_test(
        &registries,
        &mut state,
        control,
        wood_log(),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("same-rate merge control lot failed: {error}"));

    apply_clock_advance(&mut state, SimulationTick::new(1));
    validate_material_transfer_for_test(
        &registries,
        &state,
        source,
        destination,
        wood_log(),
        Mass::from_milligrams(10),
    )
    .unwrap_or_else(|error| panic!("same-rate coalescing relocation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("same-rate coalescing commit failed: {error}"));
    assert_eq!(state.inventory().lot_ids(destination).count(), 1);
    let merged = state
        .inventory()
        .lot_ids(destination)
        .next()
        .unwrap_or_else(|| panic!("same-rate merged lot disappeared"));
    apply_clock_advance(&mut state, SimulationTick::new(3));

    assert_eq!(projected_storage_age_parts(&state, stationary), 1_000_000);
    assert_eq!(
        projected_storage_age_parts(&state, merged),
        projected_storage_age_parts(&state, stationary),
        "coalescing equal-rate cohorts must not create a storage-age checkpoint"
    );
}

#[test]
fn age_sensitive_lots_with_equal_current_age_but_divergent_future_age_do_not_merge() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1A70_2015));
    let source_profile = StockpileStorageProfile::with_preservation(
        true,
        false,
        Temperature::from_millikelvin(350_000),
        2_999_999,
    )
    .unwrap_or_else(|error| panic!("phase-sensitive source profile failed: {error}"));
    let source = add_stockpile(&mut state, Mass::from_milligrams(100), source_profile)
        .unwrap_or_else(|error| panic!("phase-sensitive source stockpile failed: {error}"));
    let destination = add_stockpile(
        &mut state,
        Mass::from_milligrams(100),
        triple_preservation_profile(),
    )
    .unwrap_or_else(|error| panic!("phase-sensitive destination stockpile failed: {error}"));
    let berries = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    let incoming = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        berries,
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("phase-sensitive incoming berries failed: {error}"));
    let existing = deposit_lot_for_test(
        &registries,
        &mut state,
        destination,
        berries,
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("phase-sensitive existing berries failed: {error}"));

    apply_clock_advance(&mut state, SimulationTick::new(1));
    let source_age = projected_storage_age_parts(&state, incoming);
    let destination_age = projected_storage_age_parts(&state, existing);
    assert_eq!(source_age, 333_334);
    assert_eq!(destination_age, source_age);

    validate_material_transfer_for_test(
        &registries,
        &state,
        source,
        destination,
        berries,
        Mass::from_milligrams(10),
    )
    .unwrap_or_else(|error| panic!("phase-sensitive relocation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("phase-sensitive relocation commit failed: {error}"));

    assert_eq!(
        state.inventory().lot_ids(destination).count(),
        2,
        "age-sensitive cohorts that merely coincide now must not merge when their future projections diverge"
    );
    assert!(state.inventory().get_lot(incoming).is_some());
    assert!(state.inventory().get_lot(existing).is_some());

    apply_clock_advance(&mut state, SimulationTick::new(2));
    assert_eq!(projected_storage_age_parts(&state, existing), 666_667);
    assert_eq!(projected_storage_age_parts(&state, incoming), 666_668);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn same_rate_reform_preserves_uninterrupted_storage_history() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1A70_2014));
    let stockpile = add_stockpile(
        &mut state,
        Mass::from_milligrams(100),
        triple_preservation_profile(),
    )
    .unwrap_or_else(|error| panic!("same-rate reform stockpile failed: {error}"));
    let control = add_stockpile(
        &mut state,
        Mass::from_milligrams(100),
        triple_preservation_profile(),
    )
    .unwrap_or_else(|error| panic!("same-rate reform control failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        wood_log(),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("same-rate reform source lot failed: {error}"));
    let stationary = deposit_lot_for_test(
        &registries,
        &mut state,
        control,
        wood_log(),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("same-rate reform control lot failed: {error}"));

    apply_clock_advance(&mut state, SimulationTick::new(1));
    let selection = validate_consumption_selection(
        state.inventory(),
        stockpile,
        &[MaterialInputSpec::new(
            wood_log(),
            Mass::from_milligrams(10),
        )],
    )
    .unwrap_or_else(|error| panic!("same-rate reform selection failed: {error:?}"));
    validate_material_reform_from_selection(
        &registries,
        &state,
        stockpile,
        CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
        selection,
    )
    .unwrap_or_else(|error| panic!("same-rate reform validation failed: {error:?}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("same-rate reform commit failed: {error:?}"));
    let reformed = state
        .inventory()
        .lot_ids(stockpile)
        .find(|id| {
            state
                .inventory()
                .get_lot(*id)
                .is_some_and(|lot| lot.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_CHIP))
        })
        .unwrap_or_else(|| panic!("same-rate reform output disappeared"));
    apply_clock_advance(&mut state, SimulationTick::new(3));

    assert_eq!(projected_storage_age_parts(&state, stationary), 1_000_000);
    assert_eq!(
        projected_storage_age_parts(&state, reformed),
        projected_storage_age_parts(&state, stationary),
        "same-rate reform must not age matter based on the reform transaction boundary"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn material_reform_preserves_accumulated_storage_exposure() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1A70_2009));
    let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("reform-age stockpile failed: {error}"));
    let source_commodity = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        source_commodity,
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("reform-age wood fixture failed: {error}"));
    apply_clock_advance(&mut state, SimulationTick::new(72_000));
    let preservation = state
        .inventory()
        .get_stockpile(stockpile)
        .unwrap_or_else(|| panic!("reform-age stockpile disappeared"))
        .storage_profile()
        .preservation_multiplier_ppm();
    let exposure_before = state
        .inventory()
        .get_lot(lot)
        .unwrap_or_else(|| panic!("reform-age source lot disappeared"))
        .storage_history()
        .project(state.tick(), preservation)
        .unwrap_or_else(|| panic!("reform-age source exposure overflowed"));
    let selection = validate_consumption_selection(
        state.inventory(),
        stockpile,
        &[MaterialInputSpec::new(
            source_commodity,
            Mass::from_milligrams(10),
        )],
    )
    .unwrap_or_else(|error| panic!("reform-age selection failed: {error:?}"));

    validate_material_reform_from_selection(
        &registries,
        &state,
        stockpile,
        CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
        selection,
    )
    .unwrap_or_else(|error| panic!("reform-age validation failed: {error:?}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("reform-age commit failed: {error:?}"));

    let reformed = state
        .inventory()
        .lot_ids(stockpile)
        .find(|lot| {
            state.inventory().get_lot(*lot).is_some_and(|record| {
                record.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_CHIP)
            })
        })
        .unwrap_or_else(|| panic!("reform-age output lot disappeared"));
    let exposure_after = state
        .inventory()
        .get_lot(reformed)
        .unwrap_or_else(|| panic!("reform-age output record disappeared"))
        .storage_history()
        .project(state.tick(), preservation)
        .unwrap_or_else(|| panic!("reform-age output exposure overflowed"));

    assert_eq!(exposure_after, exposure_before);
}

#[test]
fn randomized_complete_transaction_sequence_conserves_inventory_quantity() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1A70_2006));
    let a = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(500))
        .unwrap_or_else(|error| panic!("pile a allocation failed: {error}"));
    let b = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(500))
        .unwrap_or_else(|error| panic!("pile b allocation failed: {error}"));
    let c = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(500))
        .unwrap_or_else(|error| panic!("pile c allocation failed: {error}"));
    for (pile, amount) in [(a, 100), (b, 60), (c, 40)] {
        deposit_bulk_for_test(
            &registries,
            &mut state,
            pile,
            wood_log(),
            Mass::from_milligrams(amount),
        )
        .unwrap_or_else(|error| panic!("seed deposit failed: {error}"));
    }
    let initial = stored_aggregate_total(&state);
    assert_eq!(initial, Mass::from_milligrams(200));

    let mut seed = 0xD00D_2026u64;
    for step in 1..=400 {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        let choice = (seed >> 32) % 3;
        let source = [a, b, c][((seed >> 24) % 3) as usize];
        let destination = [a, b, c][((seed >> 16) % 3) as usize];
        let requested = Mass::from_milligrams(1 + ((seed >> 8) % 20));
        let mut moved = false;

        if source == destination {
            continue;
        }

        match choice {
            0 => {
                if let Ok(validated) = validate_material_transfer_for_test(
                    &registries,
                    &state,
                    source,
                    destination,
                    wood_log(),
                    requested,
                ) {
                    validated
                        .commit(&mut state)
                        .unwrap_or_else(|error| panic!("random transfer commit failed: {error}"));
                    moved = true;
                }
            }
            1 => {
                let inputs = vec![MaterialInputSpec::new(wood_log(), requested)];
                if let Ok(selection) =
                    validate_consumption_selection(state.inventory(), source, &inputs)
                    && let Ok(relocation) = validate_material_relocation_from_selection(
                        &registries,
                        &state,
                        destination,
                        selection,
                    )
                {
                    relocation.commit(&mut state).unwrap_or_else(|error| {
                        panic!("random relocation commit failed: {error:?}")
                    });
                    moved = true;
                }
            }
            2 => {
                let inputs = vec![MaterialInputSpec::new(wood_log(), requested)];
                if let Ok(selection) =
                    validate_consumption_selection(state.inventory(), source, &inputs)
                {
                    let egress =
                        validate_material_egress_from_selection(state.inventory(), selection)
                            .unwrap_or_else(|error| {
                                panic!("random egress validation failed: {error:?}")
                            });
                    let traces = egress.consumed_inputs().to_vec();
                    apply_material_egress(state.inventory_state_mut(), egress);
                    let ingress = validate_material_ingress(
                        &registries,
                        state.inventory(),
                        destination,
                        traces.iter().map(MaterialIngressEntry::from_consumed_trace),
                        state.tick(),
                    )
                    .unwrap_or_else(|error| panic!("random ingress validation failed: {error:?}"));
                    apply_material_ingress(state.inventory_state_mut(), ingress);
                    moved = true;
                }
            }
            _ => unreachable!("three-way randomized transaction choice"),
        }

        if moved || step % 5 == 0 {
            assert_eq!(
                stored_aggregate_total(&state),
                initial,
                "step {step}: complete inventory transaction changed total stored matter"
            );
            assert_lot_aggregate_agreement(&registries, &state, &format!("step {step}"));
        }
    }
    assert_lot_aggregate_agreement(&registries, &state, "randomized sequence end");
}
