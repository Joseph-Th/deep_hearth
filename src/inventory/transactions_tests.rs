//! Tests for the sibling transactions module; isolated so test-only edits do not invalidate production builds.

use super::*;
use crate::content::{
    FORM_CHIP, FORM_FOOD, FORM_LOG, FORM_LUMP, FORM_MOLTEN, FORM_ORE, MATERIAL_BERRIES,
    MATERIAL_CHARCOAL, MATERIAL_COPPER, MATERIAL_SLAG, MATERIAL_STONE, MATERIAL_WOOD,
    build_registries,
};
use crate::core::state::apply_clock_advance;
use crate::core::time::SimulationTick;
use crate::core::time::WorldSeed;
use crate::inventory::{
    MaterialFixtureError, add_solid_stockpile_for_test, deposit_bulk_for_test,
    deposit_composed_lot_for_test, deposit_lot_for_test, validate_loaded_inventory,
};
use crate::material::CompositionComponent;
use crate::matter::calculate_matter_accounting;

fn wood_log() -> CommodityKey {
    CommodityKey::new(MATERIAL_WOOD, FORM_LOG)
}

#[test]
fn default_stockpile_rejects_liquid_material_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1A70_1001));
    let stockpile = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("solid stockpile fixture failed: {error}"),
    };
    let before = state.clone();

    let result = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(1_357_770),
    );

    assert_eq!(
        result,
        Err(MaterialFixtureError::Ingress(
            MaterialIngressError::Storage(StockpileStorageError::PhaseNotAccepted {
                stockpile,
                phase: MaterialPhase::Liquid,
            })
        ))
    );
    assert_eq!(state, before);
}

#[test]
fn liquid_storage_accepts_matching_phase_but_enforces_temperature_limit() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1A70_1002));
    let maximum = Temperature::from_millikelvin(1_400_000);
    let profile = match StockpileStorageProfile::new(false, true, maximum) {
        Ok(profile) => profile,
        Err(error) => panic!("liquid storage profile fixture failed: {error}"),
    };
    let vessel = match add_stockpile(&mut state, Mass::from_milligrams(100), profile) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("liquid storage fixture failed: {error}"),
    };

    if let Err(error) = deposit_lot_for_test(
        &registries,
        &mut state,
        vessel,
        CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(1_357_770),
    ) {
        panic!("valid molten deposit was rejected: {error}");
    }
    let before_hot_rejection = state.clone();
    let too_hot = Temperature::from_millikelvin(1_500_000);
    assert_eq!(
        deposit_lot_for_test(
            &registries,
            &mut state,
            vessel,
            CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN),
            Mass::from_milligrams(1),
            too_hot,
        ),
        Err(MaterialFixtureError::Ingress(
            MaterialIngressError::Storage(StockpileStorageError::TemperatureExceedsMaximum {
                stockpile: vessel,
                temperature: too_hot,
                maximum,
            })
        ))
    );
    assert_eq!(state, before_hot_rejection);
    assert_eq!(
        validate_loaded_inventory(registries.materials(), state.inventory(), state.tick()),
        Ok(())
    );
}

#[test]
fn transfer_rechecks_destination_containment_for_actual_selected_lots() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1A70_1003));
    let source_profile =
        match StockpileStorageProfile::new(false, true, Temperature::from_millikelvin(2_000_000)) {
            Ok(profile) => profile,
            Err(error) => panic!("source vessel profile failed: {error}"),
        };
    let source = match add_stockpile(&mut state, Mass::from_milligrams(100), source_profile) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("source vessel failed: {error}"),
    };
    let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("destination pile failed: {error}"),
    };
    if let Err(error) = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(1_357_770),
    ) {
        panic!("molten transfer source fixture failed: {error}");
    }
    let before = state.clone();

    assert_eq!(
        validate_material_transfer_for_test(
            &registries,
            &state,
            source,
            destination,
            CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN),
            Mass::from_milligrams(5),
        ),
        Err(MaterialTransferError::Storage(
            StockpileStorageError::PhaseNotAccepted {
                stockpile: destination,
                phase: MaterialPhase::Liquid,
            }
        ))
    );
    assert_eq!(state, before);
}

#[test]
fn failed_transfer_leaves_both_stockpiles_unchanged() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(1));
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(id) => id,
        Err(error) => panic!("fixture stockpile failed: {error}"),
    };
    let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(5)) {
        Ok(id) => id,
        Err(error) => panic!("fixture stockpile failed: {error}"),
    };
    if let Err(error) = deposit_bulk_for_test(
        &registries,
        &mut state,
        source,
        wood_log(),
        Mass::from_milligrams(10),
    ) {
        panic!("fixture deposit failed: {error}");
    }
    let before = state.clone();

    let result = validate_material_transfer_for_test(
        &registries,
        &state,
        source,
        destination,
        wood_log(),
        Mass::from_milligrams(10),
    );

    assert!(matches!(
        result,
        Err(MaterialTransferError::CapacityExceeded {
            stockpile: _stockpile,
            capacity: _capacity,
            committed: _committed,
            requested: _requested,
        })
    ));
    assert_eq!(state, before);
}

