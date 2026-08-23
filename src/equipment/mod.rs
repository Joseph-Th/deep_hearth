//! Maintainable capability-provider subsystem; definitions are immutable, state owns records, execution mutates, and integration resolves provider views.

mod construction_execution;
mod definitions;
mod disassembly_execution;
mod equipment_execution;
mod equipment_integration;
mod equipment_structural_integration;
mod maintenance_execution;
mod maintenance_resolution;
mod state;
mod upgrade_execution;

pub use construction_execution::{
    EquipmentAssemblyCommitError, EquipmentAssemblyError, ValidatedEquipmentAssembly,
    validate_assemble_equipment,
};
pub use definitions::{
    CapabilityConditionCurve, CapabilityConditionPoint, EquipmentDefinition, EquipmentDefinitionId,
    EquipmentMaintenanceProfile, EquipmentRegistry, EquipmentUpgradeProfile,
};
pub use disassembly_execution::{
    EquipmentDisassemblyCommitError, EquipmentDisassemblyError, EquipmentDisassemblyOutcome,
    ValidatedEquipmentDisassembly, validate_disassemble_equipment,
};
#[cfg(any(test, feature = "test-gameplay"))]
pub(crate) use equipment_execution::add_equipment;
pub use equipment_execution::{
    EquipmentConditionCommitError, EquipmentConditionPlan, EquipmentConditionPlanError,
    apply_equipment_condition_plan, decide_equipment_wear,
};
pub use equipment_integration::{
    EquipmentProviderError, ResolvedEquipmentProvider, resolve_equipment_provider,
};
pub use equipment_structural_integration::{
    EquipmentSupportCommitError, EquipmentSupportError, EquipmentSupportOutcome,
    ValidatedEquipmentSupportChange, validate_mount_equipment, validate_relocate_equipment,
    validate_unmount_equipment,
};
pub use maintenance_execution::{
    EquipmentMaintenanceCommitError, EquipmentMaintenanceError, EquipmentMaintenanceMaterialError,
    EquipmentMaintenanceOutcome, ValidatedEquipmentMaintenance, validate_equipment_maintenance,
};
pub use maintenance_resolution::{
    EquipmentMaintenanceRequest, EquipmentMaintenanceResolution,
    EquipmentMaintenanceResolutionError, resolve_equipment_maintenance,
};
pub use state::{
    EquipmentId, EquipmentOperationTrace, EquipmentRecord, EquipmentState, EquipmentValidationError,
};
pub use upgrade_execution::{
    EquipmentUpgradeCommitError, EquipmentUpgradeError, ValidatedEquipmentUpgrade,
    validate_upgrade_equipment,
};

pub(crate) use equipment_integration::{ValidatedEquipmentUse, resolve_equipment_capability};
pub(crate) use state::{EquipmentOperationConditionOutcome, validate_loaded_equipment};
