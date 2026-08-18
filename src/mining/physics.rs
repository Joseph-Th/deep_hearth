//! Shared deterministic mining physics used by admission and persistence validation.

use crate::capability::{CapabilityId, CapabilityValue, CapabilityValueKind};
use crate::core::quantity::{Mass, MassFlow, Pressure};
use crate::core::time::TickSpan;
use crate::equipment::{EquipmentDefinition, resolve_equipment_capability};
use crate::maintenance::{Condition, calculate_condition_after_active_ticks};
use crate::material::MaterialId;
use crate::ore_processing::{MassFlowDurationError, calculate_mass_flow_duration_ceiling};
use crate::registry::Registries;

use super::MiningMethodDefinition;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MiningPhysicsError {
    MissingCapability {
        capability: CapabilityId,
    },
    CapabilityKindMismatch {
        capability: CapabilityId,
        expected: CapabilityValueKind,
        found: CapabilityValueKind,
    },
    UnknownMaterialDefinition {
        material: MaterialId,
    },
    BatchTooLarge {
        maximum: Mass,
        requested: Mass,
    },
    MaterialTooHard {
        hardness: Pressure,
        maximum: Pressure,
    },
    ZeroThroughput,
    Duration(MassFlowDurationError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ResolvedMiningPhysics {
    duration: TickSpan,
    condition_after: Condition,
}

impl ResolvedMiningPhysics {
    #[must_use]
    pub(crate) const fn duration(self) -> TickSpan {
        self.duration
    }

    #[must_use]
    pub(crate) const fn condition_after(self) -> Condition {
        self.condition_after
    }
}

/// Resolves extraction throughput, batch capacity, host-material resistance, duration, and tool wear
/// from immutable method/equipment definitions plus the deposit's authored host material.
pub(crate) fn resolve_mining_physics(
    registries: &Registries,
    method: &MiningMethodDefinition,
    equipment: &EquipmentDefinition,
    condition_before: Condition,
    host_material: MaterialId,
    mass: Mass,
) -> Result<ResolvedMiningPhysics, MiningPhysicsError> {
    let flow_capability = method.mass_flow_capability();
    let flow_value = resolve_equipment_capability(equipment, condition_before, flow_capability)
        .ok_or(MiningPhysicsError::MissingCapability {
            capability: flow_capability,
        })?;
    let CapabilityValue::MassFlow(flow) = flow_value else {
        return Err(MiningPhysicsError::CapabilityKindMismatch {
            capability: flow_capability,
            expected: CapabilityValueKind::MassFlow,
            found: flow_value.kind(),
        });
    };

    let batch_capability = method.max_batch_mass_capability();
    let batch_value = resolve_equipment_capability(equipment, condition_before, batch_capability)
        .ok_or(MiningPhysicsError::MissingCapability {
        capability: batch_capability,
    })?;
    let CapabilityValue::Mass(maximum_batch) = batch_value else {
        return Err(MiningPhysicsError::CapabilityKindMismatch {
            capability: batch_capability,
            expected: CapabilityValueKind::Mass,
            found: batch_value.kind(),
        });
    };

    let hardness_capability = method.max_hardness_capability();
    let hardness_value =
        resolve_equipment_capability(equipment, condition_before, hardness_capability).ok_or(
            MiningPhysicsError::MissingCapability {
                capability: hardness_capability,
            },
        )?;
    let CapabilityValue::Pressure(maximum_hardness) = hardness_value else {
        return Err(MiningPhysicsError::CapabilityKindMismatch {
            capability: hardness_capability,
            expected: CapabilityValueKind::Pressure,
            found: hardness_value.kind(),
        });
    };

    if flow == MassFlow::ZERO {
        return Err(MiningPhysicsError::ZeroThroughput);
    }
    if mass > maximum_batch {
        return Err(MiningPhysicsError::BatchTooLarge {
            maximum: maximum_batch,
            requested: mass,
        });
    }

    let material = registries.materials().get_material(host_material).ok_or(
        MiningPhysicsError::UnknownMaterialDefinition {
            material: host_material,
        },
    )?;
    let material_hardness = Pressure::from_pascals(
        u64::from(material.properties().mechanical().hardness_mpa()) * 1_000_000,
    );
    if material_hardness > maximum_hardness {
        return Err(MiningPhysicsError::MaterialTooHard {
            hardness: material_hardness,
            maximum: maximum_hardness,
        });
    }

    let duration =
        calculate_mass_flow_duration_ceiling(flow, mass, registries.core().ticks_per_second())
            .map_err(MiningPhysicsError::Duration)?;
    let condition_after = calculate_condition_after_active_ticks(
        method.condition_wear_ppm_per_active_tick(),
        condition_before,
        duration,
    );
    Ok(ResolvedMiningPhysics {
        duration,
        condition_after,
    })
}
