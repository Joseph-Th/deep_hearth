//! Validates persisted energy stores, embodied matter, authored references, and cursors.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Mass};
use crate::core::time::SimulationTick;
use crate::inventory::ConsumedMaterialTrace;
use crate::material::{
    CommodityKey, MaterialAssemblyProfile, MaterialPhaseStateError, MaterialRegistry,
    ParticleSizeStateError, validate_material_particle_size_state, validate_material_phase_state,
};

use super::super::definitions::{EnergyRegistry, EnergyStoreDefinitionId};
use super::{EnergyState, EnergyStoreId, EnergyStoreRecord};

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
        materials,
        record,
        definition.assembly_profile(),
        definition.upgrade_profile().is_some(),
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

fn validate_embodied_material(
    materials: &MaterialRegistry,
    record: &EnergyStoreRecord,
    assembly: Option<&MaterialAssemblyProfile>,
    allows_post_construction_additions: bool,
    current: SimulationTick,
) -> Result<(), EnergyValidationError> {
    let Some(assembly) = assembly else {
        if !record.embodied_material.is_empty() {
            return Err(EnergyValidationError::UnexpectedAssemblyMaterial { store: record.id });
        }
        return Ok(());
    };

    if record.embodied_material.is_empty() {
        return Err(EnergyValidationError::MissingAssemblyMaterial { store: record.id });
    }

    let mut traced_mass = Mass::ZERO;
    let mut stored_by_commodity = BTreeMap::new();
    for trace in &record.embodied_material {
        let commodity = validate_embodied_trace(
            materials,
            record,
            trace,
            allows_post_construction_additions,
            current,
        )?;
        traced_mass = traced_mass
            .checked_add(trace.mass())
            .ok_or(EnergyValidationError::EmbodiedTraceMassOverflow { store: record.id })?;
        let stored = stored_by_commodity
            .get(&commodity)
            .copied()
            .unwrap_or(Mass::ZERO);
        let next = stored
            .checked_add(trace.mass())
            .ok_or(EnergyValidationError::EmbodiedTraceMassOverflow { store: record.id })?;
        stored_by_commodity.insert(commodity, next);
    }

    validate_embodied_totals(record, assembly, traced_mass, stored_by_commodity)
}

fn validate_embodied_trace(
    materials: &MaterialRegistry,
    record: &EnergyStoreRecord,
    trace: &ConsumedMaterialTrace,
    allows_post_construction_additions: bool,
    current: SimulationTick,
) -> Result<CommodityKey, EnergyValidationError> {
    if trace.mass().is_zero() {
        return Err(EnergyValidationError::ZeroEmbodiedTrace { store: record.id });
    }
    let commodity = trace.profile().commodity();
    if !materials.has_commodity(commodity) {
        return Err(EnergyValidationError::UnknownEmbodiedCommodity {
            store: record.id,
            commodity,
        });
    }
    if trace.profile().composition().pure_material() != Some(commodity.material()) {
        return Err(EnergyValidationError::ImpureEmbodiedMaterial {
            store: record.id,
            commodity,
        });
    }
    validate_material_phase_state(
        materials,
        commodity,
        trace.profile().composition(),
        trace.profile().temperature(),
    )
    .map_err(|error| EnergyValidationError::InvalidEmbodiedPhaseState {
        store: record.id,
        error,
    })?;
    validate_material_particle_size_state(
        materials,
        commodity,
        trace.profile().particle_size_distribution(),
    )
    .map_err(
        |error| EnergyValidationError::InvalidEmbodiedParticleSizeState {
            store: record.id,
            error,
        },
    )?;

    let provenance = trace.provenance();
    if provenance.latest_created_at() < provenance.earliest_created_at() {
        return Err(EnergyValidationError::InvalidEmbodiedProvenanceRange { store: record.id });
    }
    if provenance.latest_created_at() > current {
        return Err(EnergyValidationError::EmbodiedProvenanceInFuture {
            store: record.id,
            latest_created_at: provenance.latest_created_at(),
            current,
        });
    }
    if !allows_post_construction_additions && provenance.latest_created_at() > record.created_at {
        return Err(EnergyValidationError::EmbodiedProvenanceAfterConstruction {
            store: record.id,
            latest_created_at: provenance.latest_created_at(),
            created_at: record.created_at,
        });
    }
    Ok(commodity)
}

fn validate_embodied_totals(
    record: &EnergyStoreRecord,
    assembly: &MaterialAssemblyProfile,
    traced_mass: Mass,
    mut stored_by_commodity: BTreeMap<CommodityKey, Mass>,
) -> Result<(), EnergyValidationError> {
    if traced_mass != assembly.input_mass() {
        return Err(EnergyValidationError::EmbodiedMassMismatch {
            store: record.id,
            traced: traced_mass,
            authored: assembly.input_mass(),
        });
    }
    for input in assembly.inputs() {
        let stored = stored_by_commodity
            .remove(&input.commodity())
            .unwrap_or(Mass::ZERO);
        if stored != input.mass() {
            return Err(EnergyValidationError::AssemblyMaterialMismatch {
                store: record.id,
                commodity: input.commodity(),
                stored,
                authored: input.mass(),
            });
        }
    }
    if let Some((commodity, stored)) = stored_by_commodity.into_iter().next() {
        return Err(EnergyValidationError::AssemblyMaterialMismatch {
            store: record.id,
            commodity,
            stored,
            authored: Mass::ZERO,
        });
    }
    Ok(())
}

#[cfg(test)]
#[path = "validation_tests.rs"]
mod tests;
