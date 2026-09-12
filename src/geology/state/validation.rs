//! Validates persisted geological deposit ownership and authored references.

use crate::core::time::SimulationTick;
use crate::material::{
    MaterialPhase, MaterialRegistry, ParticleSizeStatePolicy, validate_material_phase_state,
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
    Ok(())
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
    if materials
        .get_material(record.commodity.material())
        .is_none()
    {
        return Err(GeologyValidationError::UnknownCommodityMaterial {
            deposit,
            material: record.commodity.material(),
        });
    }
    let Some(form) = materials.get_form(record.commodity.form()) else {
        return Err(GeologyValidationError::UnknownCommodityForm {
            deposit,
            form: record.commodity.form(),
        });
    };
    if !materials.has_commodity(record.commodity) {
        return Err(GeologyValidationError::UnsupportedCommodity {
            deposit,
            commodity: record.commodity,
        });
    }
    if form.phase() != MaterialPhase::Solid {
        return Err(GeologyValidationError::UnsupportedCommodityPhase {
            deposit,
            form: record.commodity.form(),
            phase: form.phase(),
        });
    }
    if form.particle_size_policy() == ParticleSizeStatePolicy::Required {
        return Err(
            GeologyValidationError::UnsupportedCommodityParticulateForm {
                deposit,
                form: record.commodity.form(),
            },
        );
    }
    for component in record.composition.components() {
        if materials.get_material(component.material()).is_none() {
            return Err(GeologyValidationError::UnknownCompositionMaterial {
                deposit,
                material: component.material(),
            });
        }
    }
    validate_material_phase_state(
        materials,
        record.commodity,
        &record.composition,
        record.temperature,
    )
    .map_err(|error| GeologyValidationError::InvalidPhaseState { deposit, error })?;
    Ok(())
}
