//! Exclusive player attention and cross-owner equipment, energy, and intake conflicts.

use super::*;

#[test]
fn raised_weapon_blocks_unrelated_tool_and_energy_dismantling_without_losing_matter() {
    use crate::content::{
        ENERGY_STONE_FLYWHEEL_DRIVE, EQUIPMENT_STONE_PICK, EQUIPMENT_WOODEN_CLUB,
    };
    use crate::core::quantity::Temperature;
    use crate::energy::{
        EnergyStoreDisassemblyCommitError, EnergyStoreDisassemblyError,
        validate_assemble_energy_store, validate_disassemble_energy_store,
    };
    use crate::equipment::{
        EquipmentDisassemblyCommitError, EquipmentDisassemblyError, validate_assemble_equipment,
        validate_disassemble_equipment,
    };
    use crate::inventory::deposit_lot_for_test;
    use crate::matter::calculate_matter_accounting;

    let registries = build_registries();
    let mut state = AppState::new();
    let carried = validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    arm_player(&registries, &mut state, EQUIPMENT_WOODEN_CLUB);
    let temperature = Temperature::from_millikelvin(293_150);
    let tool_inputs = registries
        .equipment()
        .get_equipment(EQUIPMENT_STONE_PICK)
        .unwrap()
        .assembly_profile()
        .unwrap()
        .inputs()
        .to_vec();
    for input in &tool_inputs {
        deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            input.commodity(),
            input.mass(),
            temperature,
        )
        .unwrap();
    }
    let tool = validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_PICK, carried)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let store_inputs = registries
        .energy()
        .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
        .unwrap()
        .assembly_profile()
        .unwrap()
        .inputs()
        .to_vec();
    for input in &store_inputs {
        deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            input.commodity(),
            input.mass(),
            temperature,
        )
        .unwrap();
    }
    let store =
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_FLYWHEEL_DRIVE, carried)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    let tool_token = validate_disassemble_equipment(&registries, &state, tool, carried).unwrap();
    let store_token =
        validate_disassemble_energy_store(&registries, &state, store, carried).unwrap();
    let expected = state.combat().revision();
    let _ = validate_prepare_primitive_strike(&registries, &state, east())
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let actual = state.combat().revision();
    let unchanged = state.clone();
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    assert_eq!(
        validate_disassemble_equipment(&registries, &state, tool, carried).err(),
        Some(EquipmentDisassemblyError::PreparedStrikeActive)
    );
    assert_eq!(
        validate_disassemble_energy_store(&registries, &state, store, carried).err(),
        Some(EnergyStoreDisassemblyError::PreparedStrikeActive)
    );
    assert!(matches!(
        tool_token.commit(&mut state),
        Err(EquipmentDisassemblyCommitError::StaleCombatRevision {
            expected: e, actual: a
        }) if e == expected && a == actual
    ));
    assert!(matches!(
        store_token.commit(&mut state),
        Err(EnergyStoreDisassemblyCommitError::StaleCombatRevision {
            expected: e, actual: a
        }) if e == expected && a == actual
    ));
    assert_eq!(state, unchanged);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for game in [&mut state, &mut replay] {
        let _ = validate_cancel_primitive_preparation(game)
            .unwrap()
            .commit(game)
            .unwrap();
        assert!(
            !validate_disassemble_equipment(&registries, game, tool, carried)
                .unwrap()
                .commit(game)
                .unwrap()
                .recovered_lots()
                .is_empty()
        );
        assert!(
            !validate_disassemble_energy_store(&registries, game, store, carried)
                .unwrap()
                .commit(game)
                .unwrap()
                .recovered_lots()
                .is_empty()
        );
        assert_eq!(
            calculate_matter_accounting(game).unwrap().total(),
            conserved
        );
        assert_eq!(validate_loaded_state(&registries, game), Ok(()));
    }
    assert_eq!(replay, state);
}

