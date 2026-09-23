//! Resolves condition-adjusted equipment capabilities from immutable definitions and runtime state.

use crate::capability::{
    CapabilityEvaluationError, CapabilityId, CapabilityRegistry, CapabilityRequirement,
    CapabilitySource, CapabilityValue, evaluate_capabilities, interpolate_capability_value,
};
use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::maintenance::{Condition, MaintenanceBand, MaintenanceThresholds};
use crate::registry::Registries;
use crate::structural::{StructuralElementId, StructuralLifecycle};

use super::availability::{EquipmentOccupancy, equipment_occupancy};
use super::definitions::{CapabilityConditionCurve, EquipmentDefinition};
use super::state::{EquipmentId, EquipmentOperationTrace, EquipmentRecord};

mod errors;

pub use errors::EquipmentProviderError;

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

struct ConditionedEquipmentCapabilities<'definition> {
    definition: &'definition EquipmentDefinition,
    condition: Condition,
}

impl CapabilitySource for ConditionedEquipmentCapabilities<'_> {
    fn get_capability(&self, capability: CapabilityId) -> Option<CapabilityValue> {
        resolve_equipment_capability(self.definition, self.condition, capability)
    }
}

/// Evaluates authored provider requirements against one equipment definition at an explicit
/// condition without requiring a live runtime record.
///
/// Runtime resolvers normally evaluate a resolved provider directly. Trusted-load replay uses
/// this projection to reproduce that same condition-adjusted eligibility from the immutable
/// definition and the condition recorded when the job started.
pub(crate) fn evaluate_equipment_capabilities_at_condition(
    registry: &CapabilityRegistry,
    definition: &EquipmentDefinition,
    condition: Condition,
    requirements: &[CapabilityRequirement],
) -> Result<(), CapabilityEvaluationError> {
    evaluate_capabilities(
        registry,
        &ConditionedEquipmentCapabilities {
            definition,
            condition,
        },
        requirements,
    )
}

/// Projects one definition capability at an explicit condition without authorizing runtime use.
///
/// This is the public read-side counterpart to equipment-provider resolution. Callers may use it
/// for planning or UI, but current ownership, support, occupancy, and stale-state checks remain
/// the responsibility of the operation-specific runtime resolver.
#[must_use]
pub fn project_equipment_capability(
    definition: &EquipmentDefinition,
    condition: Condition,
    capability: CapabilityId,
) -> Option<CapabilityValue> {
    resolve_equipment_capability(definition, condition, capability)
}

pub(crate) fn resolve_equipment_provider_with_occupancy<'state>(
    registries: &'state Registries,
    state: &'state AppState,
    equipment: EquipmentId,
) -> Result<
    (
        ResolvedEquipmentProvider<'state>,
        Option<EquipmentOccupancy>,
    ),
    EquipmentProviderError,
> {
    let Some(record) = state.equipment().get_equipment(equipment) else {
        return Err(EquipmentProviderError::UnknownEquipment { equipment });
    };
    let occupancy = equipment_occupancy(state, equipment);
    match occupancy {
        Some(EquipmentOccupancy::Maintenance { completes_at }) => {
            return Err(EquipmentProviderError::MaintenanceInProgress {
                equipment,
                completes_at,
            });
        }
        Some(EquipmentOccupancy::Prospecting { completes_at }) => {
            return Err(EquipmentProviderError::ProspectingInProgress {
                equipment,
                completes_at,
            });
        }
        Some(
            EquipmentOccupancy::Production { .. }
            | EquipmentOccupancy::Mining { .. }
            | EquipmentOccupancy::ManualPower { .. },
        )
        | None => {}
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
    Ok((
        ResolvedEquipmentProvider {
            record,
            definition,
            expected_equipment_revision: state.equipment().revision(),
            expected_structure_revision,
        },
        occupancy,
    ))
}

/// Resolves static capability data and current condition while rejecting direct maintenance and
/// prospecting work that already owns the equipment.
///
/// This is the base provider boundary used by exact operation resolvers. Call
/// `resolve_available_equipment_provider` for disposable current-opportunity projections that must
/// also reject production, mining, and manual-power occupancy before reporting usable capacity.
pub fn resolve_equipment_provider<'state>(
    registries: &'state Registries,
    state: &'state AppState,
    equipment: EquipmentId,
) -> Result<ResolvedEquipmentProvider<'state>, EquipmentProviderError> {
    resolve_equipment_provider_with_occupancy(registries, state, equipment)
        .map(|(provider, _occupancy)| provider)
}

/// Resolves one equipment provider only when no canonical operation currently occupies it.
///
/// Current-opportunity projections use this stricter boundary so capability planning cannot report
/// work that is physically blocked by any in-flight canonical equipment owner. Exact operation
/// admission remains responsible for its domain-specific occupancy errors and commit-time race
/// checks.
pub(crate) fn resolve_available_equipment_provider<'state>(
    registries: &'state Registries,
    state: &'state AppState,
    equipment: EquipmentId,
) -> Result<ResolvedEquipmentProvider<'state>, EquipmentProviderError> {
    let (provider, occupancy) =
        resolve_equipment_provider_with_occupancy(registries, state, equipment)?;
    match occupancy {
        Some(EquipmentOccupancy::Production { job, release }) => {
            return Err(EquipmentProviderError::ProductionInProgress {
                equipment,
                job,
                release,
            });
        }
        Some(EquipmentOccupancy::Mining { job }) => {
            return Err(EquipmentProviderError::MiningInProgress { equipment, job });
        }
        Some(EquipmentOccupancy::ManualPower { completes_at }) => {
            return Err(EquipmentProviderError::ManualPowerInProgress {
                equipment,
                completes_at,
            });
        }
        Some(EquipmentOccupancy::Prospecting { .. } | EquipmentOccupancy::Maintenance { .. }) => {
            unreachable!("base equipment provider rejects direct equipment custody")
        }
        None => {}
    }
    Ok(provider)
}

#[cfg(test)]
#[path = "equipment_integration_tests.rs"]
mod tests;
