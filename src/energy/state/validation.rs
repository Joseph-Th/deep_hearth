//! Validates persisted energy stores, embodied matter, authored references, and cursors.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Mass};
use crate::core::time::SimulationTick;
use crate::material::{
    CommodityKey, MaterialPhaseStateError, MaterialRegistry, ParticleSizeStateError,
};

use super::super::definitions::{EnergyRegistry, EnergyStoreDefinitionId};
use super::{EnergyState, EnergyStoreId, EnergyStoreRecord};

mod embodiment;

use embodiment::validate_embodied_material;

/// Invalid persisted energy ownership discovered during exhaustive load validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnergyValidationError {
    InvalidIdCursor,
    ZeroStoreId,
    RecordKeyMismatch {
        key: EnergyStoreId,
        record: EnergyStoreId,
    },
    UnknownDefinition {
        store: EnergyStoreId,
        definition: EnergyStoreDefinitionId,
    },
    CapacityExceeded {
        store: EnergyStoreId,
        stored: Energy,
        capacity: Energy,
    },
    MissingAssemblyMaterial {
        store: EnergyStoreId,
    },
    UnexpectedAssemblyMaterial {
        store: EnergyStoreId,
    },
    ZeroEmbodiedTrace {
        store: EnergyStoreId,
    },
    EmbodiedTraceMassOverflow {
        store: EnergyStoreId,
    },
    EmbodiedMassMismatch {
        store: EnergyStoreId,
        traced: Mass,
        authored: Mass,
    },
    UnknownEmbodiedCommodity {
        store: EnergyStoreId,
        commodity: CommodityKey,
    },
    ImpureEmbodiedMaterial {
        store: EnergyStoreId,
        commodity: CommodityKey,
    },
    InvalidEmbodiedPhaseState {
        store: EnergyStoreId,
        error: MaterialPhaseStateError,
    },
    InvalidEmbodiedParticleSizeState {
        store: EnergyStoreId,
        error: ParticleSizeStateError,
    },
    InvalidEmbodiedProvenanceRange {
        store: EnergyStoreId,
    },
    EmbodiedProvenanceInFuture {
        store: EnergyStoreId,
        latest_created_at: SimulationTick,
        current: SimulationTick,
    },
    EmbodiedProvenanceAfterConstruction {
        store: EnergyStoreId,
        latest_created_at: SimulationTick,
        created_at: SimulationTick,
    },
    AssemblyMaterialMismatch {
        store: EnergyStoreId,
        commodity: CommodityKey,
        stored: Mass,
        authored: Mass,
    },
    CreatedInFuture {
        store: EnergyStoreId,
        created_at: SimulationTick,
        current: SimulationTick,
    },
}