#[test]
fn same_stockpile_transfer_is_rejected_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(11));
    let stockpile = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(id) => id,
        Err(error) => panic!("fixture stockpile failed: {error}"),
    };
    if let Err(error) = deposit_bulk_for_test(
        &registries,
        &mut state,
        stockpile,
        wood_log(),
        Mass::from_milligrams(10),
    ) {
        panic!("fixture deposit failed: {error}");
    }
    let before = state.clone();

    assert_eq!(
        validate_material_transfer_for_test(
            &registries,
            &state,
            stockpile,
            stockpile,
            wood_log(),
            Mass::from_milligrams(5),
        ),
        Err(MaterialTransferError::SameStockpile { stockpile })
    );
    assert_eq!(state, before);
}

#[test]
fn validated_transfer_updates_cached_mass_and_contents_atomically() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(2));
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(id) => id,
        Err(error) => panic!("fixture stockpile failed: {error}"),
    };
    let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(id) => id,
        Err(error) => panic!("fixture stockpile failed: {error}"),
    };
    if let Err(error) = deposit_bulk_for_test(
        &registries,
        &mut state,
        source,
        wood_log(),
        Mass::from_milligrams(30),
    ) {
        panic!("fixture deposit failed: {error}");
    }

    let token = match validate_material_transfer_for_test(
        &registries,
        &state,
        source,
        destination,
        wood_log(),
        Mass::from_milligrams(12),
    ) {
        Ok(token) => token,
        Err(error) => panic!("transfer validation failed: {error}"),
    };
    if let Err(error) = token.commit(&mut state) {
        panic!("transfer commit failed: {error}");
    }

    let source_record = match state.inventory().get_stockpile(source) {
        Some(record) => record,
        None => panic!("source disappeared"),
    };
    let destination_record = match state.inventory().get_stockpile(destination) {
        Some(record) => record,
        None => panic!("destination disappeared"),
    };
    assert_eq!(source_record.stored_mass(), Mass::from_milligrams(18));
    assert_eq!(
        source_record.get_mass(wood_log()),
        Mass::from_milligrams(18)
    );
    assert_eq!(destination_record.stored_mass(), Mass::from_milligrams(12));
    assert_eq!(
        destination_record.get_mass(wood_log()),
        Mass::from_milligrams(12)
    );
    assert_eq!(
        validate_loaded_inventory(registries.materials(), state.inventory(), state.tick()),
        Ok(())
    );
}

#[test]
fn partial_transfer_splits_lots_without_erasing_thermal_history() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(3));
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(id) => id,
        Err(error) => panic!("fixture source failed: {error}"),
    };
    let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(id) => id,
        Err(error) => panic!("fixture destination failed: {error}"),
    };
    let cool = match deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        wood_log(),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(300_000),
    ) {
        Ok(id) => id,
        Err(error) => panic!("cool lot fixture failed: {error}"),
    };
    let hot = match deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        wood_log(),
        Mass::from_milligrams(20),
        Temperature::from_millikelvin(800_000),
    ) {
        Ok(id) => id,
        Err(error) => panic!("hot lot fixture failed: {error}"),
    };

    let token = match validate_material_transfer_for_test(
        &registries,
        &state,
        source,
        destination,
        wood_log(),
        Mass::from_milligrams(15),
    ) {
        Ok(token) => token,
        Err(error) => panic!("split transfer validation failed: {error}"),
    };
    if let Err(error) = token.commit(&mut state) {
        panic!("split transfer commit failed: {error}");
    }

    let cool_lot = match state.inventory().get_lot(cool) {
        Some(lot) => lot,
        None => panic!("full moved cool lot disappeared"),
    };
    assert_eq!(cool_lot.stockpile(), destination);
    assert_eq!(cool_lot.mass(), Mass::from_milligrams(10));
    assert_eq!(
        cool_lot.temperature(),
        Temperature::from_millikelvin(300_000)
    );

    let hot_lot = match state.inventory().get_lot(hot) {
        Some(lot) => lot,
        None => panic!("hot source lot disappeared"),
    };
    assert_eq!(hot_lot.stockpile(), source);
    assert_eq!(hot_lot.mass(), Mass::from_milligrams(15));
    assert_eq!(
        hot_lot.temperature(),
        Temperature::from_millikelvin(800_000)
    );

    let destination_lots: Vec<_> = state.inventory().lot_ids(destination).collect();
    assert_eq!(destination_lots.len(), 2);
    let split = match destination_lots.into_iter().find(|id| *id != cool) {
        Some(id) => id,
        None => panic!("split lot missing"),
    };
    let split_lot = match state.inventory().get_lot(split) {
        Some(lot) => lot,
        None => panic!("split lot record missing"),
    };
    assert_eq!(split_lot.mass(), Mass::from_milligrams(5));
    assert_eq!(
        split_lot.temperature(),
        Temperature::from_millikelvin(800_000)
    );
    assert_eq!(
        validate_loaded_inventory(registries.materials(), state.inventory(), state.tick()),
        Ok(())
    );
}

