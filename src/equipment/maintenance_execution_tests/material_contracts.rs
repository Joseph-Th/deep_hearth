//! Maintenance material selection, atomicity, support-load, and conservation contracts.

use super::*;

#[test]
fn authored_maintenance_resolution_rejects_unneeded_or_understocked_service() {
    let registries = registries();
    let mut state = AppState::new();
    let healthy = add_equipment(&registries, &mut state, TEST_DEFINITION, condition(700_000))
        .unwrap_or_else(|error| panic!("healthy maintenance equipment fixture failed: {error}"));
    let worn = add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000))
        .unwrap_or_else(|error| panic!("worn maintenance equipment fixture failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("maintenance stock fixture failed: {error}"));
    let spent = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("maintenance spent fixture failed: {error}"));
    add_material(&registries, &mut state, source, Mass::from_milligrams(1));

    assert_eq!(
        resolve_equipment_maintenance(
            &registries,
            &state,
            EquipmentMaintenanceRequest::new(healthy, source, spent),
        ),
        Err(
            EquipmentMaintenanceResolutionError::ConditionAtOrAboveServiceTarget {
                equipment: healthy,
                current: condition(700_000),
                target: condition(700_000),
            }
        )
    );
    assert_eq!(
        resolve_equipment_maintenance(
            &registries,
            &state,
            EquipmentMaintenanceRequest::new(worn, source, spent),
        ),
        Err(
            EquipmentMaintenanceResolutionError::InsufficientReplacementMaterial {
                stockpile: source,
                commodity: CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
                available: Mass::from_milligrams(1),
                required: Mass::from_milligrams(2),
            }
        )
    );
}

#[test]
fn maintenance_filters_contaminated_stock_and_rejects_forged_impure_selection() {
    let registries = registries();
    let mut state = AppState::new();
    let equipment = add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000))
        .unwrap_or_else(|error| panic!("impure maintenance equipment fixture failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(27))
        .unwrap_or_else(|error| panic!("impure maintenance source fixture failed: {error}"));
    let spent = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("impure maintenance spent fixture failed: {error}"));
    let composition = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_WOOD, 900_000),
        CompositionComponent::new(MATERIAL_STONE, 100_000),
    ])
    .unwrap_or_else(|error| panic!("impure maintenance composition fixture failed: {error}"));
    let lot = deposit_composed_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(20),
        Temperature::from_millikelvin(300_000),
        composition,
    )
    .unwrap_or_else(|error| panic!("impure maintenance lot fixture failed: {error}"));
    let before = state.clone();
    let replacement = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);

    assert_eq!(
        resolve_equipment_maintenance(
            &registries,
            &state,
            EquipmentMaintenanceRequest::new(equipment, source, spent),
        ),
        Err(
            EquipmentMaintenanceResolutionError::InsufficientReplacementMaterial {
                stockpile: source,
                commodity: replacement,
                available: Mass::ZERO,
                required: Mass::from_milligrams(2),
            }
        )
    );
    assert_eq!(state, before);

    let resolution = forge_maintenance_resolution(
        &state,
        equipment,
        source,
        lot,
        Mass::from_milligrams(7),
        spent,
        condition(700_000),
    );
    assert_eq!(
        validate_equipment_maintenance(&registries, &state, resolution),
        Err(EquipmentMaintenanceError::ImpureReplacementMaterial {
            commodity: replacement,
        })
    );
    assert_eq!(state, before);

    add_material(&registries, &mut state, source, Mass::from_milligrams(2));
    let resolved = resolve_equipment_maintenance(
        &registries,
        &state,
        EquipmentMaintenanceRequest::new(equipment, source, spent),
    )
    .unwrap_or_else(|error| {
        panic!("maintenance should skip contaminated replacement stock: {error}")
    });
    assert_eq!(resolved.material_mass(), Mass::from_milligrams(2));
}

