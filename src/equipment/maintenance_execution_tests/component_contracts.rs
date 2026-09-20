//! Component-replacement maintenance, embodied-trace, and recovery contracts.

use super::*;

#[test]
fn every_builtin_material_backed_component_service_executes_from_its_real_assembly_traces() {
    let registries = build_registries();
    for (case, definition) in registries
        .equipment()
        .definitions()
        .filter(|definition| {
            definition.assembly_profile().is_some()
                && definition
                    .maintenance_profile()
                    .is_some_and(|maintenance| maintenance.is_component_replacement())
        })
        .enumerate()
    {
        let definition_id = definition.id();
        let assembly_profile = definition.assembly_profile().unwrap_or_else(|| {
            panic!(
                "component service definition {} lost assembly profile",
                definition_id.value()
            )
        });
        let maintenance = definition.maintenance_profile().unwrap_or_else(|| {
            panic!(
                "component service definition {} lost maintenance profile",
                definition_id.value()
            )
        });
        assert!(maintenance.is_component_replacement());

        let mut state = AppState::new(WorldSeed::new(
            0x8120_C100_u64
                .checked_add(
                    u64::try_from(case).unwrap_or_else(|_| unreachable!("bounded case fits u64")),
                )
                .unwrap_or_else(|| unreachable!("bounded component service seed cannot overflow")),
        ));
        initialize_service_player(&registries, &mut state);
        let assembly = add_solid_stockpile_for_test(&mut state, definition.mass())
            .unwrap_or_else(|error| panic!("component service assembly stockpile failed: {error}"));
        for input in assembly_profile.inputs() {
            deposit_lot_for_test(
                &registries,
                &mut state,
                assembly,
                input.commodity(),
                input.mass(),
                Temperature::from_millikelvin(300_000),
            )
            .unwrap_or_else(|error| panic!("component service assembly input failed: {error}"));
        }
        let equipment = validate_assemble_equipment(&registries, &state, definition_id, assembly)
            .unwrap_or_else(|error| panic!("component service assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("component service assembly commit failed: {error}"));
        degrade_equipment_condition_for_test(&mut state, equipment, 100_000);
        let unrelated_embodied_before = state
            .equipment()
            .get_equipment(equipment)
            .unwrap_or_else(|| panic!("component service equipment disappeared before service"))
            .embodied_material()
            .iter()
            .filter(|trace| trace.profile().commodity() != maintenance.replacement())
            .cloned()
            .collect::<Vec<_>>();

        let replacement_mass = maintenance.full_service_replacement_mass();
        let replacement = add_solid_stockpile_for_test(&mut state, replacement_mass)
            .unwrap_or_else(|error| {
                panic!("component service replacement stockpile failed: {error}")
            });
        deposit_lot_for_test(
            &registries,
            &mut state,
            replacement,
            maintenance.replacement(),
            replacement_mass,
            Temperature::from_millikelvin(310_000),
        )
        .unwrap_or_else(|error| panic!("component service replacement input failed: {error}"));
        let spent = add_solid_stockpile_for_test(&mut state, replacement_mass)
            .unwrap_or_else(|error| panic!("component service spent stockpile failed: {error}"));
        let matter_before = calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("component service matter audit failed: {error}"))
            .total();
        let energy_before = explicit_energy(&registries, &state);

        let resolution = resolve_equipment_maintenance(
            &registries,
            &state,
            EquipmentMaintenanceRequest::new(equipment, replacement, spent),
        )
        .unwrap_or_else(|error| panic!("component service resolution failed: {error}"));
        assert!(resolution.replaces_embodied_component());
        assert_eq!(resolution.material_mass(), replacement_mass);
        let outcome = validate_equipment_maintenance(&registries, &state, resolution)
            .unwrap_or_else(|error| panic!("component service validation failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("component service commit failed: {error}"));
        assert_eq!(outcome.equipment(), equipment);
        assert_eq!(outcome.target_condition(), maintenance.restored_condition());

        let record = state
            .equipment()
            .get_equipment(equipment)
            .unwrap_or_else(|| panic!("component serviced equipment disappeared"));
        assert_eq!(record.definition(), definition_id);
        assert_eq!(record.embodied_mass(), definition.mass());
        assert_eq!(record.condition(), outcome.condition_before());
        let unrelated_embodied_after = record
            .embodied_material()
            .iter()
            .filter(|trace| trace.profile().commodity() != maintenance.replacement())
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            unrelated_embodied_after, unrelated_embodied_before,
            "component service must preserve every unrelated embodied trace exactly"
        );
        let fresh_component_mass = record
            .embodied_material()
            .iter()
            .filter(|trace| trace.profile().commodity() == maintenance.replacement())
            .map(|trace| trace.mass())
            .try_fold(Mass::ZERO, |total, mass| total.checked_add(mass))
            .unwrap_or_else(|| panic!("component service replacement trace mass overflowed"));
        assert_eq!(fresh_component_mass, replacement_mass);
        assert_eq!(
            state
                .inventory()
                .get_stockpile(spent)
                .map(|stockpile| stockpile.get_mass(maintenance.spent())),
            Some(replacement_mass),
            "component service must emit exactly the replaced component mass as spent material"
        );
        assert_eq!(
            calculate_matter_accounting(&state)
                .unwrap_or_else(|error| panic!(
                    "component service final matter audit failed: {error}"
                ))
                .total(),
            matter_before
        );
        assert_eq!(explicit_energy(&registries, &state), energy_before);
        let completion = finish_service(&registries, &mut state, outcome.completes_at());
        assert_eq!(completion.equipment(), equipment);
        assert_eq!(completion.condition_before(), outcome.condition_before());
        assert_eq!(
            completion.condition_after(),
            maintenance.restored_condition()
        );
        assert_eq!(
            state
                .equipment()
                .get_equipment(equipment)
                .map(|record| record.condition()),
            Some(maintenance.restored_condition())
        );
        validate_loaded_state(&registries, &state)
            .unwrap_or_else(|error| panic!("component service state audit failed: {error}"));
    }
}

#[test]
fn accumulated_maintenance_stone_scrap_can_reknap_the_next_pick_component() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x8120_C102));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("stone scrap maintenance survival setup failed: {error}"));

    let assembly = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| {
            panic!("stone scrap maintenance assembly stockpile failed: {error}")
        });
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(800_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(200_000),
        ),
    ] {
        deposit_lot_for_test(
            &registries,
            &mut state,
            assembly,
            commodity,
            mass,
            Temperature::from_millikelvin(300_000),
        )
        .unwrap_or_else(|error| {
            panic!("stone scrap maintenance assembly material failed: {error}")
        });
    }
    let pick = validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_PICK, assembly)
        .unwrap_or_else(|error| panic!("stone scrap maintenance pick assembly failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("stone scrap maintenance pick assembly commit failed: {error}")
        });

    let fresh_replacements =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_600_000))
            .unwrap_or_else(|error| panic!("fresh stone replacement stockpile failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        fresh_replacements,
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
        Mass::from_milligrams(1_600_000),
        Temperature::from_millikelvin(300_000),
    )
    .unwrap_or_else(|error| panic!("fresh stone replacement fixture failed: {error}"));
    let spent = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(3_000_000))
        .unwrap_or_else(|error| panic!("stone scrap maintenance spent stockpile failed: {error}"));
    let recovered = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("stone scrap reknap output stockpile failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("stone scrap maintenance matter-before failed: {error}"))
        .total();

    for service in 0..2 {
        degrade_equipment_condition_for_test(&mut state, pick, 100_000);
        let resolution = resolve_equipment_maintenance(
            &registries,
            &state,
            EquipmentMaintenanceRequest::new(pick, fresh_replacements, spent),
        )
        .unwrap_or_else(|error| panic!("fresh stone service {service} failed: {error}"));
        assert_eq!(resolution.material_mass(), Mass::from_milligrams(800_000));
        let outcome = validate_equipment_maintenance(&registries, &state, resolution)
            .unwrap_or_else(|error| {
                panic!("fresh stone service validation {service} failed: {error}")
            })
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("fresh stone service commit {service} failed: {error}"));
        assert_eq!(outcome.equipment(), pick);
        assert_eq!(outcome.material_mass(), Mass::from_milligrams(800_000));
        assert_eq!(outcome.target_condition(), condition(1_000_000));
        let completion = finish_service(&registries, &mut state, outcome.completes_at());
        assert_eq!(completion.condition_after(), condition(1_000_000));
    }
    assert_eq!(
        state
            .inventory()
            .get_stockpile(spent)
            .map(|stockpile| { stockpile.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_SCRAP)) }),
        Some(Mass::from_milligrams(1_600_000))
    );
    let scrap = state
        .inventory()
        .lot_ids(spent)
        .find(|lot| {
            state.inventory().get_lot(*lot).is_some_and(|record| {
                record.commodity() == CommodityKey::new(MATERIAL_STONE, FORM_SCRAP)
                    && record.mass() >= Mass::from_milligrams(1_000_000)
            })
        })
        .unwrap_or_else(|| panic!("maintenance stone scrap lot disappeared"));

    let reknap_job = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_REKNAP_STONE_SCRAP_TOOL,
            spent,
            MaterialLotSelection::new(scrap, Mass::from_milligrams(1_000_000)),
            recovered,
        ),
    )
    .unwrap_or_else(|error| panic!("maintenance scrap reknap start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("maintenance scrap reknap commit failed: {error}"));
    while state.production().get_job(reknap_job).is_some() {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("maintenance scrap reknap tick failed: {error}"));
    }
    assert_eq!(
        state
            .inventory()
            .get_stockpile(spent)
            .map(|stockpile| { stockpile.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_SCRAP)) }),
        Some(Mass::from_milligrams(600_000))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(recovered)
            .map(|stockpile| { stockpile.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_TOOL)) }),
        Some(Mass::from_milligrams(800_000))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(recovered)
            .map(|stockpile| { stockpile.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_CHIP)) }),
        Some(Mass::from_milligrams(200_000))
    );

    degrade_equipment_condition_for_test(&mut state, pick, 100_000);
    let recycled_resolution = resolve_equipment_maintenance(
        &registries,
        &state,
        EquipmentMaintenanceRequest::new(pick, recovered, spent),
    )
    .unwrap_or_else(|error| panic!("reknapped component service failed: {error}"));
    assert_eq!(
        recycled_resolution.material_mass(),
        Mass::from_milligrams(800_000)
    );
    let recycled_outcome = validate_equipment_maintenance(&registries, &state, recycled_resolution)
        .unwrap_or_else(|error| panic!("reknapped component service validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("reknapped component service commit failed: {error}"));
    assert_eq!(recycled_outcome.equipment(), pick);
    assert_eq!(
        recycled_outcome.material_mass(),
        Mass::from_milligrams(800_000)
    );
    assert_eq!(recycled_outcome.target_condition(), condition(1_000_000));
    let recycled_completion =
        finish_service(&registries, &mut state, recycled_outcome.completes_at());
    assert_eq!(recycled_completion.condition_after(), condition(1_000_000));

    assert_eq!(
        state
            .equipment()
            .get_equipment(pick)
            .map(|record| record.condition()),
        Some(condition(1_000_000))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(recovered)
            .map(|stockpile| { stockpile.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_TOOL)) }),
        Some(Mass::ZERO)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(recovered)
            .map(|stockpile| { stockpile.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_CHIP)) }),
        Some(Mass::from_milligrams(200_000))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(spent)
            .map(|stockpile| { stockpile.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_SCRAP)) }),
        Some(Mass::from_milligrams(1_400_000))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(fresh_replacements)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO),
        "the third service must use recovered stone rather than hidden fresh replacement stock"
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("stone scrap maintenance matter-after failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn component_maintenance_preserves_upgrade_and_exchanges_exact_embodied_trace() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x8120_C001));
    initialize_service_player(&registries, &mut state);
    let assembly = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("component service assembly stockpile failed: {error}"));
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(800_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(200_000),
        ),
    ] {
        deposit_lot_for_test(
            &registries,
            &mut state,
            assembly,
            commodity,
            mass,
            Temperature::from_millikelvin(300_000),
        )
        .unwrap_or_else(|error| panic!("component service assembly material failed: {error}"));
    }
    let pick = validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_PICK, assembly)
        .unwrap_or_else(|error| panic!("component service pick assembly failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("component service pick assembly commit failed: {error}"));

    let reinforcement = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20_000))
        .unwrap_or_else(|error| {
            panic!("component service reinforcement stockpile failed: {error}")
        });
    deposit_lot_for_test(
        &registries,
        &mut state,
        reinforcement,
        CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
        Mass::from_milligrams(20_000),
        Temperature::from_millikelvin(300_000),
    )
    .unwrap_or_else(|error| panic!("component service reinforcement material failed: {error}"));
    assert_eq!(
        validate_upgrade_equipment(
            &registries,
            &state,
            pick,
            EQUIPMENT_COPPER_REINFORCED_PICK,
            reinforcement,
        )
        .unwrap_or_else(|error| panic!("component service upgrade failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("component service upgrade commit failed: {error}")),
        pick
    );
    degrade_equipment_condition_for_test(&mut state, pick, 400_000);
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("component service provenance tick failed: {error}"));

    let old_stone_trace = state
        .equipment()
        .get_equipment(pick)
        .unwrap_or_else(|| panic!("component service upgraded pick disappeared"))
        .embodied_material()
        .iter()
        .find(|trace| trace.profile().commodity() == CommodityKey::new(MATERIAL_STONE, FORM_TOOL))
        .cloned()
        .unwrap_or_else(|| panic!("component service old stone component disappeared"));
    let replacement = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(800_000))
        .unwrap_or_else(|error| panic!("component service replacement stockpile failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        replacement,
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
        Mass::from_milligrams(800_000),
        Temperature::from_millikelvin(310_000),
    )
    .unwrap_or_else(|error| panic!("component service fresh component failed: {error}"));
    let spent = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(800_000))
        .unwrap_or_else(|error| panic!("component service spent stockpile failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("component service matter before failed: {error}"))
        .total();
    let energy_before = explicit_energy(&registries, &state);

    let resolution = resolve_equipment_maintenance(
        &registries,
        &state,
        EquipmentMaintenanceRequest::new(pick, replacement, spent),
    )
    .unwrap_or_else(|error| panic!("component service resolution failed: {error}"));
    assert!(resolution.replaces_embodied_component());
    assert_eq!(resolution.material_mass(), Mass::from_milligrams(800_000));
    let outcome = validate_equipment_maintenance(&registries, &state, resolution)
        .unwrap_or_else(|error| panic!("component service validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("component service commit failed: {error}"));
    assert_eq!(outcome.equipment(), pick);
    assert_eq!(outcome.target_condition(), Condition::PRISTINE);

    let record = state
        .equipment()
        .get_equipment(pick)
        .unwrap_or_else(|| panic!("component service pick disappeared after repair"));
    assert_eq!(record.definition(), EQUIPMENT_COPPER_REINFORCED_PICK);
    assert_eq!(record.condition(), outcome.condition_before());
    assert_eq!(record.embodied_mass(), Mass::from_milligrams(1_020_000));
    assert!(record.embodied_material().iter().any(|trace| {
        trace.profile().commodity() == CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT)
            && trace.mass() == Mass::from_milligrams(20_000)
    }));
    let fresh_stone = record
        .embodied_material()
        .iter()
        .find(|trace| trace.profile().commodity() == CommodityKey::new(MATERIAL_STONE, FORM_TOOL))
        .unwrap_or_else(|| panic!("component service fresh stone trace disappeared"));
    assert_eq!(fresh_stone.mass(), Mass::from_milligrams(800_000));
    assert_eq!(
        fresh_stone.provenance().latest_created_at(),
        SimulationTick::new(1),
        "replacement component must remain distinguishably newer than the original equipment assembly"
    );
    assert_eq!(
        fresh_stone.profile().temperature(),
        Temperature::from_millikelvin(310_000)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(spent)
            .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_SCRAP))),
        Some(Mass::from_milligrams(800_000))
    );
    let spent_trace = state
        .inventory()
        .lots()
        .find(|lot| lot.stockpile() == spent)
        .unwrap_or_else(|| panic!("component service spent lot disappeared"));
    assert_eq!(
        spent_trace.temperature(),
        old_stone_trace.profile().temperature()
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("component service matter after failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(explicit_energy(&registries, &state), energy_before);
    let completion = finish_service(&registries, &mut state, outcome.completes_at());
    assert_eq!(completion.condition_after(), Condition::PRISTINE);
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("component service final state audit failed: {error}"));
    let recovery = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_020_000))
        .unwrap_or_else(|error| panic!("component service recovery stockpile failed: {error}"));
    let encoded =
        serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("component service persistence serialization failed: {error}")
        });
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("component service persistence decode failed: {error}"));
    let loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("component service trusted reload failed: {error}"));
    assert_eq!(
        loaded, state,
        "component service must persist a newer replacement trace inside the older equipment identity"
    );

    let expected_recovery = validate_disassemble_equipment(&registries, &state, pick, recovery)
        .unwrap_or_else(|error| panic!("serviced pick disassembly validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("serviced pick disassembly commit failed: {error}"));
    let mut loaded = loaded;
    let actual_recovery = validate_disassemble_equipment(&registries, &loaded, pick, recovery)
        .unwrap_or_else(|error| {
            panic!("loaded serviced pick disassembly validation failed: {error}")
        })
        .commit(&mut loaded)
        .unwrap_or_else(|error| panic!("loaded serviced pick disassembly commit failed: {error}"));
    assert_eq!(actual_recovery, expected_recovery);
    assert_eq!(loaded, state);
    let recovered = state
        .inventory()
        .get_stockpile(recovery)
        .unwrap_or_else(|| panic!("component service recovery stockpile disappeared"));
    assert_eq!(
        recovered.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_TOOL)),
        Mass::from_milligrams(800_000),
        "completed component service must make the fresh replacement component exactly recoverable"
    );
    assert_eq!(
        recovered.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE)),
        Mass::from_milligrams(200_000)
    );
    assert_eq!(
        recovered.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT)),
        Mass::from_milligrams(20_000)
    );
    assert_eq!(
        recovered.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_SCRAP)),
        Mass::ZERO,
        "a completed service must not leave the replacement component marked as worn"
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!(
                "serviced pick disassembly matter audit failed: {error}"
            ))
            .total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
