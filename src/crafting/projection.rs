//! Read-only physical projection for authored equipment-assisted manual crafting.

use std::num::NonZeroU64;

use crate::core::quantity::Mass;
use crate::core::time::TickSpan;
use crate::equipment::EquipmentDefinitionId;
use crate::maintenance::Condition;
use crate::production::ProcessId;
use crate::registry::Registries;

use super::errors::ManualCraftEquipmentProjectionError;
use super::physics::{
    ManualCraftEquipmentResolutionError, ManualCraftEquipmentScheduleError,
    resolve_manual_craft_equipment_physics,
};

/// Physical schedule projected from authored manual-craft and equipment definitions.
///
/// This is planning evidence only. Runtime authorization must still use the normal manual-craft
/// resolver and admission path so inventory, provider availability/support, survival, and stale
/// revisions are validated against current state.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManualCraftEquipmentProjection {
    duration: TickSpan,
    condition_after: Condition,
}

impl ManualCraftEquipmentProjection {
    #[must_use]
    pub const fn duration(self) -> TickSpan {
        self.duration
    }

    #[must_use]
    pub const fn condition_after(self) -> Condition {
        self.condition_after
    }
}

/// Projects the canonical equipment-assisted schedule for an authored future provider.
///
/// The equipment definition and starting condition are explicit planning inputs. This function
/// intentionally does not inspect runtime equipment state and therefore cannot authorize work.
pub fn project_manual_craft_equipment(
    registries: &Registries,
    process: ProcessId,
    batches: NonZeroU64,
    equipment: EquipmentDefinitionId,
    condition: Condition,
) -> Result<ManualCraftEquipmentProjection, ManualCraftEquipmentProjectionError> {
    let definition = registries
        .crafting()
        .get_manual(process)
        .ok_or(ManualCraftEquipmentProjectionError::UnknownManualProcess { process })?;
    let profile = definition
        .equipment_profile()
        .ok_or(ManualCraftEquipmentProjectionError::EquipmentNotSupported { process })?;
    let equipment_definition = registries
        .equipment()
        .get_equipment(equipment)
        .ok_or(ManualCraftEquipmentProjectionError::UnknownEquipmentDefinition { equipment })?;
    let input_mass = Mass::from_milligrams(
        definition
            .input_mass()
            .milligrams()
            .checked_mul(batches.get())
            .ok_or(ManualCraftEquipmentProjectionError::InputMassOverflow { process, batches })?,
    );
    let schedule = resolve_manual_craft_equipment_physics(
        equipment_definition,
        condition,
        profile.mass_flow_capability(),
        input_mass,
        registries.core().physical_tick_duration(),
        profile.condition_wear_ppm_per_active_tick(),
    )
    .map_err(|error| match error {
        ManualCraftEquipmentResolutionError::MissingCapability { capability } => {
            ManualCraftEquipmentProjectionError::MissingEquipmentCapability {
                equipment,
                capability,
            }
        }
        ManualCraftEquipmentResolutionError::CapabilityKindMismatch { capability, found } => {
            ManualCraftEquipmentProjectionError::EquipmentCapabilityKindMismatch {
                equipment,
                capability,
                found,
            }
        }
        ManualCraftEquipmentResolutionError::Schedule(error) => match error {
            ManualCraftEquipmentScheduleError::Duration(error) => {
                ManualCraftEquipmentProjectionError::EquipmentDuration(error)
            }
            ManualCraftEquipmentScheduleError::Condition(error) => {
                ManualCraftEquipmentProjectionError::EquipmentCondition(error)
            }
        },
    })?;
    Ok(ManualCraftEquipmentProjection {
        duration: schedule.duration(),
        condition_after: schedule.condition_after(),
    })
}
