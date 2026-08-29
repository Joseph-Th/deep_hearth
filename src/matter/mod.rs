//! Read-only solid/material matter accounting across authoritative non-fluid owners.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::AggregateMass;
use crate::core::state::AppState;

/// World-scale non-fluid matter projection split by its current authoritative owner.
///
/// Finite fluids intentionally use `fluid::calculate_fluid_volume_accounting` instead. Fluid density
/// can imply sub-milligram mass for an exact microliter volume, so folding that owner into this
/// whole-milligram ledger would either lose information or manufacture matter through rounding.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MatterAccounting {
    geological: AggregateMass,
    structural: AggregateMass,
    equipment: AggregateMass,
    energy_storage: AggregateMass,
    storage_infrastructure: AggregateMass,
    stored: AggregateMass,
    in_process: AggregateMass,
    consumed: AggregateMass,
    total: AggregateMass,
}

impl MatterAccounting {
    /// Matter still owned by finite geological deposits.
    #[must_use]
    pub const fn geological(self) -> AggregateMass {
        self.geological
    }

    /// Matter embodied in structural members.
    #[must_use]
    pub const fn structural(self) -> AggregateMass {
        self.structural
    }

    /// Matter embodied in maintainable equipment and tools.
    #[must_use]
    pub const fn equipment(self) -> AggregateMass {
        self.equipment
    }

    /// Matter embodied in finite energy-storage infrastructure.
    #[must_use]
    pub const fn energy_storage(self) -> AggregateMass {
        self.energy_storage
    }

    /// Matter embodied in material-backed inventory storage enclosures.
    #[must_use]
    pub const fn storage_infrastructure(self) -> AggregateMass {
        self.storage_infrastructure
    }

    /// Matter currently owned by inventory lots.
    #[must_use]
    pub const fn stored(self) -> AggregateMass {
        self.stored
    }

    /// Matter currently owned by durable production-job output snapshots.
    #[must_use]
    pub const fn in_process(self) -> AggregateMass {
        self.in_process
    }

    /// Food matter transferred into the terminal survival-consumption conservation boundary.
    /// This is cumulative consumed matter, not live body mass.
    #[must_use]
    pub const fn consumed(self) -> AggregateMass {
        self.consumed
    }

    /// Total non-fluid matter represented by the implemented authoritative matter owners.
    #[must_use]
    pub const fn total(self) -> AggregateMass {
        self.total
    }
}

/// Overflow while projecting world-scale matter ownership.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatterAccountingError {
    GeologicalMassOverflow,
    StructuralMassOverflow,
    EquipmentMassOverflow,
    EnergyStorageMassOverflow,
    StorageInfrastructureMassOverflow,
    StoredMassOverflow,
    InProcessMassOverflow,
    ConsumedMassOverflow,
    TotalMassOverflow,
}

impl Display for MatterAccountingError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GeologicalMassOverflow => {
                formatter.write_str("geological world matter exceeds aggregate mass range")
            }
            Self::StructuralMassOverflow => {
                formatter.write_str("structural world matter exceeds aggregate mass range")
            }
            Self::EquipmentMassOverflow => {
                formatter.write_str("equipment world matter exceeds aggregate mass range")
            }
            Self::EnergyStorageMassOverflow => {
                formatter.write_str("energy-storage world matter exceeds aggregate mass range")
            }
            Self::StorageInfrastructureMassOverflow => formatter
                .write_str("storage-infrastructure world matter exceeds aggregate mass range"),
            Self::StoredMassOverflow => {
                formatter.write_str("stored world matter exceeds aggregate mass range")
            }
            Self::InProcessMassOverflow => {
                formatter.write_str("in-process world matter exceeds aggregate mass range")
            }
            Self::ConsumedMassOverflow => {
                formatter.write_str("consumed world matter exceeds aggregate mass range")
            }
            Self::TotalMassOverflow => {
                formatter.write_str("total world matter exceeds aggregate mass range")
            }
        }
    }
}

fn calculate_storage_infrastructure_mass(
    state: &AppState,
) -> Result<AggregateMass, MatterAccountingError> {
    let mut total = AggregateMass::ZERO;
    for stockpile in state.inventory().stockpiles() {
        total = total
            .checked_add(AggregateMass::from_mass(stockpile.embodied_mass()))
            .ok_or(MatterAccountingError::StorageInfrastructureMassOverflow)?;
    }
    Ok(total)
}

impl Error for MatterAccountingError {}

fn calculate_geological_mass(state: &AppState) -> Result<AggregateMass, MatterAccountingError> {
    let mut total = AggregateMass::ZERO;
    for deposit in state.geology().deposits() {
        total = total
            .checked_add(AggregateMass::from_mass(deposit.remaining_mass()))
            .ok_or(MatterAccountingError::GeologicalMassOverflow)?;
    }
    Ok(total)
}

