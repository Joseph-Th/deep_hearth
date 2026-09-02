//! Resolves condition-adjusted equipment capabilities from immutable definitions and runtime state.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::{
    CapabilityId, CapabilitySource, CapabilityValue, interpolate_capability_value,
};
use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::maintenance::{Condition, MaintenanceBand, MaintenanceThresholds};
use crate::registry::Registries;
use crate::structural::{StructuralElementId, StructuralLifecycle};

use super::definitions::{CapabilityConditionCurve, EquipmentDefinition, EquipmentDefinitionId};
use super::state::{EquipmentId, EquipmentOperationTrace, EquipmentRecord};

/// Revision-bound equipment provider selection carried by a resolved operation until start.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ValidatedEquipmentUse {
    expected_equipment_revision: u64,
    expected_structure_revision: Option<u64>,
    support: Option<StructuralElementId>,
    trace: EquipmentOperationTrace,
}

impl ValidatedEquipmentUse {
    pub(crate) const fn expected_equipment_revision(self) -> u64 {
        self.expected_equipment_revision
    }

    pub(crate) const fn expected_structure_revision(self) -> Option<u64> {
        self.expected_structure_revision
    }

    pub(crate) const fn support(self) -> Option<StructuralElementId> {
        self.support
    }

    pub(crate) const fn trace(self) -> EquipmentOperationTrace {
        self.trace
    }
}

/// Read-only resolved provider joining one runtime record to its immutable definition.
#[derive(Clone, Copy, Debug)]
pub struct ResolvedEquipmentProvider<'state> {
    record: &'state EquipmentRecord,
    definition: &'state EquipmentDefinition,
    expected_equipment_revision: u64,
    expected_structure_revision: Option<u64>,
}

impl<'state> ResolvedEquipmentProvider<'state> {
    #[must_use]
    pub const fn id(&self) -> EquipmentId {
        self.record.id()
    }

    #[must_use]
    pub const fn condition(&self) -> Condition {
        self.record.condition()
    }

    #[must_use]
    pub const fn mass(&self) -> Mass {
        self.definition.mass()
    }

    pub(crate) const fn definition(&self) -> &'state EquipmentDefinition {
        self.definition
    }

    #[must_use]
    pub fn get_capability(&self, capability: CapabilityId) -> Option<CapabilityValue> {
        resolve_equipment_capability(self.definition, self.record.condition(), capability)
    }

    #[must_use]
    pub const fn maintenance_thresholds(&self) -> MaintenanceThresholds {
        self.definition.maintenance_thresholds()
    }

    #[must_use]
    pub fn maintenance_band(&self) -> MaintenanceBand {
        self.maintenance_thresholds().classify(self.condition())
    }

    pub(crate) const fn validated_use(&self) -> ValidatedEquipmentUse {
        ValidatedEquipmentUse {
            expected_equipment_revision: self.expected_equipment_revision,
            expected_structure_revision: self.expected_structure_revision,
            support: self.record.supported_by(),
            trace: EquipmentOperationTrace::new(
                self.record.id(),
                self.record.definition(),
                self.record.condition(),
            ),
        }
    }
}

impl CapabilitySource for ResolvedEquipmentProvider<'_> {
    fn get_capability(&self, capability: CapabilityId) -> Option<CapabilityValue> {
        ResolvedEquipmentProvider::get_capability(self, capability)
    }
}

fn resolve_curve_value(
    curve: &CapabilityConditionCurve,
    nominal: CapabilityValue,
    condition: Condition,
) -> CapabilityValue {
    let points = curve.points();
    let mut degraded = points[0];
    if condition <= degraded.condition() {
        return degraded.value();
    }

    for improved in &points[1..] {
        if condition <= improved.condition() {
            let numerator =
                condition.parts_per_million() - degraded.condition().parts_per_million();
            let denominator =
                improved.condition().parts_per_million() - degraded.condition().parts_per_million();
            return match interpolate_capability_value(
                degraded.value(),
                improved.value(),
                numerator,
                denominator,
            ) {
                Some(value) => value,
                None => panic!(
                    "equipment capability condition curve {} became invalid after registry assembly",
                    curve.capability().value()
                ),
            };
        }
        degraded = *improved;
    }

    let numerator = condition.parts_per_million() - degraded.condition().parts_per_million();
    let denominator =
        Condition::PRISTINE.parts_per_million() - degraded.condition().parts_per_million();
    match interpolate_capability_value(degraded.value(), nominal, numerator, denominator) {
        Some(value) => value,
        None => panic!(
            "equipment capability condition curve {} disagrees with its nominal capability",
            curve.capability().value()
        ),
    }
}