#[test]
fn raised_weapon_blocks_equipment_and_energy_upgrades_without_spending_reinforcement() {
    use crate::content::{
        ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE, ENERGY_STONE_FLYWHEEL_DRIVE,
        EQUIPMENT_COPPER_REINFORCED_PICK, EQUIPMENT_STONE_PICK, EQUIPMENT_WOODEN_CLUB,
    };
    use crate::core::quantity::Temperature;
    use crate::energy::{
        EnergyStoreUpgradeCommitError, EnergyStoreUpgradeError, validate_assemble_energy_store,
        validate_upgrade_energy_store,
    };
    use crate::equipment::{
        EquipmentUpgradeCommitError, EquipmentUpgradeError, validate_assemble_equipment,
        validate_upgrade_equipment,
    };
    use crate::inventory::deposit_lot_for_test;
    use crate::matter::calculate_matter_accounting;

    let registries = build_registries();
    let mut state = AppState::new();
    let carried = validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    let _club = arm_player(&registries, &mut state, EQUIPMENT_WOODEN_CLUB);
    let temperature = Temperature::from_millikelvin(293_150);
    let tool_inputs = registries
        .equipment()
        .get_equipment(EQUIPMENT_STONE_PICK)
        .unwrap()
        .assembly_profile()
        .unwrap()
        .inputs()
        .to_vec();
    for input in &tool_inputs {
        deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            input.commodity(),
            input.mass(),
            temperature,
        )
        .unwrap();
    }
    let tool = validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_PICK, carried)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let store_inputs = registries
        .energy()
        .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
        .unwrap()
        .assembly_profile()
        .unwrap()
        .inputs()
        .to_vec();
    for input in &store_inputs {
        deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            input.commodity(),
            input.mass(),
            temperature,
        )
        .unwrap();
    }
    let store =
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_FLYWHEEL_DRIVE, carried)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    let tool_additions = registries
        .equipment()
        .get_equipment(EQUIPMENT_COPPER_REINFORCED_PICK)
        .unwrap()
        .upgrade_profile()
        .unwrap()
        .additions()
        .inputs()
        .to_vec();
    let store_additions = registries
        .energy()
        .get_store(ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE)
        .unwrap()
        .upgrade_profile()
        .unwrap()
        .additions()
        .inputs()
        .to_vec();
    for input in tool_additions.iter().chain(store_additions.iter()) {
        deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            input.commodity(),
            input.mass(),
            temperature,
        )
        .unwrap();
    }
    let tool_token = validate_upgrade_equipment(
        &registries,
        &state,
        tool,
        EQUIPMENT_COPPER_REINFORCED_PICK,
        carried,
    )
    .unwrap();
    let store_token = validate_upgrade_energy_store(
        &registries,
        &state,
        store,
        ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE,
        carried,
    )
    .unwrap();
    let expected = state.combat().revision();
    let _ = validate_prepare_primitive_strike(&registries, &state, east())
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let actual = state.combat().revision();
    let unchanged = state.clone();
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    assert_eq!(
        validate_upgrade_equipment(
            &registries,
            &state,
            tool,
            EQUIPMENT_COPPER_REINFORCED_PICK,
            carried,
        )
        .err(),
        Some(EquipmentUpgradeError::PreparedStrikeActive)
    );
    assert_eq!(
        validate_upgrade_energy_store(
            &registries,
            &state,
            store,
            ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE,
            carried,
        )
        .err(),
        Some(EnergyStoreUpgradeError::PreparedStrikeActive)
    );
    assert_eq!(
        tool_token.commit(&mut state),
        Err(EquipmentUpgradeCommitError::StaleCombatRevision { expected, actual })
    );
    assert_eq!(
        store_token.commit(&mut state),
        Err(EnergyStoreUpgradeCommitError::StaleCombatRevision { expected, actual })
    );
    assert_eq!(state, unchanged);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for game in [&mut state, &mut replay] {
        let _ = validate_cancel_primitive_preparation(game)
            .unwrap()
            .commit(game)
            .unwrap();
        assert_eq!(
            validate_upgrade_equipment(
                &registries,
                game,
                tool,
                EQUIPMENT_COPPER_REINFORCED_PICK,
                carried,
            )
            .unwrap()
            .commit(game),
            Ok(tool)
        );
        assert_eq!(
            validate_upgrade_energy_store(
                &registries,
                game,
                store,
                ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE,
                carried,
            )
            .unwrap()
            .commit(game),
            Ok(store)
        );
        assert_eq!(
            calculate_matter_accounting(game).unwrap().total(),
            conserved
        );
        assert_eq!(validate_loaded_state(&registries, game), Ok(()));
    }
    assert_eq!(replay, state);
}