fn calculate_structural_mass(state: &AppState) -> Result<AggregateMass, MatterAccountingError> {
    let mut total = AggregateMass::ZERO;
    for element in state.structures().elements() {
        total = total
            .checked_add(AggregateMass::from_mass(element.embodied_mass()))
            .ok_or(MatterAccountingError::StructuralMassOverflow)?;
    }
    Ok(total)
}

fn calculate_equipment_mass(state: &AppState) -> Result<AggregateMass, MatterAccountingError> {
    let mut total = AggregateMass::ZERO;
    for record in state.equipment().equipment() {
        total = total
            .checked_add(AggregateMass::from_mass(record.embodied_mass()))
            .ok_or(MatterAccountingError::EquipmentMassOverflow)?;
    }
    Ok(total)
}

fn calculate_energy_storage_mass(state: &AppState) -> Result<AggregateMass, MatterAccountingError> {
    let mut total = AggregateMass::ZERO;
    for record in state.energy().stores() {
        total = total
            .checked_add(AggregateMass::from_mass(record.embodied_mass()))
            .ok_or(MatterAccountingError::EnergyStorageMassOverflow)?;
    }
    Ok(total)
}

fn calculate_stored_mass(state: &AppState) -> Result<AggregateMass, MatterAccountingError> {
    let mut total = AggregateMass::ZERO;
    for lot in state.inventory().lots() {
        total = total
            .checked_add(AggregateMass::from_mass(lot.mass()))
            .ok_or(MatterAccountingError::StoredMassOverflow)?;
    }
    Ok(total)
}

fn calculate_in_process_mass(state: &AppState) -> Result<AggregateMass, MatterAccountingError> {
    let mut total = AggregateMass::ZERO;
    for job in state.production().jobs() {
        for stream in job.output_streams() {
            for output in stream.outputs() {
                total = total
                    .checked_add(AggregateMass::from_mass(output.mass()))
                    .ok_or(MatterAccountingError::InProcessMassOverflow)?;
            }
        }
    }
    for job in state.mining().jobs().filter(|job| job.is_ready_to_claim()) {
        total = total
            .checked_add(AggregateMass::from_mass(job.output().mass()))
            .ok_or(MatterAccountingError::InProcessMassOverflow)?;
    }
    Ok(total)
}

fn calculate_consumed_mass(state: &AppState) -> Result<AggregateMass, MatterAccountingError> {
    let mut total = AggregateMass::ZERO;
    for (_, mass) in state.survival().consumed_matter() {
        total = total
            .checked_add(mass)
            .ok_or(MatterAccountingError::ConsumedMassOverflow)?;
    }
    Ok(total)
}

fn calculate_total_mass(parts: &[AggregateMass]) -> Result<AggregateMass, MatterAccountingError> {
    parts
        .iter()
        .copied()
        .try_fold(AggregateMass::ZERO, |total, part| {
            total
                .checked_add(part)
                .ok_or(MatterAccountingError::TotalMassOverflow)
        })
}

/// Recomputes matter ownership from authoritative records without trusting stockpile caches.
///
/// Finite geological deposits own their remaining extractable matter through active mining labor.
/// Mining completion transfers the exact batch into a zero-time ready-to-claim owner, and claim then
/// transfers it into inventory. Fixture materialization can move selected inventory matter into
/// structural embodiment, where it remains authoritative structural matter because runtime
/// demolition/recovery is not currently modeled. Production inputs are removed from inventory at
/// process start. The running production job's resolved output snapshot becomes the durable owner of
/// that same matter until completion. Reserved inbound capacity and working mining output plans are
/// not additional matter and are deliberately excluded from this projection.
pub fn calculate_matter_accounting(
    state: &AppState,
) -> Result<MatterAccounting, MatterAccountingError> {
    let geological = calculate_geological_mass(state)?;
    let structural = calculate_structural_mass(state)?;
    let equipment = calculate_equipment_mass(state)?;
    let energy_storage = calculate_energy_storage_mass(state)?;
    let storage_infrastructure = calculate_storage_infrastructure_mass(state)?;
    let stored = calculate_stored_mass(state)?;
    let in_process = calculate_in_process_mass(state)?;
    let consumed = calculate_consumed_mass(state)?;
    let total = calculate_total_mass(&[
        geological,
        structural,
        equipment,
        energy_storage,
        storage_infrastructure,
        stored,
        in_process,
        consumed,
    ])?;
    Ok(MatterAccounting {
        geological,
        structural,
        equipment,
        energy_storage,
        storage_infrastructure,
        stored,
        in_process,
        consumed,
        total,
    })
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