pub(crate) fn resolve_equipment_capability(
    definition: &EquipmentDefinition,
    condition: Condition,
    capability: CapabilityId,
) -> Option<CapabilityValue> {
    if condition == Condition::FAILED {
        return None;
    }
    let nominal = definition.capabilities().get_capability(capability)?;
    Some(
        match definition.get_capability_condition_curve(capability) {
            Some(curve) => resolve_curve_value(curve, nominal, condition),
            None => nominal,
        },
    )
}

/// Failure to resolve a runtime equipment record into its immutable provider definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EquipmentProviderError {
    UnknownEquipment {
        equipment: EquipmentId,
    },
    UnknownDefinition {
        equipment: EquipmentId,
        definition: EquipmentDefinitionId,
    },
    StructuralSupportRequired {
        equipment: EquipmentId,
    },
    UnknownStructuralSupport {
        equipment: EquipmentId,
        element: StructuralElementId,
    },
    StructuralSupportNotActive {
        equipment: EquipmentId,
        element: StructuralElementId,
        lifecycle: StructuralLifecycle,
    },
    MaintenanceInProgress {
        equipment: EquipmentId,
        completes_at: SimulationTick,
    },
}

impl Display for EquipmentProviderError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownEquipment { equipment } => {
                write!(formatter, "unknown equipment id {}", equipment.value())
            }
            Self::UnknownDefinition {
                equipment,
                definition,
            } => write!(
                formatter,
                "equipment {} references unknown definition {}",
                equipment.value(),
                definition.value()
            ),
            Self::StructuralSupportRequired { equipment } => write!(
                formatter,
                "equipment {} requires an active structural support before it can authorize work",
                equipment.value()
            ),
            Self::UnknownStructuralSupport { equipment, element } => write!(
                formatter,
                "equipment {} references missing structural support {}",
                equipment.value(),
                element.value()
            ),
            Self::MaintenanceInProgress {
                equipment,
                completes_at,
            } => write!(
                formatter,
                "equipment {} is under maintenance until tick {} and cannot authorize a new operation",
                equipment.value(),
                completes_at.value()
            ),
            Self::StructuralSupportNotActive {
                equipment,
                element,
                lifecycle,
            } => write!(
                formatter,
                "equipment {} structural support {} is {lifecycle:?} and cannot authorize a new operation",
                equipment.value(),
                element.value()
            ),
        }
    }
}

impl Error for EquipmentProviderError {}

/// Resolves static capability data and current condition without duplicating either source of truth.
pub fn resolve_equipment_provider<'state>(
    registries: &'state Registries,
    state: &'state AppState,
    equipment: EquipmentId,
) -> Result<ResolvedEquipmentProvider<'state>, EquipmentProviderError> {
    let Some(record) = state.equipment().get_equipment(equipment) else {
        return Err(EquipmentProviderError::UnknownEquipment { equipment });
    };
    if let Some(work) = state
        .player_work()
        .get_equipment_maintenance_occupant(equipment)
    {
        return Err(EquipmentProviderError::MaintenanceInProgress {
            equipment,
            completes_at: work.completes_at(),
        });
    }
    let Some(definition) = registries.equipment().get_equipment(record.definition()) else {
        return Err(EquipmentProviderError::UnknownDefinition {
            equipment,
            definition: record.definition(),
        });
    };
    if definition.requires_structural_support() && record.supported_by().is_none() {
        return Err(EquipmentProviderError::StructuralSupportRequired { equipment });
    }
    let expected_structure_revision = if let Some(element) = record.supported_by() {
        let Some(support) = state.structures().get_element(element) else {
            return Err(EquipmentProviderError::UnknownStructuralSupport { equipment, element });
        };
        if support.lifecycle() != StructuralLifecycle::Active {
            return Err(EquipmentProviderError::StructuralSupportNotActive {
                equipment,
                element,
                lifecycle: support.lifecycle(),
            });
        }
        Some(state.structures().revision())
    } else {
        None
    };
    Ok(ResolvedEquipmentProvider {
        record,
        definition,
        expected_equipment_revision: state.equipment().revision(),
        expected_structure_revision,
    })
}

#[cfg(test)]
#[path = "equipment_integration_tests.rs"]
mod tests;