#[test]
fn maintenance_moves_exact_material_to_spent_storage_and_preserves_conservation() {
    let registries = registries();
    let mut state = AppState::new();
    initialize_service_player(&registries, &mut state);
    let equipment =
        match add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000)) {
            Ok(equipment) => equipment,
            Err(error) => panic!("maintenance equipment fixture failed: {error}"),
        };
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("maintenance source fixture failed: {error}"),
    };
    let spent = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("maintenance spent fixture failed: {error}"),
    };
    let lot = add_material(&registries, &mut state, source, Mass::from_milligrams(20));
    let source_lot = match state.inventory().get_lot(lot) {
        Some(record) => record,
        None => panic!("maintenance source lot disappeared"),
    };
    let temperature_before = source_lot.temperature();
    let composition_before = source_lot.composition().clone();
    let particle_size_before = source_lot.particle_size();
    let created_before = source_lot.created_at();
    let latest_before = source_lot.latest_created_at();
    let matter_before = match calculate_matter_accounting(&state) {
        Ok(accounting) => accounting.total(),
        Err(error) => panic!("maintenance initial matter accounting failed: {error}"),
    };
    let energy_before = explicit_energy(&registries, &state);
    let resolution = resolve_equipment_maintenance(
        &registries,
        &state,
        EquipmentMaintenanceRequest::new(equipment, source, spent),
    )
    .unwrap_or_else(|error| panic!("maintenance resolution failed: {error}"));
    let token = match validate_equipment_maintenance(&registries, &state, resolution) {
        Ok(token) => token,
        Err(error) => panic!("maintenance validation failed: {error}"),
    };
    assert_eq!(token.material_mass(), Mass::from_milligrams(2));

    let outcome = match token.commit(&mut state) {
        Ok(outcome) => outcome,
        Err(error) => panic!("maintenance commit failed: {error}"),
    };

    assert_eq!(outcome.condition_before(), condition(500_000));
    assert_eq!(outcome.target_condition(), condition(700_000));
    assert_eq!(outcome.material_mass(), Mass::from_milligrams(2));
    assert_eq!(
        state
            .equipment()
            .get_equipment(equipment)
            .map(|record| record.condition()),
        Some(condition(500_000))
    );
    assert_eq!(
        state.inventory().get_lot(lot).map(|record| record.mass()),
        Some(Mass::from_milligrams(18))
    );
    let spent_lot = match state
        .inventory()
        .lots()
        .find(|record| record.stockpile() == spent)
    {
        Some(record) => record,
        None => panic!("maintenance spent material missing"),
    };
    assert_eq!(spent_lot.mass(), Mass::from_milligrams(2));
    assert_eq!(
        spent_lot.commodity(),
        CommodityKey::new(MATERIAL_WOOD, FORM_CHIP)
    );
    assert_eq!(spent_lot.temperature(), temperature_before);
    assert_eq!(spent_lot.composition(), &composition_before);
    assert_eq!(spent_lot.particle_size(), particle_size_before);
    assert_eq!(spent_lot.created_at(), created_before);
    assert_eq!(spent_lot.latest_created_at(), latest_before);
    assert_eq!(
        calculate_matter_accounting(&state).map(|accounting| accounting.total()),
        Ok(matter_before)
    );
    assert_eq!(explicit_energy(&registries, &state), energy_before);
    let completion = finish_service(&registries, &mut state, outcome.completes_at());
    assert_eq!(completion.condition_after(), condition(700_000));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn maintenance_rejects_non_improvement_and_allows_spent_material_to_return_to_source() {
    let registries = registries();
    let mut state = AppState::new();
    initialize_service_player(&registries, &mut state);
    let equipment =
        match add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000)) {
            Ok(equipment) => equipment,
            Err(error) => panic!("maintenance rejection equipment fixture failed: {error}"),
        };
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("maintenance rejection source fixture failed: {error}"),
    };
    let spent = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("maintenance rejection spent fixture failed: {error}"),
    };
    let lot = add_material(&registries, &mut state, source, Mass::from_milligrams(10));
    let before = state.clone();

    let no_improvement = forge_maintenance_resolution(
        &state,
        equipment,
        source,
        lot,
        Mass::from_milligrams(1),
        spent,
        condition(500_000),
    );
    assert_eq!(
        validate_equipment_maintenance(&registries, &state, no_improvement),
        Err(EquipmentMaintenanceError::ConditionNotImproved {
            equipment,
            before: condition(500_000),
            after: condition(500_000),
        })
    );
    assert_eq!(state, before);

    let same_destination = resolve_equipment_maintenance(
        &registries,
        &state,
        EquipmentMaintenanceRequest::new(equipment, source, source),
    )
    .unwrap_or_else(|error| panic!("same-stockpile maintenance resolution failed: {error}"));
    let outcome = validate_equipment_maintenance(&registries, &state, same_destination)
        .unwrap_or_else(|error| panic!("same-stockpile maintenance validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("same-stockpile maintenance commit failed: {error}"));
    assert_eq!(outcome.material_mass(), Mass::from_milligrams(2));
    assert_eq!(
        state
            .equipment()
            .get_equipment(equipment)
            .map(|record| record.condition()),
        Some(condition(500_000))
    );
    let source_record = state
        .inventory()
        .get_stockpile(source)
        .unwrap_or_else(|| panic!("same-stockpile maintenance source disappeared"));
    assert_eq!(source_record.stored_mass(), Mass::from_milligrams(10));
    assert_eq!(
        source_record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_LOG)),
        Mass::from_milligrams(8)
    );
    assert_eq!(
        source_record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_CHIP)),
        Mass::from_milligrams(2)
    );
    let completion = finish_service(&registries, &mut state, outcome.completes_at());
    assert_eq!(completion.condition_after(), condition(700_000));
}