#[test]
fn raised_weapon_blocks_material_backed_tool_and_energy_store_assembly() {
    use crate::content::{ENERGY_STONE_RING_CAMPFIRE, EQUIPMENT_STONE_PICK, EQUIPMENT_WOODEN_CLUB};
    use crate::core::quantity::Temperature;
    use crate::energy::{
        EnergyStoreAssemblyCommitError, EnergyStoreAssemblyError, validate_assemble_energy_store,
    };
    use crate::equipment::{
        EquipmentAssemblyCommitError, EquipmentAssemblyError, validate_assemble_equipment,
    };
    use crate::inventory::deposit_lot_for_test;
    use crate::matter::calculate_matter_accounting;

    let registries = build_registries();
    let mut state = AppState::new();
    let carried = validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    let club = arm_player(&registries, &mut state, EQUIPMENT_WOODEN_CLUB);
    let tool_assembly = registries
        .equipment()
        .get_equipment(EQUIPMENT_STONE_PICK)
        .unwrap()
        .assembly_profile()
        .unwrap();
    for input in tool_assembly.inputs() {
        let _ = deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            input.commodity(),
            input.mass(),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap();
    }
    let campfire_assembly = registries
        .energy()
        .get_store(ENERGY_STONE_RING_CAMPFIRE)
        .unwrap()
        .assembly_profile()
        .unwrap();
    for input in campfire_assembly.inputs() {
        let _ = deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            input.commodity(),
            input.mass(),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap();
    }
    let tool_token =
        validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_PICK, carried).unwrap();
    let store_token =
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_RING_CAMPFIRE, carried)
            .unwrap();
    let previous = state.combat().revision();
    let _ = validate_prepare_primitive_strike(&registries, &state, east())
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let actual = state.combat().revision();
    let before = state.clone();
    let total = calculate_matter_accounting(&state).unwrap().total();
    assert_eq!(state.combat().prepared.map(|p| p.weapon), Some(club));
    assert_eq!(
        validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_PICK, carried).err(),
        Some(EquipmentAssemblyError::PreparedStrikeActive)
    );
    assert_eq!(
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_RING_CAMPFIRE, carried)
            .err(),
        Some(EnergyStoreAssemblyError::PreparedStrikeActive)
    );
    assert_eq!(
        tool_token.commit(&mut state),
        Err(EquipmentAssemblyCommitError::StaleCombatRevision {
            expected: previous,
            actual,
        })
    );
    assert_eq!(
        store_token.commit(&mut state),
        Err(EnergyStoreAssemblyCommitError::StaleCombatRevision {
            expected: previous,
            actual,
        })
    );
    assert_eq!(state, before);
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), total);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for game in [&mut state, &mut replay] {
        let _ = validate_cancel_primitive_preparation(game)
            .unwrap()
            .commit(game)
            .unwrap();
        let _ = validate_assemble_equipment(&registries, game, EQUIPMENT_STONE_PICK, carried)
            .unwrap()
            .commit(game)
            .unwrap();
        let _ =
            validate_assemble_energy_store(&registries, game, ENERGY_STONE_RING_CAMPFIRE, carried)
                .unwrap()
                .commit(game)
                .unwrap();
        assert_eq!(calculate_matter_accounting(game).unwrap().total(), total);
        assert_eq!(validate_loaded_state(&registries, game), Ok(()));
    }
    assert_eq!(replay, state);
}

