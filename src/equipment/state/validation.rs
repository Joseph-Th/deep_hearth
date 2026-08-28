//! Persistent-state validation for equipment; this child audits private owner data without exposing mutation.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::time::SimulationTick;
use crate::inventory::ConsumedMaterialTrace;
use crate::material::{
    CommodityKey, MaterialAssemblyProfile, MaterialPhaseStateError, MaterialRegistry,
    ParticleSizeStateError, validate_material_particle_size_state, validate_material_phase_state,
};
use crate::structural::{StructuralElementId, SupportIndexValidationFault, validate_support_index};

use super::super::definitions::{EquipmentDefinition, EquipmentDefinitionId, EquipmentRegistry};
use super::{EquipmentId, EquipmentRecord, EquipmentState};

/// Structural or cross-reference failure in decoded persistent equipment state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EquipmentValidationError {
    ZeroNextEquipmentId,
    ZeroEquipmentId,
    KeyIdMismatch {
        key: EquipmentId,
        record: EquipmentId,
    },
    NextEquipmentIdNotAboveAllocated {
        next: u32,
        highest: EquipmentId,
    },
    ZeroDefinitionId {
        equipment: EquipmentId,
    },
    ZeroSupportElementId {
        equipment: EquipmentId,
    },
    ZeroIndexedSupportElementId,
    ZeroIndexedEquipmentId {
        element: StructuralElementId,
    },
    EmptySupportIndex {
        element: StructuralElementId,
    },
    MissingSupportIndex {
        equipment: EquipmentId,
        element: StructuralElementId,
    },
    UnknownIndexedEquipment {
        equipment: EquipmentId,
        element: StructuralElementId,
    },
    SupportIndexMismatch {
        equipment: EquipmentId,
        indexed: StructuralElementId,
        actual: Option<StructuralElementId>,
    },
    UnknownDefinition {
        equipment: EquipmentId,
        definition: EquipmentDefinitionId,
    },
    EmbodiedMassMismatch {
        equipment: EquipmentId,
        stored: Mass,
        authored: Mass,
    },
    MissingAssemblyMaterial {
        equipment: EquipmentId,
    },
    UnexpectedAssemblyMaterial {
        equipment: EquipmentId,
    },
    ZeroEmbodiedTrace {
        equipment: EquipmentId,
    },
    EmbodiedTraceMassOverflow {
        equipment: EquipmentId,
    },
    EmbodiedTraceMassMismatch {
        equipment: EquipmentId,
        stored: Mass,
        traced: Mass,
    },
    UnknownEmbodiedCommodity {
        equipment: EquipmentId,
        commodity: crate::material::CommodityKey,
    },
    ImpureEmbodiedMaterial {
        equipment: EquipmentId,
        commodity: crate::material::CommodityKey,
    },
    InvalidEmbodiedPhaseState {
        equipment: EquipmentId,
        error: MaterialPhaseStateError,
    },
    InvalidEmbodiedParticleSizeState {
        equipment: EquipmentId,
        error: ParticleSizeStateError,
    },
    InvalidEmbodiedProvenanceRange {
        equipment: EquipmentId,
    },
    EmbodiedProvenanceInFuture {
        equipment: EquipmentId,
        latest_created_at: SimulationTick,
        current: SimulationTick,
    },
    AssemblyMaterialMismatch {
        equipment: EquipmentId,
        commodity: crate::material::CommodityKey,
        stored: Mass,
        authored: Mass,
    },
    CreatedInFuture {
        equipment: EquipmentId,
        created_at: SimulationTick,
        current: SimulationTick,
    },
}