#[test]
fn maintenance_rechecks_inventory_and_equipment_before_any_partial_commit() {
    let registries = registries();
    let mut state = AppState::new();
    initialize_service_player(&registries, &mut state);
    let equipment =
        match add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000)) {
            Ok(equipment) => equipment,
            Err(error) => panic!("maintenance stale equipment fixture failed: {error}"),
        };
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("maintenance stale source fixture failed: {error}"),
    };
    let spent = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("maintenance stale spent fixture failed: {error}"),
    };
    let lot = add_material(&registries, &mut state, source, Mass::from_milligrams(10));

    let inventory_resolution = resolve_equipment_maintenance(
        &registries,
        &state,
        EquipmentMaintenanceRequest::new(equipment, source, spent),
    )
    .unwrap_or_else(|error| panic!("maintenance stale inventory resolution failed: {error}"));
    let inventory_stale =
        match validate_equipment_maintenance(&registries, &state, inventory_resolution) {
            Ok(token) => token,
            Err(error) => panic!("maintenance stale inventory validation failed: {error}"),
        };
    if let Err(error) = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1)) {
        panic!("maintenance stale inventory mutation failed: {error}");
    }
    let condition_before = state
        .equipment()
        .get_equipment(equipment)
        .map(|record| record.condition());
    assert!(matches!(
        inventory_stale.commit(&mut state),
        Err(EquipmentMaintenanceCommitError::StaleInventoryRevision {
            expected: _expected,
            actual: _actual,
        })
    ));
    assert_eq!(
        state
            .equipment()
            .get_equipment(equipment)
            .map(|record| record.condition()),
        condition_before
    );
    assert_eq!(
        state.inventory().get_lot(lot).map(|record| record.mass()),
        Some(Mass::from_milligrams(10))
    );

    let equipment_resolution = resolve_equipment_maintenance(
        &registries,
        &state,
        EquipmentMaintenanceRequest::new(equipment, source, spent),
    )
    .unwrap_or_else(|error| panic!("maintenance stale equipment resolution failed: {error}"));
    let equipment_stale =
        match validate_equipment_maintenance(&registries, &state, equipment_resolution) {
            Ok(token) => token,
            Err(error) => panic!("maintenance stale equipment validation failed: {error}"),
        };
    degrade_equipment_condition_for_test(&mut state, equipment, 1_000);
    let lot_mass_before = state.inventory().get_lot(lot).map(|record| record.mass());
    assert!(matches!(
        equipment_stale.commit(&mut state),
        Err(EquipmentMaintenanceCommitError::StaleEquipmentRevision {
            expected: _expected,
            actual: _actual,
        })
    ));
    assert_eq!(
        state.inventory().get_lot(lot).map(|record| record.mass()),
        lot_mass_before
    );
}

