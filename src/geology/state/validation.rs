//! Validates persisted geological deposit ownership and authored references.

use crate::core::time::SimulationTick;
use crate::material::MaterialRegistry;

use super::super::material_validation::{
    GeologicalMaterialStateError, validate_geological_material_state,
};
use super::{
    GeologicalDepositId, GeologicalDepositLifecycle, GeologicalDepositRecord, GeologyState,
};

mod error;

pub use error::GeologyValidationError;

pub(crate) fn validate_loaded_geology(
    materials: &MaterialRegistry,
    state: &GeologyState,
    current: SimulationTick,
) -> Result<(), GeologyValidationError> {
    validate_geology_cursor(state)?;
    for (key, record) in &state.deposits {
        validate_geological_deposit(materials, *key, record, current)?;
    }
    Ok(())
}

fn validate_geology_cursor(state: &GeologyState) -> Result<(), GeologyValidationError> {
    if state.next_deposit_id == 0 {
        return Err(GeologyValidationError::ZeroNextDepositId);
    }
    if let Some(highest) = state.deposits.keys().next_back().copied()
        && state.next_deposit_id <= highest.value()
    {
        return Err(GeologyValidationError::NextIdNotAfterExisting {
            next: state.next_deposit_id,
            highest,
        });
    }
    Ok(())
}

fn validate_geological_deposit(
    materials: &MaterialRegistry,
    key: GeologicalDepositId,
    record: &GeologicalDepositRecord,
    current: SimulationTick,
) -> Result<(), GeologyValidationError> {
    if key.value() == 0 || record.id.value() == 0 {
        return Err(GeologyValidationError::ZeroDepositId);
    }
    if key != record.id {
        return Err(GeologyValidationError::IdMismatch {
            key,
            record: record.id,
        });
    }
    validate_deposit_mass_and_lifecycle(key, record)?;
    validate_deposit_material_state(materials, key, record)?;
    if record.generated_at > current {
        return Err(GeologyValidationError::GeneratedInFuture {
            deposit: key,
            generated_at: record.generated_at,
            current,
        });
    }
    validate_deposit_timeline(key, record, current)?;
    Ok(())
}

fn validate_deposit_timeline(
    deposit: GeologicalDepositId,
    record: &GeologicalDepositRecord,
    current: SimulationTick,
) -> Result<(), GeologyValidationError> {
    match (record.lifecycle, record.depleted_at) {
        (GeologicalDepositLifecycle::Available, None) => Ok(()),
        (GeologicalDepositLifecycle::Available, Some(depleted_at)) => {
            Err(GeologyValidationError::AvailableHasDepletionTick {
                deposit,
                depleted_at,
            })
        }
        (GeologicalDepositLifecycle::Depleted, None) => {
            Err(GeologyValidationError::DepletedMissingDepletionTick { deposit })
        }
        (GeologicalDepositLifecycle::Depleted, Some(depleted_at)) => {
            if depleted_at < record.generated_at {
                return Err(GeologyValidationError::DepletionBeforeGeneration {
                    deposit,
                    generated_at: record.generated_at,
                    depleted_at,
                });
            }
            if depleted_at > current {
                return Err(GeologyValidationError::DepletedInFuture {
                    deposit,
                    depleted_at,
                    current,
                });
            }
            Ok(())
        }
    }
}

fn validate_deposit_mass_and_lifecycle(
    deposit: GeologicalDepositId,
    record: &GeologicalDepositRecord,
) -> Result<(), GeologyValidationError> {
    if record.initial_mass.is_zero() {
        return Err(GeologyValidationError::ZeroInitialMass { deposit });
    }
    if record.excavation_hardness.is_zero() {
        return Err(GeologyValidationError::ZeroExcavationHardness { deposit });
    }
    if record.remaining_mass > record.initial_mass {
        return Err(GeologyValidationError::RemainingMassExceedsInitial {
            deposit,
            initial: record.initial_mass,
            remaining: record.remaining_mass,
        });
    }
    match record.lifecycle {
        GeologicalDepositLifecycle::Available if record.remaining_mass.is_zero() => {
            Err(GeologyValidationError::AvailableWithoutMass { deposit })
        }
        GeologicalDepositLifecycle::Depleted if !record.remaining_mass.is_zero() => {
            Err(GeologyValidationError::DepletedWithRemainingMass {
                deposit,
                remaining: record.remaining_mass,
            })
        }
        GeologicalDepositLifecycle::Available | GeologicalDepositLifecycle::Depleted => Ok(()),
    }
}

fn validate_deposit_material_state(
    materials: &MaterialRegistry,
    deposit: GeologicalDepositId,
    record: &GeologicalDepositRecord,
) -> Result<(), GeologyValidationError> {
    record
        .composition
        .validate()
        .map_err(|error| GeologyValidationError::InvalidComposition { deposit, error })?;
    if record
        .composition
        .parts_per_million(record.commodity.material())
        == 0
    {
        return Err(GeologyValidationError::CompositionMissingHost {
            deposit,
            host: record.commodity.material(),
        });
    }
    validate_geological_material_state(
        materials,
        record.commodity,
        &record.composition,
        record.temperature,
    )
    .map_err(|error| map_material_state_error(deposit, error))?;
    Ok(())
}

fn map_material_state_error(
    deposit: GeologicalDepositId,
    error: GeologicalMaterialStateError,
) -> GeologyValidationError {
    match error {
        GeologicalMaterialStateError::UnknownCommodityMaterial { material } => {
            GeologyValidationError::UnknownCommodityMaterial { deposit, material }
        }
        GeologicalMaterialStateError::UnknownCommodityForm { form } => {
            GeologyValidationError::UnknownCommodityForm { deposit, form }
        }
        GeologicalMaterialStateError::UnsupportedCommodity { commodity } => {
            GeologyValidationError::UnsupportedCommodity { deposit, commodity }
        }
        GeologicalMaterialStateError::UnsupportedCommodityPhase { form, phase } => {
            GeologyValidationError::UnsupportedCommodityPhase {
                deposit,
                form,
                phase,
            }
        }
        GeologicalMaterialStateError::UnsupportedCommodityParticulateForm { form } => {
            GeologyValidationError::UnsupportedCommodityParticulateForm { deposit, form }
        }
        GeologicalMaterialStateError::UnknownCompositionMaterial { material } => {
            GeologyValidationError::UnknownCompositionMaterial { deposit, material }
        }
        GeologicalMaterialStateError::InvalidPhaseState(error) => {
            GeologyValidationError::InvalidPhaseState { deposit, error }
        }
    }
}