impl Display for EquipmentValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroNextEquipmentId => {
                formatter.write_str("equipment next-id cursor must be nonzero")
            }
            Self::ZeroEquipmentId => formatter.write_str("equipment record id must be nonzero"),
            Self::KeyIdMismatch { key, record } => write!(
                formatter,
                "equipment map key {} disagrees with record id {}",
                key.value(),
                record.value()
            ),
            Self::ZeroSupportElementId { equipment } => write!(
                formatter,
                "equipment {} references zero structural support id",
                equipment.value()
            ),
            Self::ZeroIndexedSupportElementId => {
                formatter.write_str("equipment support reverse index contains zero structural id")
            }
            Self::ZeroIndexedEquipmentId { element } => write!(
                formatter,
                "equipment support reverse index for element {} contains zero equipment id",
                element.value()
            ),
            Self::EmptySupportIndex { element } => write!(
                formatter,
                "equipment support reverse index contains empty entry for element {}",
                element.value()
            ),
            Self::MissingSupportIndex { equipment, element } => write!(
                formatter,
                "equipment {} references support element {} but is absent from the reverse index",
                equipment.value(),
                element.value()
            ),
            Self::UnknownIndexedEquipment { equipment, element } => write!(
                formatter,
                "equipment support reverse index element {} references missing equipment {}",
                element.value(),
                equipment.value()
            ),
            Self::SupportIndexMismatch {
                equipment,
                indexed,
                actual,
            } => write!(
                formatter,
                "equipment support reverse index places equipment {} on element {} but record support is {actual:?}",
                equipment.value(),
                indexed.value()
            ),
            Self::NextEquipmentIdNotAboveAllocated { next, highest } => write!(
                formatter,
                "equipment next-id cursor {next} is not above allocated id {}",
                highest.value()
            ),
            Self::ZeroDefinitionId { equipment } => write!(
                formatter,
                "equipment {} has zero definition id",
                equipment.value()
            ),
            Self::UnknownDefinition {
                equipment,
                definition,
            } => write!(
                formatter,
                "equipment {} references unknown definition {}",
                equipment.value(),
                definition.value()
            ),
            Self::EmbodiedMassMismatch {
                equipment,
                stored,
                authored,
            } => write!(
                formatter,
                "equipment {} owns {} mg but definition requires {} mg",
                equipment.value(),
                stored.milligrams(),
                authored.milligrams()
            ),
            Self::MissingAssemblyMaterial { equipment } => write!(
                formatter,
                "equipment {} has an authored assembly profile but no persisted embodied material",
                equipment.value()
            ),
            Self::UnexpectedAssemblyMaterial { equipment } => write!(
                formatter,
                "equipment {} persists assembled material but its definition has no assembly profile",
                equipment.value()
            ),
            Self::ZeroEmbodiedTrace { equipment } => write!(
                formatter,
                "equipment {} contains a zero-mass embodied material trace",
                equipment.value()
            ),
            Self::EmbodiedTraceMassOverflow { equipment } => write!(
                formatter,
                "equipment {} embodied material trace mass overflows",
                equipment.value()
            ),
            Self::EmbodiedTraceMassMismatch {
                equipment,
                stored,
                traced,
            } => write!(
                formatter,
                "equipment {} stores {} mg embodied mass but traces own {} mg",
                equipment.value(),
                stored.milligrams(),
                traced.milligrams()
            ),
            Self::UnknownEmbodiedCommodity {
                equipment,
                commodity,
            } => write!(
                formatter,
                "equipment {} embodied material references unknown commodity {}",
                equipment.value(),
                commodity.value()
            ),
            Self::ImpureEmbodiedMaterial {
                equipment,
                commodity,
            } => write!(
                formatter,
                "equipment {} embodied commodity {} is not pure authored material",
                equipment.value(),
                commodity.value()
            ),
            Self::InvalidEmbodiedPhaseState { equipment, error } => write!(
                formatter,
                "equipment {} contains embodied matter with invalid phase state: {error}",
                equipment.value()
            ),
            Self::InvalidEmbodiedParticleSizeState { equipment, error } => write!(
                formatter,
                "equipment {} contains embodied matter with invalid particle-size state: {error}",
                equipment.value()
            ),
            Self::InvalidEmbodiedProvenanceRange { equipment } => write!(
                formatter,
                "equipment {} embodied material has an invalid provenance range",
                equipment.value()
            ),
            Self::EmbodiedProvenanceInFuture {
                equipment,
                latest_created_at,
                current,
            } => write!(
                formatter,
                "equipment {} embodied material provenance ends at tick {} after current tick {}",
                equipment.value(),
                latest_created_at.value(),
                current.value()
            ),
            Self::AssemblyMaterialMismatch {
                equipment,
                commodity,
                stored,
                authored,
            } => write!(
                formatter,
                "equipment {} owns {} mg of assembly commodity {} but definition requires {} mg",
                equipment.value(),
                stored.milligrams(),
                commodity.value(),
                authored.milligrams()
            ),
            Self::CreatedInFuture {
                equipment,
                created_at,
                current,
            } => write!(
                formatter,
                "equipment {} was created at tick {} after current tick {}",
                equipment.value(),
                created_at.value(),
                current.value()
            ),
        }
    }
}

impl Error for EquipmentValidationError {}