#[test]
fn maintenance_resolution_is_invalidated_by_equipment_change_before_validation() {
    let registries = registries();
    let mut state = AppState::new();
    let equipment =
        match add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000)) {
            Ok(equipment) => equipment,
            Err(error) => panic!("maintenance stale-resolution equipment fixture failed: {error}"),
        };
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("maintenance stale-resolution source fixture failed: {error}"),
    };
    let spent = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("maintenance stale-resolution spent fixture failed: {error}"),
    };
    add_material(&registries, &mut state, source, Mass::from_milligrams(2));
    let expected_revision = state.equipment().revision();
    let resolution = resolve_equipment_maintenance(
        &registries,
        &state,
        EquipmentMaintenanceRequest::new(equipment, source, spent),
    )
    .unwrap_or_else(|error| panic!("maintenance stale-resolution binding failed: {error}"));
    assert_eq!(resolution.condition_before(), condition(500_000));
    degrade_equipment_condition_for_test(&mut state, equipment, 1_000);
    let actual_revision = state.equipment().revision();
    let inventory_before = state.inventory().clone();

    assert_eq!(
        validate_equipment_maintenance(&registries, &state, resolution),
        Err(EquipmentMaintenanceError::StaleEquipmentResolution {
            equipment,
            expected_revision,
            actual_revision,
        })
    );
    assert_eq!(state.inventory(), &inventory_before);
}