impl Display for EnergyValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidIdCursor => formatter.write_str("energy store ID cursor is invalid"),
            Self::ZeroStoreId => formatter.write_str("energy store ID must be nonzero"),
            Self::RecordKeyMismatch { key, record } => write!(
                formatter,
                "energy store map key {} disagrees with record id {}",
                key.value(),
                record.value()
            ),
            Self::UnknownDefinition { store, definition } => write!(
                formatter,
                "energy store {} references unknown definition {}",
                store.value(),
                definition.value()
            ),
            Self::CapacityExceeded {
                store,
                stored,
                capacity,
            } => write!(
                formatter,
                "energy store {} contains {} nJ above capacity {} nJ",
                store.value(),
                stored.nanojoules(),
                capacity.nanojoules()
            ),
            Self::MissingAssemblyMaterial { store } => write!(
                formatter,
                "energy store {} has an authored assembly profile but no persisted embodied material",
                store.value()
            ),
            Self::UnexpectedAssemblyMaterial { store } => write!(
                formatter,
                "energy store {} persists assembled material but its definition has no assembly profile",
                store.value()
            ),
            Self::ZeroEmbodiedTrace { store } => write!(
                formatter,
                "energy store {} contains a zero-mass embodied material trace",
                store.value()
            ),
            Self::EmbodiedTraceMassOverflow { store } => write!(
                formatter,
                "energy store {} embodied material trace mass overflows",
                store.value()
            ),
            Self::EmbodiedMassMismatch {
                store,
                traced,
                authored,
            } => write!(
                formatter,
                "energy store {} traces {} mg embodied mass but its assembly requires {} mg",
                store.value(),
                traced.milligrams(),
                authored.milligrams()
            ),
            Self::UnknownEmbodiedCommodity { store, commodity } => write!(
                formatter,
                "energy store {} embodied material references unknown commodity {}",
                store.value(),
                commodity.value()
            ),
            Self::ImpureEmbodiedMaterial { store, commodity } => write!(
                formatter,
                "energy store {} embodied commodity {} is not pure authored material",
                store.value(),
                commodity.value()
            ),
            Self::InvalidEmbodiedPhaseState { store, error } => write!(
                formatter,
                "energy store {} contains embodied matter with invalid phase state: {error}",
                store.value()
            ),
            Self::InvalidEmbodiedParticleSizeState { store, error } => write!(
                formatter,
                "energy store {} contains embodied matter with invalid particle-size state: {error}",
                store.value()
            ),
            Self::InvalidEmbodiedProvenanceRange { store } => write!(
                formatter,
                "energy store {} embodied material has an invalid provenance range",
                store.value()
            ),
            Self::EmbodiedProvenanceInFuture {
                store,
                latest_created_at,
                current,
            } => write!(
                formatter,
                "energy store {} embodied material provenance ends at tick {} after current tick {}",
                store.value(),
                latest_created_at.value(),
                current.value()
            ),
            Self::EmbodiedProvenanceAfterConstruction {
                store,
                latest_created_at,
                created_at,
            } => write!(
                formatter,
                "energy store {} embodied material provenance ends at tick {} after the store was constructed at tick {}",
                store.value(),
                latest_created_at.value(),
                created_at.value()
            ),
            Self::AssemblyMaterialMismatch {
                store,
                commodity,
                stored,
                authored,
            } => write!(
                formatter,
                "energy store {} owns {} mg of assembly commodity {} but definition requires {} mg",
                store.value(),
                stored.milligrams(),
                commodity.value(),
                authored.milligrams()
            ),
            Self::CreatedInFuture {
                store,
                created_at,
                current,
            } => write!(
                formatter,
                "energy store {} was created at tick {} after current tick {}",
                store.value(),
                created_at.value(),
                current.value()
            ),
        }
    }
}

impl Error for EnergyValidationError {}

pub(crate) fn validate_loaded_energy(
    registry: &EnergyRegistry,
    materials: &MaterialRegistry,
    state: &EnergyState,
    current: SimulationTick,
) -> Result<(), EnergyValidationError> {
    if !state.has_valid_id_cursor() {
        return Err(EnergyValidationError::InvalidIdCursor);
    }
    for (key, record) in &state.records {
        validate_energy_store_record(registry, materials, *key, record, current)?;
    }
    Ok(())
}

fn validate_energy_store_record(
    registry: &EnergyRegistry,
    materials: &MaterialRegistry,
    key: EnergyStoreId,
    record: &EnergyStoreRecord,
    current: SimulationTick,
) -> Result<(), EnergyValidationError> {
    validate_energy_store_identity(key, record.id)?;
    let Some(definition) = registry.get_store(record.definition) else {
        return Err(EnergyValidationError::UnknownDefinition {
            store: record.id,
            definition: record.definition,
        });
    };
    if record.stored > definition.capacity() {
        return Err(EnergyValidationError::CapacityExceeded {
            store: record.id,
            stored: record.stored,
            capacity: definition.capacity(),
        });
    }
    validate_embodied_material(
        registry,
        materials,
        record,
        definition.assembly_profile(),
        current,
    )?;
    if record.created_at > current {
        return Err(EnergyValidationError::CreatedInFuture {
            store: record.id,
            created_at: record.created_at,
            current,
        });
    }
    Ok(())
}

fn validate_energy_store_identity(
    key: EnergyStoreId,
    record: EnergyStoreId,
) -> Result<(), EnergyValidationError> {
    if key.value() == 0 || record.value() == 0 {
        return Err(EnergyValidationError::ZeroStoreId);
    }
    if key != record {
        return Err(EnergyValidationError::RecordKeyMismatch { key, record });
    }
    Ok(())
}

#[cfg(test)]
#[path = "validation_tests.rs"]
mod tests;