pub(crate) fn validate_loaded_equipment(
    definitions: &EquipmentRegistry,
    materials: &MaterialRegistry,
    state: &EquipmentState,
    current_tick: SimulationTick,
) -> Result<(), EquipmentValidationError> {
    validate_equipment_cursor(state)?;
    for (key, record) in &state.records {
        validate_equipment_record(definitions, materials, state, *key, record, current_tick)?;
    }
    validate_equipment_support_index(state)
}

fn validate_equipment_cursor(state: &EquipmentState) -> Result<(), EquipmentValidationError> {
    if state.next_equipment_id == 0 {
        return Err(EquipmentValidationError::ZeroNextEquipmentId);
    }
    if let Some(highest) = state.records.keys().next_back().copied()
        && highest.value() >= state.next_equipment_id
    {
        return Err(EquipmentValidationError::NextEquipmentIdNotAboveAllocated {
            next: state.next_equipment_id,
            highest,
        });
    }
    Ok(())
}

fn validate_equipment_record(
    definitions: &EquipmentRegistry,
    materials: &MaterialRegistry,
    state: &EquipmentState,
    key: EquipmentId,
    record: &EquipmentRecord,
    current_tick: SimulationTick,
) -> Result<(), EquipmentValidationError> {
    if key.value() == 0 || record.id.value() == 0 {
        return Err(EquipmentValidationError::ZeroEquipmentId);
    }
    if key != record.id {
        return Err(EquipmentValidationError::KeyIdMismatch {
            key,
            record: record.id,
        });
    }
    if record.definition.value() == 0 {
        return Err(EquipmentValidationError::ZeroDefinitionId {
            equipment: record.id,
        });
    }
    validate_equipment_support_reference(state, record)?;
    let Some(definition) = definitions.get_equipment(record.definition) else {
        return Err(EquipmentValidationError::UnknownDefinition {
            equipment: record.id,
            definition: record.definition,
        });
    };
    validate_equipment_material(materials, record, definition, current_tick)?;
    if record.created_at > current_tick {
        return Err(EquipmentValidationError::CreatedInFuture {
            equipment: record.id,
            created_at: record.created_at,
            current: current_tick,
        });
    }
    Ok(())
}

fn validate_equipment_support_reference(
    state: &EquipmentState,
    record: &EquipmentRecord,
) -> Result<(), EquipmentValidationError> {
    if record
        .supported_by
        .is_some_and(|element| element.value() == 0)
    {
        return Err(EquipmentValidationError::ZeroSupportElementId {
            equipment: record.id,
        });
    }
    if let Some(element) = record.supported_by
        && !state
            .equipment_by_support
            .get(&element)
            .is_some_and(|equipment| equipment.contains(&record.id))
    {
        return Err(EquipmentValidationError::MissingSupportIndex {
            equipment: record.id,
            element,
        });
    }
    Ok(())
}

fn validate_equipment_material(
    materials: &MaterialRegistry,
    record: &EquipmentRecord,
    definition: &EquipmentDefinition,
    current_tick: SimulationTick,
) -> Result<(), EquipmentValidationError> {
    if record.embodied_mass != definition.mass() {
        return Err(EquipmentValidationError::EmbodiedMassMismatch {
            equipment: record.id,
            stored: record.embodied_mass,
            authored: definition.mass(),
        });
    }
    validate_embodied_material(
        materials,
        record,
        definition.assembly_profile(),
        current_tick,
    )
}

fn validate_embodied_material(
    materials: &MaterialRegistry,
    record: &EquipmentRecord,
    assembly: Option<&MaterialAssemblyProfile>,
    current_tick: SimulationTick,
) -> Result<(), EquipmentValidationError> {
    let Some(assembly) = assembly else {
        if !record.embodied_material.is_empty() {
            return Err(EquipmentValidationError::UnexpectedAssemblyMaterial {
                equipment: record.id,
            });
        }
        return Ok(());
    };
    if record.embodied_material.is_empty() {
        return Err(EquipmentValidationError::MissingAssemblyMaterial {
            equipment: record.id,
        });
    }
    let mut traced_mass = Mass::ZERO;
    let mut stored_by_commodity = BTreeMap::new();
    for trace in &record.embodied_material {
        let commodity = validate_embodied_trace(materials, record, trace, current_tick)?;
        traced_mass = traced_mass.checked_add(trace.mass()).ok_or(
            EquipmentValidationError::EmbodiedTraceMassOverflow {
                equipment: record.id,
            },
        )?;
        let current = stored_by_commodity
            .get(&commodity)
            .copied()
            .unwrap_or(Mass::ZERO);
        let next = current.checked_add(trace.mass()).ok_or(
            EquipmentValidationError::EmbodiedTraceMassOverflow {
                equipment: record.id,
            },
        )?;
        stored_by_commodity.insert(commodity, next);
    }
    validate_embodied_totals(record, assembly, traced_mass, stored_by_commodity)
}

