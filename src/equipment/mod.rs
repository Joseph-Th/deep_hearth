//! Owns persistent equipment, condition-adjusted capability, lifecycle operations, and support integration.

mod availability;
mod construction_execution;
mod definitions;
mod disassembly_execution;
mod equipment_integration;
mod equipment_structural_integration;
#[cfg(any(test, feature = "test-gameplay"))]
mod fixture_execution;
mod maintenance_execution;
mod maintenance_resolution;
mod mass_flow_schedule;
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
pub use equipment_integration::{
    EquipmentProviderError, ResolvedEquipmentProvider, project_equipment_capability,
    resolve_available_equipment_provider, resolve_equipment_provider,
};
pub use equipment_structural_integration::{
    EquipmentSupportCommitError, EquipmentSupportError, EquipmentSupportOutcome,
    ValidatedEquipmentSupportChange, validate_mount_equipment, validate_relocate_equipment,
    validate_unmount_equipment,
};
#[cfg(any(test, feature = "test-gameplay"))]
pub(crate) use fixture_execution::add_equipment;
#[cfg(test)]
pub(crate) use fixture_execution::degrade_equipment_condition_for_test;
pub use maintenance_execution::{
    EquipmentMaintenanceCommitError, EquipmentMaintenanceError, EquipmentMaintenanceMaterialError,
    EquipmentMaintenanceOutcome, EquipmentMaintenanceStartOutcome, ValidatedEquipmentMaintenance,
    validate_equipment_maintenance,
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

pub(crate) use availability::{EquipmentOccupancy, equipment_occupancy};
pub(crate) use equipment_integration::{
    ValidatedEquipmentUse, evaluate_equipment_capabilities_at_condition,
    resolve_equipment_capability, resolve_equipment_provider_with_occupancy,
};
pub(crate) use equipment_structural_integration::{
    EquipmentStructuralLoadConsistencyError, validate_existing_equipment_structural_load,
};
pub(crate) use maintenance_execution::{
    apply_equipment_maintenance_tick, decide_equipment_maintenance_tick,
};
pub(crate) use mass_flow_schedule::{
    EquipmentMassFlowResolutionError, EquipmentMassFlowSchedule, EquipmentMassFlowScheduleError,
    resolve_equipment_mass_flow_schedule,
};
pub(crate) use state::{EquipmentOperationConditionOutcome, validate_loaded_equipment};
