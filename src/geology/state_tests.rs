//! Contract tests for geological deposit state.

use super::*;
use crate::content::{FORM_CRUSHED, FORM_MOLTEN, FORM_ORE, MATERIAL_COPPER, build_registries};
use crate::material::MaterialPhase;
use crate::spatial::VoxelCoord;

fn bounds() -> VoxelBounds {
    match VoxelBounds::new(VoxelCoord::new(0, -8, 0), VoxelCoord::new(4, -4, 4)) {
        Ok(bounds) => bounds,
        Err(error) => panic!("geology state bounds fixture failed: {error}"),
    }
}

#[test]
fn loaded_validation_rejects_incoherent_depletion_timeline() {
    let registries = build_registries();

    let (mut available_with_tick, deposit) = extraction_state(
        Mass::from_milligrams(75),
        GeologicalDepositLifecycle::Available,
    );
    available_with_tick
        .deposits
        .get_mut(&deposit)
        .unwrap_or_else(|| panic!("available timeline fixture disappeared"))
        .depleted_at = Some(SimulationTick::ZERO);
    assert_eq!(
        validate_loaded_geology(
            registries.materials(),
            &available_with_tick,
            SimulationTick::ZERO,
        ),
        Err(GeologyValidationError::AvailableHasDepletionTick {
            deposit,
            depleted_at: SimulationTick::ZERO,
        })
    );

    let (mut depleted_without_tick, deposit) =
        extraction_state(Mass::ZERO, GeologicalDepositLifecycle::Depleted);
    depleted_without_tick
        .deposits
        .get_mut(&deposit)
        .unwrap_or_else(|| panic!("depleted timeline fixture disappeared"))
        .depleted_at = None;
    assert_eq!(
        validate_loaded_geology(
            registries.materials(),
            &depleted_without_tick,
            SimulationTick::ZERO,
        ),
        Err(GeologyValidationError::DepletedMissingDepletionTick { deposit })
    );
}

fn extraction_state(
    remaining_mass: Mass,
    lifecycle: GeologicalDepositLifecycle,
) -> (GeologyState, GeologicalDepositId) {
    let deposit = GeologicalDepositId::new(1);
    let mut state = GeologyState::new();
    state.next_deposit_id = 2;
    state.deposits.insert(
        deposit,
        GeologicalDepositRecord {
            id: deposit,
            bounds: bounds(),
            commodity: CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            initial_mass: Mass::from_milligrams(100),
            remaining_mass,
            temperature: Temperature::from_millikelvin(300_000),
            excavation_hardness: Pressure::from_pascals(350_000_000),
            composition: MaterialComposition::pure(MATERIAL_COPPER),
            lifecycle,
            generated_at: SimulationTick::ZERO,
            depleted_at: (lifecycle == GeologicalDepositLifecycle::Depleted)
                .then_some(SimulationTick::ZERO),
        },
    );
    (state, deposit)
}

#[test]
fn extraction_owner_rejects_over_extraction() {
    let (mut state, deposit) = extraction_state(
        Mass::from_milligrams(75),
        GeologicalDepositLifecycle::Available,
    );
    let before = state.clone();

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        state.apply_extraction(deposit, Mass::from_milligrams(76), SimulationTick::ZERO, 1);
    }));

    assert!(result.is_err());
    assert_eq!(state, before);
}

#[test]
fn extraction_owner_rejects_zero_mass_transfer() {
    let (mut state, deposit) = extraction_state(
        Mass::from_milligrams(75),
        GeologicalDepositLifecycle::Available,
    );
    let before = state.clone();

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        state.apply_extraction(deposit, Mass::ZERO, SimulationTick::ZERO, 1);
    }));

    assert!(result.is_err());
    assert_eq!(state, before);
}

#[test]
fn extraction_owner_rejects_depleted_deposit_mutation() {
    let (mut state, deposit) = extraction_state(Mass::ZERO, GeologicalDepositLifecycle::Depleted);
    let before = state.clone();

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        state.apply_extraction(deposit, Mass::ZERO, SimulationTick::ZERO, 1);
    }));

    assert!(result.is_err());
    assert_eq!(state, before);
}

#[test]
fn extraction_owner_subtracts_exact_transferred_mass_and_depletes_at_zero() {
    let (mut state, deposit) = extraction_state(
        Mass::from_milligrams(75),
        GeologicalDepositLifecycle::Available,
    );

    state.apply_extraction(
        deposit,
        Mass::from_milligrams(25),
        SimulationTick::new(1),
        1,
    );
    let record = state
        .get_deposit(deposit)
        .unwrap_or_else(|| panic!("extracted deposit disappeared"));
    assert_eq!(record.remaining_mass(), Mass::from_milligrams(50));
    assert_eq!(record.lifecycle(), GeologicalDepositLifecycle::Available);
    assert_eq!(record.depleted_at, None);
    assert_eq!(state.revision(), 1);

    state.apply_extraction(
        deposit,
        Mass::from_milligrams(50),
        SimulationTick::new(2),
        2,
    );
    let record = state
        .get_deposit(deposit)
        .unwrap_or_else(|| panic!("depleted deposit disappeared"));
    assert_eq!(record.remaining_mass(), Mass::ZERO);
    assert_eq!(record.lifecycle(), GeologicalDepositLifecycle::Depleted);
    assert_eq!(record.depleted_at, Some(SimulationTick::new(2)));
    assert_eq!(state.revision(), 2);
}