#[test]
fn stale_transfer_token_is_rejected_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(4));
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(id) => id,
        Err(error) => panic!("fixture source failed: {error}"),
    };
    let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(id) => id,
        Err(error) => panic!("fixture destination failed: {error}"),
    };
    if let Err(error) = deposit_bulk_for_test(
        &registries,
        &mut state,
        source,
        wood_log(),
        Mass::from_milligrams(20),
    ) {
        panic!("fixture deposit failed: {error}");
    }
    let token = match validate_material_transfer_for_test(
        &registries,
        &state,
        source,
        destination,
        wood_log(),
        Mass::from_milligrams(10),
    ) {
        Ok(token) => token,
        Err(error) => panic!("transfer validation failed: {error}"),
    };

    if let Err(error) = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1)) {
        panic!("intervening stockpile mutation failed: {error}");
    }
    let before_commit = state.clone();
    let result = token.commit(&mut state);

    assert!(matches!(
        result,
        Err(MaterialTransferCommitError::StaleInventoryRevision {
            expected: _expected,
            actual: _actual,
        })
    ));
    assert_eq!(state, before_commit);
}

#[test]
fn repeated_partial_transfers_coalesce_new_fragments_in_destination() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(41));
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(id) => id,
        Err(error) => panic!("fixture source failed: {error}"),
    };
    let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(id) => id,
        Err(error) => panic!("fixture destination failed: {error}"),
    };
    if let Err(error) = deposit_bulk_for_test(
        &registries,
        &mut state,
        source,
        wood_log(),
        Mass::from_milligrams(10),
    ) {
        panic!("fixture deposit failed: {error}");
    }

    for _ in 0..2 {
        let token = match validate_material_transfer_for_test(
            &registries,
            &state,
            source,
            destination,
            wood_log(),
            Mass::from_milligrams(3),
        ) {
            Ok(token) => token,
            Err(error) => panic!("fragment transfer validation failed: {error}"),
        };
        if let Err(error) = token.commit(&mut state) {
            panic!("fragment transfer commit failed: {error}");
        }
    }

    let source_record = match state.inventory().get_stockpile(source) {
        Some(record) => record,
        None => panic!("source disappeared"),
    };
    let destination_record = match state.inventory().get_stockpile(destination) {
        Some(record) => record,
        None => panic!("destination disappeared"),
    };
    assert_eq!(source_record.get_mass(wood_log()), Mass::from_milligrams(4));
    assert_eq!(
        destination_record.get_mass(wood_log()),
        Mass::from_milligrams(6)
    );
    assert_eq!(state.inventory().lot_ids(destination).count(), 1);
    assert_eq!(state.inventory().lots().count(), 2);
    assert_eq!(
        validate_loaded_inventory(registries.materials(), state.inventory(), state.tick()),
        Ok(())
    );
}

#[test]
fn composed_lot_split_preserves_normalized_constituent_profile() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(5));
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(id) => id,
        Err(error) => panic!("fixture source failed: {error}"),
    };
    let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(id) => id,
        Err(error) => panic!("fixture destination failed: {error}"),
    };
    let composition = match MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, 700_000),
        CompositionComponent::new(MATERIAL_SLAG, 300_000),
    ]) {
        Ok(composition) => composition,
        Err(error) => panic!("composition fixture failed: {error}"),
    };
    let commodity = CommodityKey::new(MATERIAL_COPPER, FORM_ORE);
    let original = match deposit_composed_lot_for_test(
        &registries,
        &mut state,
        source,
        commodity,
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(400_000),
        composition.clone(),
    ) {
        Ok(id) => id,
        Err(error) => panic!("composed lot fixture failed: {error}"),
    };

    let token = match validate_material_transfer_for_test(
        &registries,
        &state,
        source,
        destination,
        commodity,
        Mass::from_milligrams(4),
    ) {
        Ok(token) => token,
        Err(error) => panic!("composed split validation failed: {error}"),
    };
    if let Err(error) = token.commit(&mut state) {
        panic!("composed split commit failed: {error}");
    }

    let source_lot = match state.inventory().get_lot(original) {
        Some(lot) => lot,
        None => panic!("source composition lot disappeared"),
    };
    assert_eq!(source_lot.mass(), Mass::from_milligrams(6));
    assert_eq!(source_lot.composition(), &composition);
    let split_id = match state.inventory().lot_ids(destination).next() {
        Some(id) => id,
        None => panic!("destination split lot missing"),
    };
    let split = match state.inventory().get_lot(split_id) {
        Some(lot) => lot,
        None => panic!("destination split lot record missing"),
    };
    assert_eq!(split.mass(), Mass::from_milligrams(4));
    assert_eq!(split.composition(), &composition);
    assert_eq!(
        split.composition().parts_per_million(MATERIAL_COPPER),
        700_000
    );
    assert_eq!(
        split.composition().parts_per_million(MATERIAL_SLAG),
        300_000
    );
    assert_eq!(
        validate_loaded_inventory(registries.materials(), state.inventory(), state.tick()),
        Ok(())
    );
}

