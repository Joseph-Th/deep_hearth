//! Trusted-load validation for material-backed stockpile enclosures.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::material::{
    CommodityKey, MaterialPhaseStateError, ParticleSizeStateError,
    validate_material_particle_size_state, validate_material_phase_state,
};
use crate::registry::Registries;

use super::{
    ConsumedMaterialTrace, StockpileEnclosureRecord, StockpileId, StockpileRecord,
    StockpileStorageProfile, StorageDefinition, StorageDefinitionId,
};

/// Invalid persisted state for one stockpile's material-backed storage enclosure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageEnclosureValidationError {
    UnknownDefinition {
        stockpile: StockpileId,
        definition: StorageDefinitionId,
    },
    StorageProfileMismatch {
        stockpile: StockpileId,
        stored: StockpileStorageProfile,
        authored: StockpileStorageProfile,
    },
    CapacityExceeded {
        stockpile: StockpileId,
        capacity: Mass,
        maximum: Mass,
    },
    ConstructionInFuture {
        stockpile: StockpileId,
        created_at: SimulationTick,
        current: SimulationTick,
    },
    MissingEmbodiedMaterial {
        stockpile: StockpileId,
    },
    ZeroEmbodiedTrace {
        stockpile: StockpileId,
    },
    UnknownEmbodiedCommodity {
        stockpile: StockpileId,
        commodity: CommodityKey,
    },
    ImpureEmbodiedMaterial {
        stockpile: StockpileId,
        commodity: CommodityKey,
    },
    InvalidEmbodiedPhaseState {
        stockpile: StockpileId,
        error: MaterialPhaseStateError,
    },
    InvalidEmbodiedParticleSizeState {
        stockpile: StockpileId,
        error: ParticleSizeStateError,
    },
    InvalidEmbodiedProvenanceRange {
        stockpile: StockpileId,
    },
    EmbodiedProvenanceInFuture {
        stockpile: StockpileId,
        latest_created_at: SimulationTick,
        current: SimulationTick,
    },
    EmbodiedProvenanceAfterConstruction {
        stockpile: StockpileId,
        latest_created_at: SimulationTick,
        created_at: SimulationTick,
    },
    EmbodiedTraceMassOverflow {
        stockpile: StockpileId,
    },
    EmbodiedTraceMassMismatch {
        stockpile: StockpileId,
        stored: Mass,
        traced: Mass,
        authored: Mass,
    },
    AssemblyMaterialMismatch {
        stockpile: StockpileId,
        commodity: CommodityKey,
        stored: Mass,
        authored: Mass,
    },
}