#[test]
fn loaded_validation_rejects_lifecycle_mass_disagreement() {
    let registries = build_registries();
    let deposit = GeologicalDepositId::new(1);
    let mut state = GeologyState::new();
    state.next_deposit_id = 2;
    state.deposits.insert(
        deposit,
        GeologicalDepositRecord {
            id: deposit,
            bounds: bounds(),
            commodity: CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            initial_mass: Mass::from_milligrams(100),
            remaining_mass: Mass::from_milligrams(25),
            temperature: Temperature::from_millikelvin(300_000),
            excavation_hardness: Pressure::from_pascals(350_000_000),
            composition: MaterialComposition::pure(MATERIAL_COPPER),
            lifecycle: GeologicalDepositLifecycle::Depleted,
            generated_at: SimulationTick::ZERO,
            depleted_at: Some(SimulationTick::ZERO),
        },
    );

    assert_eq!(
        validate_loaded_geology(registries.materials(), &state, SimulationTick::ZERO),
        Err(GeologyValidationError::DepletedWithRemainingMass {
            deposit,
            remaining: Mass::from_milligrams(25),
        })
    );
}

#[test]
fn loaded_validation_rejects_zero_excavation_hardness() {
    let registries = build_registries();
    let deposit = GeologicalDepositId::new(1);
    let mut state = GeologyState::new();
    state.next_deposit_id = 2;
    state.deposits.insert(
        deposit,
        GeologicalDepositRecord {
            id: deposit,
            bounds: bounds(),
            commodity: CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            initial_mass: Mass::from_milligrams(100),
            remaining_mass: Mass::from_milligrams(100),
            temperature: Temperature::from_millikelvin(300_000),
            excavation_hardness: Pressure::ZERO,
            composition: MaterialComposition::pure(MATERIAL_COPPER),
            lifecycle: GeologicalDepositLifecycle::Available,
            generated_at: SimulationTick::ZERO,
            depleted_at: None,
        },
    );

    assert_eq!(
        validate_loaded_geology(registries.materials(), &state, SimulationTick::ZERO),
        Err(GeologyValidationError::ZeroExcavationHardness { deposit })
    );
}

#[test]
fn loaded_validation_rejects_liquid_geological_deposit() {
    let registries = build_registries();
    let deposit = GeologicalDepositId::new(1);
    let mut state = GeologyState::new();
    state.next_deposit_id = 2;
    state.deposits.insert(
        deposit,
        GeologicalDepositRecord {
            id: deposit,
            bounds: bounds(),
            commodity: CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN),
            initial_mass: Mass::from_milligrams(100),
            remaining_mass: Mass::from_milligrams(100),
            temperature: Temperature::from_millikelvin(1_357_770),
            excavation_hardness: Pressure::from_pascals(350_000_000),
            composition: MaterialComposition::pure(MATERIAL_COPPER),
            lifecycle: GeologicalDepositLifecycle::Available,
            generated_at: SimulationTick::ZERO,
            depleted_at: None,
        },
    );

    assert_eq!(
        validate_loaded_geology(registries.materials(), &state, SimulationTick::ZERO),
        Err(GeologyValidationError::UnsupportedCommodityPhase {
            deposit,
            form: FORM_MOLTEN,
            phase: MaterialPhase::Liquid,
        })
    );
}

#[test]
fn loaded_validation_rejects_processed_particulate_geological_deposit() {
    let registries = build_registries();
    let deposit = GeologicalDepositId::new(1);
    let mut state = GeologyState::new();
    state.next_deposit_id = 2;
    state.deposits.insert(
        deposit,
        GeologicalDepositRecord {
            id: deposit,
            bounds: bounds(),
            commodity: CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED),
            initial_mass: Mass::from_milligrams(100),
            remaining_mass: Mass::from_milligrams(100),
            temperature: Temperature::from_millikelvin(300_000),
            excavation_hardness: Pressure::from_pascals(350_000_000),
            composition: MaterialComposition::pure(MATERIAL_COPPER),
            lifecycle: GeologicalDepositLifecycle::Available,
            generated_at: SimulationTick::ZERO,
            depleted_at: None,
        },
    );

    assert_eq!(
        validate_loaded_geology(registries.materials(), &state, SimulationTick::ZERO),
        Err(
            GeologyValidationError::UnsupportedCommodityParticulateForm {
                deposit,
                form: FORM_CRUSHED,
            }
        )
    );
}
