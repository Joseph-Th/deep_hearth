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
/// `geological()`, `surface()`, and `total()` include world truth that is not globally actor-visible
/// and must not be used to authorize or choose player actions. Actor policy derives geology from
/// acquired knowledge and loose surface matter from exact-local observation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MatterAccounting {
    geological: AggregateMass,
    world: AggregateMass,
    surface: AggregateMass,
    structural: AggregateMass,
    equipment: AggregateMass,
    energy_storage: AggregateMass,
    storage_infrastructure: AggregateMass,
    stored: AggregateMass,
    in_process: AggregateMass,
    consumed: AggregateMass,
    combusted: AggregateMass,
    photosynthetic_input: AggregateMass,
    total: AggregateMass,
}

impl MatterAccounting {
    /// Matter still owned by finite geological deposits.
    #[must_use]
    pub const fn geological(self) -> AggregateMass {
        self.geological
    }

    /// Matter embodied directly in generated block-world cells.
    #[must_use]
    pub const fn world(self) -> AggregateMass {
        self.world
    }

    /// Matter still owned by finite loose surface resources across the complete world state.
    #[must_use]
    pub const fn surface(self) -> AggregateMass {
        self.surface
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

    /// Matter currently owned by durable production jobs or mining output awaiting settlement.
    #[must_use]
    pub const fn in_process(self) -> AggregateMass {
        self.in_process
    }

    /// Food transferred into terminal player and kobold consumption custody.
    /// This is cumulative consumed matter, not live body mass.
    #[must_use]
    pub const fn consumed(self) -> AggregateMass {
        self.consumed
    }

    /// Fuel matter transferred into the terminal combustion-products conservation boundary.
    #[must_use]
    pub const fn combusted(self) -> AggregateMass {
        self.combusted
    }

    /// External atmospheric/environmental mass synthesized into living and subsequently felled wood.
    /// This is a source flux, not another matter owner; it is deducted from conservation totals.
    #[must_use]
    pub const fn photosynthetic_input(self) -> AggregateMass {
        self.photosynthetic_input
    }

    /// Conserved initial-system matter, net of explicit external photosynthetic input.
    #[must_use]
    pub const fn total(self) -> AggregateMass {
        self.total
    }
}

/// Overflow while projecting world-scale matter ownership.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatterAccountingError {
    GeologicalMassOverflow,
    WorldMassOverflow,
    SurfaceMassOverflow,
    StructuralMassOverflow,
    EquipmentMassOverflow,
    EnergyStorageMassOverflow,
    StorageInfrastructureMassOverflow,
    StoredMassOverflow,
    InProcessMassOverflow,
    ConsumedMassOverflow,
    CombustedMassOverflow,
    PhotosyntheticInputOverflow,
    PhotosyntheticInputExceedsPhysicalMass,
    TotalMassOverflow,
}

impl Display for MatterAccountingError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GeologicalMassOverflow => {
                formatter.write_str("geological world matter exceeds aggregate mass range")
            }
            Self::WorldMassOverflow => {
                formatter.write_str("block-world matter exceeds aggregate mass range")
            }
            Self::SurfaceMassOverflow => {
                formatter.write_str("surface-resource world matter exceeds aggregate mass range")
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
            Self::CombustedMassOverflow => {
                formatter.write_str("combusted world matter exceeds aggregate mass range")
            }
            Self::PhotosyntheticInputOverflow => formatter
                .write_str("atmospheric wood assimilation exceeded the supported mass range"),
            Self::PhotosyntheticInputExceedsPhysicalMass => formatter
                .write_str("atmospheric wood assimilation exceeds represented physical matter"),
            Self::TotalMassOverflow => {
                formatter.write_str("total world matter exceeds aggregate mass range")
            }
        }
    }
}

fn calculate_surface_mass(state: &AppState) -> Result<AggregateMass, MatterAccountingError> {
    sum_masses(
        state.surface().resources().map(|resource| {
            AggregateMass::from_mass(
                resource
                    .remaining_mass()
                    .checked_add(resource.perennial_root_mass())
                    .expect("perennial source cannot exceed its initial budget"),
            )
        }),
        MatterAccountingError::SurfaceMassOverflow,
    )
}

fn calculate_world_mass(state: &AppState) -> Result<AggregateMass, MatterAccountingError> {
    sum_masses(
        state
            .world()
            .chunks()
            .flat_map(|chunk| {
                chunk
                    .sections()
                    .map(|section| AggregateMass::from_mass(section.represented_mass()))
            })
            .chain(
                state
                    .world()
                    .trees()
                    .map(|tree| AggregateMass::from_mass(tree.wood_at(state.tick()))),
            ),
        MatterAccountingError::WorldMassOverflow,
    )
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
        .filter(|job| job.is_awaiting_settlement())
        .map(|job| AggregateMass::from_mass(job.output().mass()));
    sum_masses(
        production.chain(mining),
        MatterAccountingError::InProcessMassOverflow,
    )
}

fn calculate_consumed_mass(state: &AppState) -> Result<AggregateMass, MatterAccountingError> {
    sum_masses(
        state
            .survival()
            .consumed_matter()
            .chain(state.kobold_state().consumed_matter())
            .map(|(_, mass)| mass),
        MatterAccountingError::ConsumedMassOverflow,
    )
}

fn calculate_combusted_mass(state: &AppState) -> Result<AggregateMass, MatterAccountingError> {
    sum_masses(
        state.energy().combusted_matter().map(|(_, mass)| mass),
        MatterAccountingError::CombustedMassOverflow,
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
/// awaiting settlement remains mining-owned until the automatic settlement transaction transfers it
/// into inventory. Structural embodiment remains
/// structure-owned because no demolition/recovery operation exists. Production start transfers input
/// matter from inventory to the durable in-process output snapshot until completion. Reservations and
/// unfinished mining output plans are not matter owners and are excluded from this projection.
/// Because the result includes hidden geological truth, it is suitable for conservation audits and
/// post-hoc diagnostics, not as a player-observation surface.
pub fn calculate_matter_accounting(
    state: &AppState,
) -> Result<MatterAccounting, MatterAccountingError> {
    let geological = calculate_geological_mass(state)?;
    let world = calculate_world_mass(state)?;
    let surface = calculate_surface_mass(state)?;
    let structural = calculate_structural_mass(state)?;
    let equipment = calculate_equipment_mass(state)?;
    let energy_storage = calculate_energy_storage_mass(state)?;
    let storage_infrastructure = calculate_storage_infrastructure_mass(state)?;
    let stored = calculate_stored_mass(state)?;
    let in_process = calculate_in_process_mass(state)?;
    let consumed = calculate_consumed_mass(state)?;
    let combusted = calculate_combusted_mass(state)?;
    let photosynthetic_input = state
        .world()
        .assimilated_wood_at(state.tick())
        .ok_or(MatterAccountingError::PhotosyntheticInputOverflow)?;
    let physical_total = calculate_total_mass(&[
        geological,
        world,
        surface,
        structural,
        equipment,
        energy_storage,
        storage_infrastructure,
        stored,
        in_process,
        consumed,
        combusted,
    ])?;
    let total = physical_total
        .checked_sub(photosynthetic_input)
        .ok_or(MatterAccountingError::PhotosyntheticInputExceedsPhysicalMass)?;
    Ok(MatterAccounting {
        geological,
        world,
        surface,
        structural,
        equipment,
        energy_storage,
        storage_infrastructure,
        stored,
        in_process,
        consumed,
        combusted,
        photosynthetic_input,
        total,
    })
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