impl Display for StorageEnclosureValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownDefinition {
                stockpile,
                definition,
            } => write!(
                formatter,
                "stockpile {} references unknown storage enclosure definition {}",
                stockpile.value(),
                definition.value()
            ),
            Self::StorageProfileMismatch {
                stockpile,
                stored: _,
                authored: _,
            } => write!(
                formatter,
                "stockpile {} storage profile disagrees with its enclosure definition",
                stockpile.value()
            ),
            Self::CapacityExceeded {
                stockpile,
                capacity,
                maximum,
            } => write!(
                formatter,
                "stockpile {} capacity {} mg exceeds enclosure maximum {} mg",
                stockpile.value(),
                capacity.milligrams(),
                maximum.milligrams()
            ),
            Self::ConstructionInFuture {
                stockpile,
                created_at,
                current,
            } => write!(
                formatter,
                "stockpile {} enclosure was created at tick {} after current tick {}",
                stockpile.value(),
                created_at.value(),
                current.value()
            ),
            Self::MissingEmbodiedMaterial { stockpile } => write!(
                formatter,
                "stockpile {} enclosure has no embodied construction traces",
                stockpile.value()
            ),
            Self::ZeroEmbodiedTrace { stockpile } => write!(
                formatter,
                "stockpile {} enclosure contains a zero-mass construction trace",
                stockpile.value()
            ),
            Self::UnknownEmbodiedCommodity {
                stockpile,
                commodity,
            } => write!(
                formatter,
                "stockpile {} enclosure contains unknown construction commodity {}",
                stockpile.value(),
                commodity.value()
            ),
            Self::ImpureEmbodiedMaterial {
                stockpile,
                commodity,
            } => write!(
                formatter,
                "stockpile {} enclosure construction commodity {} is not pure host material",
                stockpile.value(),
                commodity.value()
            ),
            Self::InvalidEmbodiedPhaseState { stockpile, error } => write!(
                formatter,
                "stockpile {} enclosure has invalid construction phase state: {error}",
                stockpile.value()
            ),
            Self::InvalidEmbodiedParticleSizeState { stockpile, error } => write!(
                formatter,
                "stockpile {} enclosure has invalid construction particle state: {error}",
                stockpile.value()
            ),
            Self::InvalidEmbodiedProvenanceRange { stockpile } => write!(
                formatter,
                "stockpile {} enclosure construction provenance range is reversed",
                stockpile.value()
            ),
            Self::EmbodiedProvenanceInFuture {
                stockpile,
                latest_created_at,
                current,
            } => write!(
                formatter,
                "stockpile {} enclosure construction matter was created at tick {} after current tick {}",
                stockpile.value(),
                latest_created_at.value(),
                current.value()
            ),
            Self::EmbodiedProvenanceAfterConstruction {
                stockpile,
                latest_created_at,
                created_at,
            } => write!(
                formatter,
                "stockpile {} enclosure contains matter created at tick {} after enclosure construction tick {}",
                stockpile.value(),
                latest_created_at.value(),
                created_at.value()
            ),
            Self::EmbodiedTraceMassOverflow { stockpile } => write!(
                formatter,
                "stockpile {} enclosure construction trace mass overflowed",
                stockpile.value()
            ),
            Self::EmbodiedTraceMassMismatch {
                stockpile,
                stored,
                traced,
                authored,
            } => write!(
                formatter,
                "stockpile {} enclosure stores {} mg embodied, traces {} mg, and definition requires {} mg",
                stockpile.value(),
                stored.milligrams(),
                traced.milligrams(),
                authored.milligrams()
            ),
            Self::AssemblyMaterialMismatch {
                stockpile,
                commodity,
                stored,
                authored,
            } => write!(
                formatter,
                "stockpile {} enclosure traces {} mg of commodity {} but definition requires {} mg",
                stockpile.value(),
                stored.milligrams(),
                commodity.value(),
                authored.milligrams()
            ),
        }
    }
}

impl Error for StorageEnclosureValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidEmbodiedPhaseState { error, .. } => Some(error),
            Self::InvalidEmbodiedParticleSizeState { error, .. } => Some(error),
            Self::UnknownDefinition { .. }
            | Self::StorageProfileMismatch { .. }
            | Self::CapacityExceeded { .. }
            | Self::ConstructionInFuture { .. }
            | Self::MissingEmbodiedMaterial { .. }
            | Self::ZeroEmbodiedTrace { .. }
            | Self::UnknownEmbodiedCommodity { .. }
            | Self::ImpureEmbodiedMaterial { .. }
            | Self::InvalidEmbodiedProvenanceRange { .. }
            | Self::EmbodiedProvenanceInFuture { .. }
            | Self::EmbodiedProvenanceAfterConstruction { .. }
            | Self::EmbodiedTraceMassOverflow { .. }
            | Self::EmbodiedTraceMassMismatch { .. }
            | Self::AssemblyMaterialMismatch { .. } => None,
        }
    }
}

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
    enclosure: &StockpileEnclosureRecord,
    definition: &StorageDefinition,
    traced_mass: Mass,
    mut traced_by_commodity: BTreeMap<CommodityKey, Mass>,
) -> Result<(), StorageEnclosureValidationError> {
    let authored_mass = definition.assembly_profile().input_mass();
    if traced_mass != enclosure.embodied_mass() || enclosure.embodied_mass() != authored_mass {
        return Err(StorageEnclosureValidationError::EmbodiedTraceMassMismatch {
            stockpile,
            stored: enclosure.embodied_mass(),
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
    validate_enclosure_assembly(
        stockpile.id(),
        enclosure,
        definition,
        traced_mass,
        traced_by_commodity,
    )
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