#[test]
fn maintenance_material_relocation_updates_supported_stockpile_loads_atomically() {
    let registries = registries();
    let mut state = AppState::new();
    initialize_service_player(&registries, &mut state);
    let equipment =
        match add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000)) {
            Ok(equipment) => equipment,
            Err(error) => panic!("maintenance support equipment fixture failed: {error}"),
        };
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("maintenance supported source fixture failed: {error}"),
    };
    let spent = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("maintenance supported spent fixture failed: {error}"),
    };
    add_material(&registries, &mut state, source, Mass::from_milligrams(10));
    let source_support = active_support(&registries, &mut state, 0);
    let spent_support = active_support(&registries, &mut state, 2);
    for (stockpile, support) in [(source, source_support), (spent, spent_support)] {
        let token = match validate_mount_stockpile(&registries, &state, stockpile, support) {
            Ok(token) => token,
            Err(error) => panic!("maintenance stockpile mount failed: {error}"),
        };
        if let Err(error) = token.commit(&mut state) {
            panic!("maintenance stockpile mount commit failed: {error}");
        }
    }
    let source_load_before = state
        .structures()
        .get_element(source_support)
        .map(|record| record.load(StructuralLoadKind::StoredMatter))
        .unwrap_or(Force::ZERO);
    assert!(source_load_before > Force::ZERO);
    assert_eq!(
        state
            .structures()
            .get_element(spent_support)
            .map(|record| record.load(StructuralLoadKind::StoredMatter)),
        Some(Force::ZERO)
    );

    let resolution = resolve_equipment_maintenance(
        &registries,
        &state,
        EquipmentMaintenanceRequest::new(equipment, source, spent),
    )
    .unwrap_or_else(|error| panic!("maintenance supported resolution failed: {error}"));
    let token = match validate_equipment_maintenance(&registries, &state, resolution) {
        Ok(token) => token,
        Err(error) => panic!("maintenance supported validation failed: {error}"),
    };
    if let Err(error) = token.commit(&mut state) {
        panic!("maintenance supported commit failed: {error}");
    }

    let expected_source_load = calculate_aggregate_weight_force_ceiling(
        AggregateMass::from_mass(Mass::from_milligrams(8)),
        registries.core().gravity(),
    )
    .unwrap_or_else(|| panic!("maintenance supported source load overflowed"));
    let expected_spent_load = calculate_aggregate_weight_force_ceiling(
        AggregateMass::from_mass(Mass::from_milligrams(2)),
        registries.core().gravity(),
    )
    .unwrap_or_else(|| panic!("maintenance supported spent load overflowed"));
    assert_eq!(
        state
            .structures()
            .get_element(source_support)
            .map(|record| record.load(StructuralLoadKind::StoredMatter)),
        Some(expected_source_load)
    );
    assert_eq!(
        state
            .structures()
            .get_element(spent_support)
            .map(|record| record.load(StructuralLoadKind::StoredMatter)),
        Some(expected_spent_load)
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn maintenance_preserves_multiple_lot_profiles_without_id_collision() {
    let registries = registries();
    let mut state = AppState::new();
    initialize_service_player(&registries, &mut state);
    let equipment =
        match add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000)) {
            Ok(equipment) => equipment,
            Err(error) => panic!("multi-lot maintenance equipment fixture failed: {error}"),
        };
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("multi-lot maintenance source fixture failed: {error}"),
    };
    let spent = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("multi-lot maintenance spent fixture failed: {error}"),
    };
    let first = match deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(1),
        Temperature::from_millikelvin(300_000),
    ) {
        Ok(lot) => lot,
        Err(error) => panic!("multi-lot first fixture failed: {error}"),
    };
    let second = match deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(5),
        Temperature::from_millikelvin(310_000),
    ) {
        Ok(lot) => lot,
        Err(error) => panic!("multi-lot second fixture failed: {error}"),
    };
    let resolution = resolve_equipment_maintenance(
        &registries,
        &state,
        EquipmentMaintenanceRequest::new(equipment, source, spent),
    )
    .unwrap_or_else(|error| panic!("multi-lot maintenance resolution failed: {error}"));
    let token = match validate_equipment_maintenance(&registries, &state, resolution) {
        Ok(token) => token,
        Err(error) => panic!("multi-lot maintenance validation failed: {error}"),
    };
    if let Err(error) = token.commit(&mut state) {
        panic!("multi-lot maintenance commit failed: {error}");
    }

    assert_eq!(state.inventory().get_lot(first).map(|lot| lot.mass()), None);
    assert_eq!(
        state.inventory().get_lot(second).map(|lot| lot.mass()),
        Some(Mass::from_milligrams(4))
    );
    let mut spent_lots: Vec<_> = state
        .inventory()
        .lots()
        .filter(|lot| lot.stockpile() == spent)
        .map(|lot| (lot.id(), lot.mass(), lot.temperature()))
        .collect();
    spent_lots.sort_by_key(|entry| entry.0);
    assert_eq!(spent_lots.len(), 2);
    assert_ne!(spent_lots[0].0, spent_lots[1].0);
    assert_eq!(spent_lots[0].1, Mass::from_milligrams(1));
    assert_eq!(spent_lots[1].1, Mass::from_milligrams(1));
    assert_eq!(
        spent_lots
            .iter()
            .map(|entry| entry.2)
            .collect::<std::collections::BTreeSet<_>>(),
        std::collections::BTreeSet::from([
            Temperature::from_millikelvin(300_000),
            Temperature::from_millikelvin(310_000),
        ])
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn maintenance_spent_capacity_failure_is_atomic() {
    let registries = registries();
    let mut state = AppState::new();
    initialize_service_player(&registries, &mut state);
    let equipment =
        match add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000)) {
            Ok(equipment) => equipment,
            Err(error) => panic!("maintenance capacity equipment fixture failed: {error}"),
        };
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("maintenance capacity source fixture failed: {error}"),
    };
    let spent = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("maintenance capacity spent fixture failed: {error}"),
    };
    add_material(&registries, &mut state, source, Mass::from_milligrams(10));
    let resolution = resolve_equipment_maintenance(
        &registries,
        &state,
        EquipmentMaintenanceRequest::new(equipment, source, spent),
    )
    .unwrap_or_else(|error| panic!("maintenance capacity resolution failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_equipment_maintenance(&registries, &state, resolution),
        Err(EquipmentMaintenanceError::Material(
            EquipmentMaintenanceMaterialError::SpentCapacityExceeded {
                stockpile: spent,
                capacity: Mass::from_milligrams(1),
                committed: Mass::ZERO,
                requested: Mass::from_milligrams(2),
            }
        ))
    );
    assert_eq!(state, before);
}
