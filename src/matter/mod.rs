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
///
/// This is a diagnostic conservation surface, not actor-safe observation. In particular,
/// `geological()` and `total()` include hidden finite geology and must not be used to authorize or
/// choose player actions; actor policy derives geological information only from acquired knowledge.
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

    /// Matter currently owned by durable production jobs or ready-to-claim mining output.
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

impl Error for MatterAccountingError {}

fn sum_masses(
    masses: impl IntoIterator<Item = AggregateMass>,
    overflow: MatterAccountingError,
) -> Result<AggregateMass, MatterAccountingError> {
    masses
        .into_iter()
        .try_fold(AggregateMass::ZERO, |total, mass| {
            total.checked_add(mass).ok_or(overflow)
        })
}

fn calculate_storage_infrastructure_mass(
    state: &AppState,
) -> Result<AggregateMass, MatterAccountingError> {
    sum_masses(
        state
            .inventory()
            .stockpiles()
            .map(|stockpile| AggregateMass::from_mass(stockpile.embodied_mass())),
        MatterAccountingError::StorageInfrastructureMassOverflow,
    )
}

fn calculate_geological_mass(state: &AppState) -> Result<AggregateMass, MatterAccountingError> {
    sum_masses(
        state
            .geology()
            .deposits()
            .map(|deposit| AggregateMass::from_mass(deposit.remaining_mass())),
        MatterAccountingError::GeologicalMassOverflow,
    )
}

fn calculate_structural_mass(state: &AppState) -> Result<AggregateMass, MatterAccountingError> {
    sum_masses(
        state
            .structures()
            .elements()
            .map(|element| AggregateMass::from_mass(element.embodied_mass())),
        MatterAccountingError::StructuralMassOverflow,
    )
}

fn calculate_equipment_mass(state: &AppState) -> Result<AggregateMass, MatterAccountingError> {
    sum_masses(
        state
            .equipment()
            .equipment()
            .map(|record| AggregateMass::from_mass(record.embodied_mass())),
        MatterAccountingError::EquipmentMassOverflow,
    )
}

fn calculate_energy_storage_mass(state: &AppState) -> Result<AggregateMass, MatterAccountingError> {
    sum_masses(
        state
            .energy()
            .stores()
            .map(|record| AggregateMass::from_mass(record.embodied_mass())),
        MatterAccountingError::EnergyStorageMassOverflow,
    )
}

fn calculate_stored_mass(state: &AppState) -> Result<AggregateMass, MatterAccountingError> {
    sum_masses(
        state
            .inventory()
            .lots()
            .map(|lot| AggregateMass::from_mass(lot.mass())),
        MatterAccountingError::StoredMassOverflow,
    )
}

fn calculate_in_process_mass(state: &AppState) -> Result<AggregateMass, MatterAccountingError> {
    let production = state.production().jobs().flat_map(|job| {
        job.output_streams()
            .iter()
            .flat_map(|stream| stream.outputs())
            .map(|output| AggregateMass::from_mass(output.mass()))
    });
    let mining = state
        .mining()
        .jobs()
        .filter(|job| job.is_ready_to_claim())
        .map(|job| AggregateMass::from_mass(job.output().mass()));
    sum_masses(
        production.chain(mining),
        MatterAccountingError::InProcessMassOverflow,
    )
}

fn calculate_consumed_mass(state: &AppState) -> Result<AggregateMass, MatterAccountingError> {
    sum_masses(
        state.survival().consumed_matter().map(|(_, mass)| mass),
        MatterAccountingError::ConsumedMassOverflow,
    )
}

fn calculate_total_mass(parts: &[AggregateMass]) -> Result<AggregateMass, MatterAccountingError> {
    sum_masses(
        parts.iter().copied(),
        MatterAccountingError::TotalMassOverflow,
    )
}

/// Recomputes diagnostic matter ownership from authoritative records without trusting stockpile
/// caches.
///
/// Finite geological deposits own extractable matter until mining completion. Completed mining output
/// is a ready-to-claim owner until claim transfers it into inventory. Structural embodiment remains
/// structure-owned because no demolition/recovery operation exists. Production start transfers input
/// matter from inventory to the durable in-process output snapshot until completion. Reservations and
/// unfinished mining output plans are not matter owners and are excluded from this projection.
/// Because the result includes hidden geological truth, it is suitable for conservation audits and
/// post-hoc diagnostics, not as a player-observation surface.
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
