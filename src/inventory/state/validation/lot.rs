//! Material-lot validation against material definitions, provenance, ownership, and storage policy.

use crate::core::time::SimulationTick;
use crate::material::{
    MaterialRegistry, validate_material_particle_size_state, validate_material_phase_state,
};

use super::super::{InventoryState, MaterialLotId, MaterialLotRecord, StockpileRecord};
use super::InventoryValidationError;

pub(super) fn validate_material_lot(
    materials: &MaterialRegistry,
    state: &InventoryState,
    key: MaterialLotId,
    lot: &MaterialLotRecord,
    current_tick: SimulationTick,
) -> Result<(), InventoryValidationError> {
    if key.value() == 0 || lot.id.value() == 0 {
        return Err(InventoryValidationError::ZeroLotId);
    }
    if key != lot.id {
        return Err(InventoryValidationError::LotIdMismatch {
            key,
            record: lot.id,
        });
    }
    if lot.mass.is_zero() {
        return Err(InventoryValidationError::ZeroLotMass { lot: key });
    }
    validate_lot_material_state(materials, key, lot)?;
    validate_lot_provenance(key, lot, current_tick)?;
    let Some(owner) = state.stockpiles.get(&lot.stockpile) else {
        return Err(InventoryValidationError::MissingLotOwner {
            lot: key,
            stockpile: lot.stockpile,
        });
    };
    validate_lot_storage(materials, key, lot, owner, current_tick)
}

fn validate_lot_material_state(
    materials: &MaterialRegistry,
    key: MaterialLotId,
    lot: &MaterialLotRecord,
) -> Result<(), InventoryValidationError> {
    lot.composition()
        .validate()
        .map_err(|error| InventoryValidationError::InvalidLotComposition { lot: key, error })?;
    if lot
        .composition()
        .parts_per_million(lot.commodity().material())
        == 0
    {
        return Err(InventoryValidationError::LotCompositionMissingHost {
            lot: key,
            host: lot.commodity().material(),
        });
    }
    if materials.get_material(lot.commodity().material()).is_some()
        && materials.get_form(lot.commodity().form()).is_some()
        && !materials.has_commodity(lot.commodity())
    {
        return Err(InventoryValidationError::UnsupportedLotCommodity {
            lot: key,
            commodity: lot.commodity(),
        });
    }
    validate_material_phase_state(
        materials,
        lot.commodity(),
        lot.composition(),
        lot.temperature(),
    )
    .map_err(|error| InventoryValidationError::InvalidLotPhaseState { lot: key, error })?;
    validate_material_particle_size_state(
        materials,
        lot.commodity(),
        lot.particle_size_distribution(),
    )
    .map_err(|error| InventoryValidationError::InvalidLotParticleSizeState { lot: key, error })?;
    Ok(())
}

fn validate_lot_provenance(
    key: MaterialLotId,
    lot: &MaterialLotRecord,
    current_tick: SimulationTick,
) -> Result<(), InventoryValidationError> {
    if lot.latest_created_at() < lot.created_at() {
        return Err(InventoryValidationError::InvalidLotProvenanceRange {
            lot: key,
            earliest: lot.created_at(),
            latest: lot.latest_created_at(),
        });
    }
    if lot.latest_created_at() > current_tick {
        return Err(InventoryValidationError::LotProvenanceInFuture {
            lot: key,
            latest: lot.latest_created_at(),
            current: current_tick,
        });
    }
    Ok(())
}

fn validate_lot_storage_history(
    key: MaterialLotId,
    lot: &MaterialLotRecord,
    owner: &StockpileRecord,
    current_tick: SimulationTick,
) -> Result<(), InventoryValidationError> {
    let transition = lot.storage_history().last_transition_at();
    if transition < lot.created_at() {
        return Err(
            InventoryValidationError::LotStorageTransitionBeforeCreation {
                lot: key,
                transition,
                created: lot.created_at(),
            },
        );
    }
    if transition > current_tick {
        return Err(InventoryValidationError::LotStorageTransitionInFuture {
            lot: key,
            transition,
            current: current_tick,
        });
    }
    if lot
        .storage_history()
        .project(
            current_tick,
            owner.storage_profile().preservation_multiplier_ppm(),
        )
        .is_none()
    {
        return Err(InventoryValidationError::LotStorageAgeOverflow { lot: key });
    }
    Ok(())
}

fn validate_lot_storage(
    materials: &MaterialRegistry,
    key: MaterialLotId,
    lot: &MaterialLotRecord,
    owner: &StockpileRecord,
    current_tick: SimulationTick,
) -> Result<(), InventoryValidationError> {
    validate_lot_storage_history(key, lot, owner, current_tick)?;
    let form_id = lot.commodity().form();
    let Some(form) = materials.get_form(form_id) else {
        return Err(InventoryValidationError::UnknownLotForm {
            lot: key,
            form: form_id,
        });
    };
    if !owner.storage_profile.can_store_phase(form.phase()) {
        return Err(InventoryValidationError::LotPhaseNotAccepted {
            lot: key,
            stockpile: lot.stockpile,
            phase: form.phase(),
        });
    }
    if lot.temperature() > owner.storage_profile.maximum_temperature() {
        return Err(
            InventoryValidationError::LotTemperatureExceedsStorageMaximum {
                lot: key,
                stockpile: lot.stockpile,
                temperature: lot.temperature(),
                maximum: owner.storage_profile.maximum_temperature(),
            },
        );
    }
    Ok(())
}
