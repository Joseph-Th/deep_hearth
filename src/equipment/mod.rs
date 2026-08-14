//! Maintainable capability-provider subsystem; definitions are immutable, state owns records, execution mutates, and integration resolves provider views.

mod definitions;
mod equipment_execution;
mod equipment_integration;
mod equipment_structural_integration;
mod repair_execution;
mod state;

pub use definitions::{
    CapabilityConditionCurve, CapabilityConditionPoint, EquipmentDefinition, EquipmentDefinitionId,
    EquipmentRegistry,
};
pub use equipment_execution::{
    AddEquipmentError, EquipmentConditionCommitError, EquipmentConditionPlan,
    EquipmentConditionPlanError, add_equipment, apply_equipment_condition_plan,
    decide_equipment_wear,
};
pub use equipment_integration::{
    EquipmentProviderError, ResolvedEquipmentProvider, resolve_equipment_provider,
};
pub use equipment_structural_integration::{
    EquipmentSupportCommitError, EquipmentSupportError, EquipmentSupportOutcome,
    ValidatedEquipmentSupportChange, validate_mount_equipment, validate_unmount_equipment,
};
pub use repair_execution::{
    EquipmentRepairCommitError, EquipmentRepairError, EquipmentRepairMaterialError,
    EquipmentRepairOutcome, EquipmentRepairResolution, ValidatedEquipmentRepair,
    validate_equipment_repair,
};
pub use state::{
    EquipmentId, EquipmentOperationTrace, EquipmentRecord, EquipmentState, EquipmentValidationError,
};

pub(crate) use equipment_execution::{
    EquipmentOperationConditionOutcome, apply_operation_condition_outcomes,
};
pub(crate) use equipment_integration::{ValidatedEquipmentUse, resolve_equipment_capability};
pub(crate) use state::validate_loaded_equipment;
