//! Pure condition-to-capability projection for authored equipment definitions.

use crate::capability::{
    CapabilityEvaluationError, CapabilityId, CapabilityRegistry, CapabilityRequirement,
    CapabilitySource, CapabilityValue, evaluate_capabilities, interpolate_capability_value,
};
use crate::maintenance::Condition;

use super::super::definitions::{CapabilityConditionCurve, EquipmentDefinition};

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
