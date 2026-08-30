//! Identity, occupancy, persistence, and recovery contracts for additive energy-store upgrades.

use super::*;

use crate::content::{
    ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE, ENERGY_MECHANICAL_SMALL_DRIVE,
    ENERGY_STONE_FLYWHEEL_DRIVE, EQUIPMENT_STONE_HAND_CRANK, FORM_FLYWHEEL, FORM_HANDLE,
    FORM_REINFORCEMENT, MANUAL_POWER_HAND_CRANK, MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD,
    build_registries,
};
use crate::core::quantity::{Energy, Mass, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
use crate::core::time::WorldSeed;
use crate::energy::{
    EnergySinkError, EnergyStoreRecord, add_energy_store, calculate_explicit_energy_accounting,
    validate_assemble_energy_store, validate_disassemble_energy_store,
};
use crate::equipment::validate_assemble_equipment;
use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};
use crate::labor::{ManualPowerError, ManualPowerRequest, validate_start_manual_power};
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::simulation::advance_tick;
use crate::survival::initialize_player_survival;

const TEMPERATURE: Temperature = Temperature::from_millikelvin(293_150);

fn assemble_stone_flywheel(registries: &Registries, state: &mut AppState) -> EnergyStoreId {
    let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_100_000))
        .unwrap_or_else(|error| panic!("flywheel upgrade assembly stockpile failed: {error}"));
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
            Mass::from_milligrams(900_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(200_000),
        ),
    ] {
        deposit_lot_for_test(registries, state, source, commodity, mass, TEMPERATURE)
            .unwrap_or_else(|error| panic!("flywheel upgrade assembly material failed: {error}"));
    }
    validate_assemble_energy_store(registries, state, ENERGY_STONE_FLYWHEEL_DRIVE, source)
        .unwrap_or_else(|error| panic!("flywheel upgrade assembly validation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("flywheel upgrade assembly commit failed: {error}"))
}

fn reinforcement_source(registries: &Registries, state: &mut AppState) -> StockpileId {
    let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(20_000))
        .unwrap_or_else(|error| panic!("flywheel reinforcement stockpile failed: {error}"));
    deposit_lot_for_test(
        registries,
        state,
        source,
        CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
        Mass::from_milligrams(20_000),
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("flywheel reinforcement material failed: {error}"));
    source
}

fn assemble_stone_crank(
    registries: &Registries,
    state: &mut AppState,
) -> crate::equipment::EquipmentId {
    let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_100_000))
        .unwrap_or_else(|error| panic!("flywheel race crank stockpile failed: {error}"));
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
            Mass::from_milligrams(900_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(200_000),
        ),
    ] {
        deposit_lot_for_test(registries, state, source, commodity, mass, TEMPERATURE)
            .unwrap_or_else(|error| panic!("flywheel race crank material failed: {error}"));
    }
    validate_assemble_equipment(registries, state, EQUIPMENT_STONE_HAND_CRANK, source)
        .unwrap_or_else(|error| panic!("flywheel race crank assembly failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("flywheel race crank commit failed: {error}"))
}

#[test]
fn copper_banded_flywheel_upgrade_preserves_identity_matter_and_replay() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xE660_1001));
    let store = assemble_stone_flywheel(&registries, &mut state);
    let created_at = state
        .energy()
        .get_store(store)
        .unwrap_or_else(|| panic!("flywheel disappeared after assembly"))
        .created_at();
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("flywheel provenance tick failed: {error}"));
    let reinforcement = reinforcement_source(&registries, &mut state);
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("flywheel upgrade matter-before failed: {error}"))
        .total();
    let energy_before = calculate_explicit_energy_accounting(&registries, &state)
        .unwrap_or_else(|error| panic!("flywheel upgrade energy-before failed: {error}"))
        .total();

    let upgraded = validate_upgrade_energy_store(
        &registries,
        &state,
        store,
        ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE,
        reinforcement,
    )
    .unwrap_or_else(|error| panic!("flywheel upgrade validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("flywheel upgrade commit failed: {error}"));

    assert_eq!(upgraded, store);
    let record = state
        .energy()
        .get_store(store)
        .unwrap_or_else(|| panic!("upgraded flywheel disappeared"));
    assert_eq!(
        record.definition(),
        ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE
    );
    assert_eq!(record.created_at(), created_at);
    assert_eq!(record.stored(), Energy::ZERO);
    assert_eq!(record.embodied_mass(), Mass::from_milligrams(1_120_000));
    assert_eq!(record.embodied_material().len(), 3);
    assert!(record.embodied_material().iter().any(|trace| {
        trace.profile().commodity() == CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT)
            && trace.mass() == Mass::from_milligrams(20_000)
            && trace.provenance().latest_created_at() == state.tick()
    }));
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("flywheel upgrade matter-after failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(
        calculate_explicit_energy_accounting(&registries, &state)
            .unwrap_or_else(|error| panic!("flywheel upgrade energy-after failed: {error}"))
            .total(),
        energy_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("flywheel upgrade serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("flywheel upgrade decode failed: {error}"));
    assert_eq!(
        decoded
            .into_state(&registries)
            .unwrap_or_else(|error| panic!("flywheel upgrade trusted load failed: {error}")),
        state
    );
}

#[test]
fn flywheel_upgrade_requires_empty_store_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xE660_1002));
    let store = assemble_stone_flywheel(&registries, &mut state);
    let reinforcement = reinforcement_source(&registries, &mut state);
    state
        .energy_state_mut()
        .add_stored_energy(store, Energy::from_nanojoules(1));
    let before = state.clone();

    assert_eq!(
        validate_upgrade_energy_store(
            &registries,
            &state,
            store,
            ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE,
            reinforcement,
        )
        .err(),
        Some(EnergyStoreUpgradeError::StoreNotEmpty {
            store,
            stored: Energy::from_nanojoules(1),
        })
    );
    assert_eq!(state, before);
}