#[test]
fn prepared_melee_stance_excludes_timed_work_and_stales_prior_work_admission() {
    use crate::content::EQUIPMENT_WOODEN_CLUB;
    use crate::core::time::TickSpan;
    use crate::labor::{PlayerWork, PlayerWorkCommitError, PlayerWorkStartError};
    use crate::production::ProductionJobId;
    use crate::survival::SurvivalExertion;

    let registries = build_registries();
    let mut state = AppState::new();
    validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let club = arm_player(&registries, &mut state, EQUIPMENT_WOODEN_CLUB);
    let work = PlayerWork::ManualProduction {
        job: ProductionJobId::new(1),
    };
    let authorized_before_preparation = crate::labor::validate_player_work_start(
        &registries,
        &state,
        work,
        TickSpan::new(1),
        SurvivalExertion::REST,
    )
    .unwrap();
    let before_revision = state.combat().revision();
    let _ = validate_prepare_primitive_strike(&registries, &state, east())
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let after_revision = state.combat().revision();
    let held_state = state.clone();

    assert_eq!(
        authorized_before_preparation.precheck(&state),
        Err(PlayerWorkCommitError::StaleCombatRevision {
            expected: before_revision,
            actual: after_revision,
        })
    );
    assert_eq!(
        crate::labor::validate_player_work_start(
            &registries,
            &state,
            work,
            TickSpan::new(1),
            SurvivalExertion::REST,
        )
        .err(),
        Some(PlayerWorkStartError::PreparedStrikeActive)
    );
    assert_eq!(state, held_state);
    assert_eq!(state.combat().prepared.map(|p| p.weapon), Some(club));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    // Forge the impossible cross-owner combination without passing through
    // work admission, then check the trusted-load combat reference boundary.
    let mut impossible = state.clone();
    let work_revision = impossible.player_work().revision();
    impossible
        .player_work_state_mut()
        .apply_start(work_revision, work_revision + 1, work);
    assert_eq!(
        preparation::validate_loaded_combat_references(registries.equipment(), &impossible),
        Err("prepared strike cannot coexist with exclusive player work")
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(loaded, state);

    let _ = validate_cancel_primitive_preparation(&state)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert!(
        crate::labor::validate_player_work_start(
            &registries,
            &state,
            work,
            TickSpan::new(1),
            SurvivalExertion::REST,
        )
        .is_ok()
    );
}

#[test]
fn prepared_melee_stance_excludes_meals_and_drinks_and_stales_both_intakes() {
    use crate::content::{EQUIPMENT_WOODEN_CLUB, FLUID_WATER, FORM_FOOD, MATERIAL_BERRIES};
    use crate::core::quantity::{Mass, Temperature, Volume};
    use crate::fluid::{
        add_fluid_store_with_contents_for_fixture, calculate_fluid_volume_accounting,
    };
    use crate::inventory::{MaterialLotSelection, deposit_lot_for_test};
    use crate::logistics::validate_place_fluid_store;
    use crate::material::CommodityKey;
    use crate::matter::calculate_matter_accounting;
    use crate::survival::{
        DrinkCommitError, DrinkError, EatCommitError, EatError, validate_drink, validate_eat,
    };

    let registries = build_registries();
    let mut state = AppState::new();
    let here = VoxelCoord::new(0, 0, 0);
    validate_admit_player(&registries, &state, here)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let _club = arm_player(&registries, &mut state, EQUIPMENT_WOODEN_CLUB);
    let carried = state.logistics().player().unwrap().carried_stockpile();
    let meal_mass = Mass::from_milligrams(150_000);
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        meal_mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let selections = [MaterialLotSelection::new(lot, meal_mass)];
    let serving = Volume::from_microliters(750_000);
    let water = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        serving,
        FLUID_WATER,
        serving,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    validate_place_fluid_store(&state, water, here)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let meal = validate_eat(&registries, &state, carried, &selections).unwrap();
    let drink = validate_drink(&registries, &state, water, serving).unwrap();
    let combat_before = state.combat().revision();
    let preparation = validate_prepare_primitive_strike(&registries, &state, east())
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert!(matches!(
        preparation,
        PrimitivePreparationOutcome::Prepared { .. }
    ));
    let combat_after = state.combat().revision();
    let matter_before = calculate_matter_accounting(&state).unwrap();
    let fluid_before = calculate_fluid_volume_accounting(&state).unwrap();
    let prepared_state = state.clone();

    assert_eq!(
        validate_eat(&registries, &state, carried, &selections).err(),
        Some(EatError::PreparedStrikeActive)
    );
    assert_eq!(
        validate_drink(&registries, &state, water, serving).err(),
        Some(DrinkError::PreparedStrikeActive)
    );
    assert_eq!(
        meal.commit(&mut state),
        Err(EatCommitError::StaleCombatRevision {
            expected: combat_before,
            actual: combat_after,
        })
    );
    assert_eq!(
        drink.commit(&mut state),
        Err(DrinkCommitError::StaleCombatRevision {
            expected: combat_before,
            actual: combat_after,
        })
    );
    assert_eq!(state, prepared_state);
    assert_eq!(calculate_matter_accounting(&state).unwrap(), matter_before);
    assert_eq!(
        calculate_fluid_volume_accounting(&state).unwrap(),
        fluid_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(restored, state);

    let cancelled = validate_cancel_primitive_preparation(&state)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(cancelled, PrimitivePreparationOutcome::Cancelled);
    let mut drinking = state.clone();
    let eaten = validate_eat(&registries, &state, carried, &selections)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let drank = validate_drink(&registries, &drinking, water, serving)
        .unwrap()
        .commit(&mut drinking)
        .unwrap();
    assert_eq!(eaten.total_mass(), meal_mass);
    assert_eq!(drank.volume(), serving);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &drinking), Ok(()));
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        matter_before.total()
    );
    assert_eq!(
        calculate_fluid_volume_accounting(&drinking).unwrap(),
        fluid_before
    );
}

