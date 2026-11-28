//! Immutable cross-owner references needed to validate one mining job.

use crate::core::quantity::{Mass, Pressure, Temperature};
use crate::core::state::AppState;
use crate::equipment::EquipmentDefinition;
use crate::inventory::StockpileRecord;
use crate::material::{CommodityKey, MaterialComposition};
use crate::registry::Registries;

use super::super::MiningJobValidationError;
use crate::mining::{MiningJobRecord, MiningMethodDefinition};

pub(super) struct MiningJobReferences<'state> {
    pub(super) method: &'state MiningMethodDefinition,
    pub(super) destination: &'state StockpileRecord,
    pub(super) equipment_definition: &'state EquipmentDefinition,
    pub(super) deposit_commodity: CommodityKey,
    pub(super) deposit_temperature: Temperature,
    pub(super) deposit_composition: &'state MaterialComposition,
    pub(super) deposit_remaining_mass: Mass,
    pub(super) excavation_hardness: Pressure,
}

pub(super) fn resolve_mining_job_references<'state>(
    registries: &'state Registries,
    state: &'state AppState,
    job: &MiningJobRecord,
) -> Result<MiningJobReferences<'state>, MiningJobValidationError> {
    let method = registries
        .mining()
        .get_method(job.method())
        .ok_or(MiningJobValidationError::UnknownMethod { job: job.id() })?;
    let deposit = state
        .geology()
        .get_deposit(job.deposit())
        .ok_or(MiningJobValidationError::UnknownDeposit { job: job.id() })?;
    let destination = state
        .inventory()
        .get_stockpile(job.destination())
        .ok_or(MiningJobValidationError::UnknownDestination { job: job.id() })?;
    let equipment_definition = registries
        .equipment()
        .get_equipment(job.equipment_definition())
        .ok_or(MiningJobValidationError::UnknownEquipmentDefinition {
            job: job.id(),
            definition: job.equipment_definition(),
        })?;
    Ok(MiningJobReferences {
        method,
        destination,
        equipment_definition,
        deposit_commodity: deposit.commodity(),
        deposit_temperature: deposit.temperature(),
        deposit_composition: deposit.composition(),
        deposit_remaining_mass: deposit.remaining_mass(),
        excavation_hardness: deposit.excavation_hardness(),
    })
}