fn validate_embodied_trace(
    materials: &MaterialRegistry,
    record: &EquipmentRecord,
    trace: &ConsumedMaterialTrace,
    current_tick: SimulationTick,
) -> Result<CommodityKey, EquipmentValidationError> {
    if trace.mass().is_zero() {
        return Err(EquipmentValidationError::ZeroEmbodiedTrace {
            equipment: record.id,
        });
    }
    let commodity = trace.profile().commodity();
    if !materials.has_commodity(commodity) {
        return Err(EquipmentValidationError::UnknownEmbodiedCommodity {
            equipment: record.id,
            commodity,
        });
    }
    if trace.profile().composition().pure_material() != Some(commodity.material()) {
        return Err(EquipmentValidationError::ImpureEmbodiedMaterial {
            equipment: record.id,
            commodity,
        });
    }
    validate_material_phase_state(
        materials,
        commodity,
        trace.profile().composition(),
        trace.profile().temperature(),
    )
    .map_err(
        |error| EquipmentValidationError::InvalidEmbodiedPhaseState {
            equipment: record.id,
            error,
        },
    )?;
    validate_material_particle_size_state(
        materials,
        commodity,
        trace.profile().particle_size_distribution(),
    )
    .map_err(
        |error| EquipmentValidationError::InvalidEmbodiedParticleSizeState {
            equipment: record.id,
            error,
        },
    )?;
    let provenance = trace.provenance();
    if provenance.latest_created_at() < provenance.earliest_created_at() {
        return Err(EquipmentValidationError::InvalidEmbodiedProvenanceRange {
            equipment: record.id,
        });
    }
    if provenance.latest_created_at() > current_tick {
        return Err(EquipmentValidationError::EmbodiedProvenanceInFuture {
            equipment: record.id,
            latest_created_at: provenance.latest_created_at(),
            current: current_tick,
        });
    }
    Ok(commodity)
}

fn validate_embodied_totals(
    record: &EquipmentRecord,
    assembly: &MaterialAssemblyProfile,
    traced_mass: Mass,
    mut stored_by_commodity: BTreeMap<CommodityKey, Mass>,
) -> Result<(), EquipmentValidationError> {
    if traced_mass != record.embodied_mass {
        return Err(EquipmentValidationError::EmbodiedTraceMassMismatch {
            equipment: record.id,
            stored: record.embodied_mass,
            traced: traced_mass,
        });
    }
    for input in assembly.inputs() {
        let stored = stored_by_commodity
            .remove(&input.commodity())
            .unwrap_or(Mass::ZERO);
        if stored != input.mass() {
            return Err(EquipmentValidationError::AssemblyMaterialMismatch {
                equipment: record.id,
                commodity: input.commodity(),
                stored,
                authored: input.mass(),
            });
        }
    }
    if let Some((commodity, stored)) = stored_by_commodity.into_iter().next() {
        return Err(EquipmentValidationError::AssemblyMaterialMismatch {
            equipment: record.id,
            commodity,
            stored,
            authored: Mass::ZERO,
        });
    }
    Ok(())
}

fn validate_equipment_support_index(
    state: &EquipmentState,
) -> Result<(), EquipmentValidationError> {
    validate_support_index(
        &state.equipment_by_support,
        |equipment| equipment.value() == 0,
        |equipment| {
            state
                .records
                .get(&equipment)
                .map(|record| record.supported_by)
        },
    )
    .map_err(|fault| match fault {
        SupportIndexValidationFault::ZeroSupportElementId => {
            EquipmentValidationError::ZeroIndexedSupportElementId
        }
        SupportIndexValidationFault::EmptySupportBucket { element } => {
            EquipmentValidationError::EmptySupportIndex { element }
        }
        SupportIndexValidationFault::InvalidItemId { element, .. } => {
            EquipmentValidationError::ZeroIndexedEquipmentId { element }
        }
        SupportIndexValidationFault::UnknownIndexedItem { item, element } => {
            EquipmentValidationError::UnknownIndexedEquipment {
                equipment: item,
                element,
            }
        }
        SupportIndexValidationFault::SupportMismatch {
            item,
            indexed,
            actual,
        } => EquipmentValidationError::SupportIndexMismatch {
            equipment: item,
            indexed,
            actual,
        },
    })
}