fn stored_lot_total(state: &AppState) -> Mass {
    state.inventory().lots().fold(Mass::ZERO, |acc, lot| {
        acc.checked_add(lot.mass())
            .unwrap_or_else(|| panic!("conservation test overflow"))
    })
}

fn stored_aggregate_total(state: &AppState) -> Mass {
    state
        .inventory()
        .stockpiles()
        .fold(Mass::ZERO, |acc, pile| {
            acc.checked_add(pile.stored_mass())
                .unwrap_or_else(|| panic!("conservation test overflow"))
        })
}

fn assert_lot_aggregate_agreement(registries: &Registries, state: &AppState, label: &str) {
    assert_eq!(
        stored_lot_total(state),
        stored_aggregate_total(state),
        "{label}: lot total disagrees with stockpile aggregate total"
    );
    assert_eq!(
        validate_loaded_inventory(registries.materials(), state.inventory(), state.tick()),
        Ok(())
    );
}

#[test]
fn transfer_split_sequence_preserves_inventory_quantity() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1A70_2001));
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
        let token = validate_material_transfer_for_test(
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
    let mut state = AppState::new(WorldSeed::new(0x1A70_2002));
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
        validate_material_transfer_for_test(
            &registries,
            &state,
            source,
            destination,
            wood_log(),
            Mass::from_milligrams(11),
        ),
        Err(MaterialTransferError::InsufficientMass {
            stockpile: source,
            commodity: wood_log(),
            available: Mass::from_milligrams(10),
            requested: Mass::from_milligrams(11),
        })
    );
    assert_eq!(
        validate_material_transfer_for_test(
            &registries,
            &state,
            source,
            destination,
            wood_log(),
            Mass::from_milligrams(9),
        ),
        Err(MaterialTransferError::CapacityExceeded {
            stockpile: destination,
            capacity: Mass::from_milligrams(5),
            committed: Mass::ZERO,
            requested: Mass::from_milligrams(9),
        })
    );
    assert_eq!(state, before, "failed validation must not mutate inventory");

    let valid = validate_material_transfer_for_test(
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
            Err(MaterialTransferCommitError::StaleInventoryRevision {
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
    let mut state = AppState::new(WorldSeed::new(0x1A70_2003));
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
        CommodityKey::new(MATERIAL_CHARCOAL, FORM_LUMP),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(500_000),
    );
    let created_at = state.tick();
    let deposit_plan = decide_reserved_deposits(
        &registries,
        state.inventory(),
        created_at,
        vec![ReservedDepositRequest::new(
            destination,
            vec![output],
            Mass::from_milligrams(10),
        )],
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
    let mut state = AppState::new(WorldSeed::new(0x1A70_2004));
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
    let mut state = AppState::new(WorldSeed::new(0x1A70_2005));
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
    validate_loaded_inventory(registries.materials(), state.inventory(), state.tick())
        .unwrap_or_else(|error| panic!("reformed inventory failed validation: {error}"));
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
fn material_reform_preserves_accumulated_storage_exposure() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1A70_2009));
    let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("reform-age stockpile failed: {error}"));
    let food = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        food,
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("reform-age food fixture failed: {error}"));
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
        &[MaterialInputSpec::new(food, Mass::from_milligrams(10))],
    )
    .unwrap_or_else(|error| panic!("reform-age selection failed: {error:?}"));

    validate_material_reform_from_selection(
        &registries,
        &state,
        stockpile,
        CommodityKey::new(MATERIAL_BERRIES, FORM_CHIP),
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
                record.commodity() == CommodityKey::new(MATERIAL_BERRIES, FORM_CHIP)
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
