//! Trusted-load validation for material-backed stockpile enclosures.

use std::collections::BTreeMap;

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::material::CommodityKey;
use crate::registry::Registries;

use super::{
    ConsumedMaterialTrace, PureMaterialTraceValidationError, StockpileEnclosureRecord, StockpileId,
    StockpileRecord, StorageDefinition,
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
    let commodity = trace
        .validate_pure_material_state(registries.materials(), state.tick())
        .map_err(|error| match error {
            PureMaterialTraceValidationError::ZeroMass => {
                StorageEnclosureValidationError::ZeroEmbodiedTrace { stockpile }
            }
            PureMaterialTraceValidationError::UnknownCommodity { commodity } => {
                StorageEnclosureValidationError::UnknownEmbodiedCommodity {
                    stockpile,
                    commodity,
                }
            }
            PureMaterialTraceValidationError::ImpureMaterial { commodity } => {
                StorageEnclosureValidationError::ImpureEmbodiedMaterial {
                    stockpile,
                    commodity,
                }
            }
            PureMaterialTraceValidationError::InvalidPhaseState(error) => {
                StorageEnclosureValidationError::InvalidEmbodiedPhaseState { stockpile, error }
            }
            PureMaterialTraceValidationError::InvalidParticleSizeState(error) => {
                StorageEnclosureValidationError::InvalidEmbodiedParticleSizeState {
                    stockpile,
                    error,
                }
            }
            PureMaterialTraceValidationError::ProvenanceInFuture {
                latest_created_at,
                current,
            } => StorageEnclosureValidationError::EmbodiedProvenanceInFuture {
                stockpile,
                latest_created_at,
                current,
            },
        })?;
    let provenance = trace.provenance();
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
    traced_by_commodity: BTreeMap<CommodityKey, Mass>,
) -> Result<(), StorageEnclosureValidationError> {
    let authored_mass = definition.assembly_profile().input_mass();
    if traced_mass != authored_mass {
        return Err(StorageEnclosureValidationError::EmbodiedMassMismatch {
            stockpile,
            traced: traced_mass,
            authored: authored_mass,
        });
    }
    if let Some((commodity, stored, authored)) = definition
        .assembly_profile()
        .first_mass_mismatch(&traced_by_commodity)
    {
        return Err(StorageEnclosureValidationError::AssemblyMaterialMismatch {
            stockpile,
            commodity,
            stored,
            authored,
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