#[test]
fn prepared_weapon_cannot_be_dropped_or_disassembled_into_unloadable_state() {
    use crate::content::EQUIPMENT_WOODEN_CLUB;
    use crate::equipment::{EquipmentDisassemblyError, validate_disassemble_equipment};
    use crate::logistics::{PlayerEquipmentCarryError, validate_drop_equipment};
    let (registries, mut state, _, _, tip) = opening_contact_world();
    validate_admit_player(&registries, &state, tip)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let club = arm_player(&registries, &mut state, EQUIPMENT_WOODEN_CLUB);
    let stockpile = state.logistics().player().unwrap().carried_stockpile();
    let _ = validate_prepare_primitive_strike(&registries, &state, east())
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert!(matches!(
        validate_drop_equipment(&registries, &state, club),
        Err(PlayerEquipmentCarryError::PreparedStrikeActive)
    ));
    assert_eq!(
        validate_disassemble_equipment(&registries, &state, club, stockpile).err(),
        Some(EquipmentDisassemblyError::EquipmentPreparedForCombat { equipment: club })
    );
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(
        &serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap(),
    )
    .unwrap()
    .into_state(&registries)
    .unwrap();
    assert_eq!(loaded, state);
    let _ = validate_cancel_primitive_preparation(&state)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_drop_equipment(&registries, &state, club)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let _ = validate_disassemble_equipment(&registries, &state, club, stockpile)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert!(state.equipment().get_equipment(club).is_none());
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
