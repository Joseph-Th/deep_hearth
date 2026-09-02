//! Durable equipment identity, record schema, operation traces, and owner-local mutation payloads.

use serde::{Deserialize, Serialize};

use crate::core::quantity::Mass;
use crate::core::time::SimulationTick;
use crate::inventory::ConsumedMaterialTrace;
use crate::maintenance::Condition;
use crate::structural::StructuralElementId;

use crate::equipment::definitions::EquipmentDefinitionId;

/// Persistent identifier for one runtime equipment record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EquipmentId(u32);

impl EquipmentId {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        assert!(value != 0, "equipment id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Complete owner-local payload for one prevalidated additive equipment upgrade.
pub(in crate::equipment) struct EquipmentUpgradeMutation {
    pub(in crate::equipment) equipment: EquipmentId,
    pub(in crate::equipment) expected_definition: EquipmentDefinitionId,
    pub(in crate::equipment) target_definition: EquipmentDefinitionId,
    pub(in crate::equipment) expected_embodied_mass: Mass,
    pub(in crate::equipment) target_embodied_mass: Mass,
    pub(in crate::equipment) additions: Vec<ConsumedMaterialTrace>,
}

/// Complete owner-local payload for one prevalidated embodied-component service.
pub(in crate::equipment) struct EquipmentComponentMaintenanceMutation {
    pub(in crate::equipment) equipment: EquipmentId,
    pub(in crate::equipment) component: crate::material::CommodityKey,
    pub(in crate::equipment) condition_before: Condition,
    pub(in crate::equipment) replacement: Vec<ConsumedMaterialTrace>,
}

/// Persistent mutable state of one maintainable equipment instance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentRecord {
    pub(in crate::equipment) id: EquipmentId,
    pub(in crate::equipment) definition: EquipmentDefinitionId,
    pub(in crate::equipment) condition: Condition,
    pub(in crate::equipment) embodied_mass: Mass,
    pub(in crate::equipment) embodied_material: Vec<ConsumedMaterialTrace>,
    pub(in crate::equipment) supported_by: Option<StructuralElementId>,
    pub(in crate::equipment) created_at: SimulationTick,
}

impl EquipmentRecord {
    #[must_use]
    pub const fn id(&self) -> EquipmentId {
        self.id
    }

    #[must_use]
    pub const fn definition(&self) -> EquipmentDefinitionId {
        self.definition
    }

    #[must_use]
    pub const fn condition(&self) -> Condition {
        self.condition
    }

    /// Returns conserved material mass currently owned by this equipment instance.
    #[must_use]
    pub const fn embodied_mass(&self) -> Mass {
        self.embodied_mass
    }

    /// Exact physical/provenance traces transferred into this instance at gameplay assembly.
    #[must_use]
    pub fn embodied_material(&self) -> &[ConsumedMaterialTrace] {
        &self.embodied_material
    }

    /// Returns the structural member currently carrying this equipment's weight, if assigned.
    #[must_use]
    pub const fn supported_by(&self) -> Option<StructuralElementId> {
        self.supported_by
    }

    #[must_use]
    pub const fn created_at(&self) -> SimulationTick {
        self.created_at
    }
}

/// Persistent provenance of the equipment instance that authorized a timed operation.
///
/// The operation owner enforces exclusivity while work is active; this trace preserves the provider
/// definition and condition that were validated at resolution time for deterministic replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentOperationTrace {
    equipment: EquipmentId,
    definition: EquipmentDefinitionId,
    condition: Condition,
}

impl EquipmentOperationTrace {
    pub(crate) const fn new(
        equipment: EquipmentId,
        definition: EquipmentDefinitionId,
        condition: Condition,
    ) -> Self {
        Self {
            equipment,
            definition,
            condition,
        }
    }

    #[must_use]
    pub const fn equipment(self) -> EquipmentId {
        self.equipment
    }

    #[must_use]
    pub const fn definition(self) -> EquipmentDefinitionId {
        self.definition
    }

    #[must_use]
    pub const fn condition(self) -> Condition {
        self.condition
    }
}

/// One completed operation's validated equipment-condition transition.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EquipmentOperationConditionOutcome {
    pub(super) equipment: EquipmentId,
    pub(super) before: Condition,
    pub(super) after: Condition,
}

impl EquipmentOperationConditionOutcome {
    pub(crate) const fn new(equipment: EquipmentId, before: Condition, after: Condition) -> Self {
        Self {
            equipment,
            before,
            after,
        }
    }
}