#[test]
fn manual_power_start_invalidates_prior_flywheel_upgrade_without_energy_revision_change() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xE660_1003));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("flywheel race survival setup failed: {error}"));
    let crank = assemble_stone_crank(&registries, &mut state);
    let store = assemble_stone_flywheel(&registries, &mut state);
    let reinforcement = reinforcement_source(&registries, &mut state);
    let token = validate_upgrade_energy_store(
        &registries,
        &state,
        store,
        ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE,
        reinforcement,
    )
    .unwrap_or_else(|error| panic!("flywheel race upgrade validation failed: {error}"));
    let energy_revision = state.energy().revision();
    validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(
            MANUAL_POWER_HAND_CRANK,
            crank,
            store,
            Energy::from_nanojoules(1_000_000_000),
        ),
    )
    .unwrap_or_else(|error| panic!("flywheel race manual-power validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("flywheel race manual-power commit failed: {error}"));
    assert_eq!(state.energy().revision(), energy_revision);
    let before_commit = state.clone();

    assert_eq!(
        token.commit(&mut state),
        Err(EnergyStoreUpgradeCommitError::StoreBusyManualPower { store })
    );
    assert_eq!(state, before_commit);
}

#[test]
fn intervening_energy_mutation_invalidates_flywheel_upgrade_token() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xE660_1004));
    let store = assemble_stone_flywheel(&registries, &mut state);
    let reinforcement = reinforcement_source(&registries, &mut state);
    let token = validate_upgrade_energy_store(
        &registries,
        &state,
        store,
        ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE,
        reinforcement,
    )
    .unwrap_or_else(|error| panic!("stale flywheel upgrade validation failed: {error}"));
    let expected = state.energy().revision();
    add_energy_store(&registries, &mut state, ENERGY_MECHANICAL_SMALL_DRIVE)
        .unwrap_or_else(|error| panic!("stale flywheel competing mutation failed: {error}"));
    let before_commit = state.clone();

    assert_eq!(
        token.commit(&mut state),
        Err(EnergyStoreUpgradeCommitError::StaleEnergy {
            expected,
            actual: expected + 1,
        })
    );
    assert_eq!(state, before_commit);
}

#[test]
fn upgraded_flywheel_disassembly_returns_reusable_reinforcement_exactly() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xE660_1005));
    let store = assemble_stone_flywheel(&registries, &mut state);
    let reinforcement = reinforcement_source(&registries, &mut state);
    validate_upgrade_energy_store(
        &registries,
        &state,
        store,
        ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE,
        reinforcement,
    )
    .unwrap_or_else(|error| panic!("flywheel recovery upgrade validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("flywheel recovery upgrade commit failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_120_000))
        .unwrap_or_else(|error| panic!("flywheel recovery destination failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("flywheel recovery matter-before failed: {error}"))
        .total();

    let outcome = validate_disassemble_energy_store(&registries, &state, store, destination)
        .unwrap_or_else(|error| panic!("flywheel recovery disassembly failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("flywheel recovery disassembly commit failed: {error}"));
    assert_eq!(outcome.recovered_lots().len(), 3);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| stockpile
                .get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT))),
        Some(Mass::from_milligrams(20_000))
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("flywheel recovery matter-after failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn flywheel_reinforcement_makes_a_seven_hundred_fifty_joule_manual_charge_reachable() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xE660_1006));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("flywheel capacity survival setup failed: {error}"));
    let crank = assemble_stone_crank(&registries, &mut state);
    let store = assemble_stone_flywheel(&registries, &mut state);
    let reinforcement = reinforcement_source(&registries, &mut state);
    let requested = Energy::from_nanojoules(750_000_000_000);

    assert_eq!(
        validate_start_manual_power(
            &registries,
            &state,
            ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, store, requested),
        )
        .err(),
        Some(ManualPowerError::EnergySink(
            EnergySinkError::InsufficientCapacity {
                store,
                stored: Energy::ZERO,
                requested,
                capacity: Energy::from_nanojoules(500_000_000_000),
            }
        ))
    );

    validate_upgrade_energy_store(
        &registries,
        &state,
        store,
        ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE,
        reinforcement,
    )
    .unwrap_or_else(|error| panic!("flywheel capacity upgrade validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("flywheel capacity upgrade commit failed: {error}"));
    let start = validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, store, requested),
    )
    .unwrap_or_else(|error| panic!("flywheel capacity manual power failed: {error}"));
    let duration = start.work().completes_at().value() - state.tick().value();
    assert_eq!(duration, 5);
    start
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("flywheel capacity manual-power commit failed: {error}"));
    for _ in 0..duration {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("flywheel capacity charging tick failed: {error}"));
    }
    assert_eq!(
        state
            .energy()
            .get_store(store)
            .map(EnergyStoreRecord::stored),
        Some(requested)
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
