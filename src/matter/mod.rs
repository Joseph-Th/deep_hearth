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

impl Error for MatterAccountingError {}

/// Recomputes matter ownership from authoritative records without trusting stockpile caches.
///
/// Finite geological deposits own their remaining extractable matter until a canonical extraction
/// transfers it into inventory. Construction moves selected inventory matter into structural
/// embodiment until conserved deconstruction returns or transforms it. Production inputs are removed
/// from inventory at process start. The running job's resolved output snapshot becomes the durable
/// owner of that same matter until completion. Reserved inbound capacity is not additional matter and
/// is deliberately
/// excluded from this projection.
pub fn calculate_matter_accounting(
    state: &AppState,
) -> Result<MatterAccounting, MatterAccountingError> {
    let mut geological = AggregateMass::ZERO;
    for deposit in state.geology().deposits() {
        geological = geological
            .checked_add(AggregateMass::from_mass(deposit.remaining_mass()))
            .ok_or(MatterAccountingError::GeologicalMassOverflow)?;
    }

    let mut structural = AggregateMass::ZERO;
    for element in state.structures().elements() {
        structural = structural
            .checked_add(AggregateMass::from_mass(element.embodied_mass()))
            .ok_or(MatterAccountingError::StructuralMassOverflow)?;
    }

    let mut equipment = AggregateMass::ZERO;
    for record in state.equipment().equipment() {
        equipment = equipment
            .checked_add(AggregateMass::from_mass(record.embodied_mass()))
            .ok_or(MatterAccountingError::EquipmentMassOverflow)?;
    }

    let mut energy_storage = AggregateMass::ZERO;
    for record in state.energy().stores() {
        energy_storage = energy_storage
            .checked_add(AggregateMass::from_mass(record.embodied_mass()))
            .ok_or(MatterAccountingError::EnergyStorageMassOverflow)?;
    }

    let mut stored = AggregateMass::ZERO;
    for lot in state.inventory().lots() {
        stored = stored
            .checked_add(AggregateMass::from_mass(lot.mass()))
            .ok_or(MatterAccountingError::StoredMassOverflow)?;
    }

    let mut in_process = AggregateMass::ZERO;
    for job in state.production().jobs() {
        for stream in job.output_streams() {
            for output in stream.outputs() {
                in_process = in_process
                    .checked_add(AggregateMass::from_mass(output.mass()))
                    .ok_or(MatterAccountingError::InProcessMassOverflow)?;
            }
        }
    }
    for job in state.mining().jobs() {
        in_process = in_process
            .checked_add(AggregateMass::from_mass(job.output().mass()))
            .ok_or(MatterAccountingError::InProcessMassOverflow)?;
    }

    let mut consumed = AggregateMass::ZERO;
    for (_, mass) in state.survival().consumed_matter() {
        consumed = consumed
            .checked_add(mass)
            .ok_or(MatterAccountingError::ConsumedMassOverflow)?;
    }

    let total = geological
        .checked_add(structural)
        .ok_or(MatterAccountingError::TotalMassOverflow)?
        .checked_add(equipment)
        .ok_or(MatterAccountingError::TotalMassOverflow)?
        .checked_add(energy_storage)
        .ok_or(MatterAccountingError::TotalMassOverflow)?
        .checked_add(stored)
        .ok_or(MatterAccountingError::TotalMassOverflow)?
        .checked_add(in_process)
        .ok_or(MatterAccountingError::TotalMassOverflow)?
        .checked_add(consumed)
        .ok_or(MatterAccountingError::TotalMassOverflow)?;
    Ok(MatterAccounting {
        geological,
        structural,
        equipment,
        energy_storage,
        stored,
        in_process,
        consumed,
        total,
    })
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
