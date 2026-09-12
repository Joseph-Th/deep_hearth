//! Trusted-load validation for material-backed stockpile enclosures.

use std::collections::BTreeMap;

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::material::{
    CommodityKey, validate_material_particle_size_state, validate_material_phase_state,
};
use crate::registry::Registries;

use super::{
    ConsumedMaterialTrace, StockpileEnclosureRecord, StockpileId, StockpileRecord,
    StorageDefinition,
};

mod error;

pub use error::StorageEnclosureValidationError;

fn validate_enclosure_definition_binding(
    state: &AppState,
    stockpile: &StockpileRecord,
    enclosure: &StockpileEnclosureRecord,
    definition: &StorageDefinition,
) -> Result<(), StorageEnclosureValidationError> {
    if stockpile.storage_profile() != definition.storage_profile() {
        return Err(StorageEnclosureValidationError::StorageProfileMismatch {
            stockpile: stockpile.id(),
            stored: stockpile.storage_profile(),
            authored: definition.storage_profile(),
        });
    }
    if stockpile.capacity() > definition.maximum_stockpile_capacity() {
        return Err(StorageEnclosureValidationError::CapacityExceeded {
            stockpile: stockpile.id(),
            capacity: stockpile.capacity(),
            maximum: definition.maximum_stockpile_capacity(),
        });
    }
    if enclosure.created_at() > state.tick() {
        return Err(StorageEnclosureValidationError::ConstructionInFuture {
            stockpile: stockpile.id(),
            created_at: enclosure.created_at(),
            current: state.tick(),
        });
    }
    if enclosure.embodied_material().is_empty() {
        return Err(StorageEnclosureValidationError::MissingEmbodiedMaterial {
            stockpile: stockpile.id(),
        });
    }
    Ok(())
}

fn validate_enclosure_trace(
    registries: &Registries,
    state: &AppState,
    stockpile: StockpileId,
    enclosure: &StockpileEnclosureRecord,
    trace: &ConsumedMaterialTrace,
) -> Result<CommodityKey, StorageEnclosureValidationError> {
    if trace.mass().is_zero() {
        return Err(StorageEnclosureValidationError::ZeroEmbodiedTrace { stockpile });
    }
    let commodity = trace.profile().commodity();
    if !registries.materials().has_commodity(commodity) {
        return Err(StorageEnclosureValidationError::UnknownEmbodiedCommodity {
            stockpile,
            commodity,
        });
    }
    if trace.profile().composition().pure_material() != Some(commodity.material()) {
        return Err(StorageEnclosureValidationError::ImpureEmbodiedMaterial {
            stockpile,
            commodity,
        });
    }
    validate_material_phase_state(
        registries.materials(),
        commodity,
        trace.profile().composition(),
        trace.profile().temperature(),
    )
    .map_err(
        |error| StorageEnclosureValidationError::InvalidEmbodiedPhaseState { stockpile, error },
    )?;
    validate_material_particle_size_state(
        registries.materials(),
        commodity,
        trace.profile().particle_size_distribution(),
    )
    .map_err(
        |error| StorageEnclosureValidationError::InvalidEmbodiedParticleSizeState {
            stockpile,
            error,
        },
    )?;
    let provenance = trace.provenance();
    if provenance.latest_created_at() < provenance.earliest_created_at() {
        return Err(StorageEnclosureValidationError::InvalidEmbodiedProvenanceRange { stockpile });
    }
    if provenance.latest_created_at() > state.tick() {
        return Err(
            StorageEnclosureValidationError::EmbodiedProvenanceInFuture {
                stockpile,
                latest_created_at: provenance.latest_created_at(),
                current: state.tick(),
            },
        );
    }
    if provenance.latest_created_at() > enclosure.created_at() {
        return Err(
            StorageEnclosureValidationError::EmbodiedProvenanceAfterConstruction {
                stockpile,
                latest_created_at: provenance.latest_created_at(),
                created_at: enclosure.created_at(),
            },
        );
    }
    Ok(commodity)
}

fn collect_validated_enclosure_traces(
    registries: &Registries,
    state: &AppState,
    stockpile: StockpileId,
    enclosure: &StockpileEnclosureRecord,
) -> Result<(Mass, BTreeMap<CommodityKey, Mass>), StorageEnclosureValidationError> {
    let mut traced_mass = Mass::ZERO;
    let mut traced_by_commodity = BTreeMap::new();
    for trace in enclosure.embodied_material() {
        let commodity = validate_enclosure_trace(registries, state, stockpile, enclosure, trace)?;
        traced_mass = traced_mass
            .checked_add(trace.mass())
            .ok_or(StorageEnclosureValidationError::EmbodiedTraceMassOverflow { stockpile })?;
        let current = traced_by_commodity
            .get(&commodity)
            .copied()
            .unwrap_or(Mass::ZERO);
        let next = current
            .checked_add(trace.mass())
            .ok_or(StorageEnclosureValidationError::EmbodiedTraceMassOverflow { stockpile })?;
        traced_by_commodity.insert(commodity, next);
    }
    Ok((traced_mass, traced_by_commodity))
}

fn validate_enclosure_assembly(
    stockpile: StockpileId,
    definition: &StorageDefinition,
    traced_mass: Mass,
    mut traced_by_commodity: BTreeMap<CommodityKey, Mass>,
) -> Result<(), StorageEnclosureValidationError> {
    let authored_mass = definition.assembly_profile().input_mass();
    if traced_mass != authored_mass {
        return Err(StorageEnclosureValidationError::EmbodiedMassMismatch {
            stockpile,
            traced: traced_mass,
            authored: authored_mass,
        });
    }
    for input in definition.assembly_profile().inputs() {
        let stored = traced_by_commodity
            .remove(&input.commodity())
            .unwrap_or(Mass::ZERO);
        if stored != input.mass() {
            return Err(StorageEnclosureValidationError::AssemblyMaterialMismatch {
                stockpile,
                commodity: input.commodity(),
                stored,
                authored: input.mass(),
            });
        }
    }
    if let Some((commodity, stored)) = traced_by_commodity.into_iter().next() {
        return Err(StorageEnclosureValidationError::AssemblyMaterialMismatch {
            stockpile,
            commodity,
            stored,
            authored: Mass::ZERO,
        });
    }
    Ok(())
}

fn validate_loaded_storage_enclosure(
    registries: &Registries,
    state: &AppState,
    stockpile: &StockpileRecord,
    enclosure: &StockpileEnclosureRecord,
) -> Result<(), StorageEnclosureValidationError> {
    let definition = registries.storage().get(enclosure.definition()).ok_or(
        StorageEnclosureValidationError::UnknownDefinition {
            stockpile: stockpile.id(),
            definition: enclosure.definition(),
        },
    )?;
    validate_enclosure_definition_binding(state, stockpile, enclosure, definition)?;
    let (traced_mass, traced_by_commodity) =
        collect_validated_enclosure_traces(registries, state, stockpile.id(), enclosure)?;
    validate_enclosure_assembly(stockpile.id(), definition, traced_mass, traced_by_commodity)
}

/// Replays every persisted storage enclosure against immutable authored definitions.
pub(crate) fn validate_loaded_storage_enclosures(
    registries: &Registries,
    state: &AppState,
) -> Result<(), StorageEnclosureValidationError> {
    for stockpile in state.inventory().stockpiles() {
        let Some(enclosure) = stockpile.enclosure() else {
            continue;
        };
        validate_loaded_storage_enclosure(registries, state, stockpile, enclosure)?;
    }
    Ok(())
}
