//! Material-built straight timber line shafts between finite mechanical accumulators.
//!
//! Every active, supported roller is physical transmission. Runs of contiguous
//! roller cells may connect opposed mechanical stores with one friction stage
//! per roller. No free global bus or invisible persistent cable topology exists.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};

use crate::content::{
    BASIC_TIMBER_DRIVE_COUPLING_CROSS_SECTION, BASIC_TIMBER_DRIVE_COUPLING_LENGTH, MATERIAL_WOOD,
    STRUCTURAL_PROFILE_TIMBER_DRIVE_COUPLING,
};
use crate::core::quantity::Energy;
use crate::core::state::AppState;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::structural::{
    StructuralElementId, StructuralElementRecord, StructuralLifecycle, StructureState,
};

use super::EnergyStoreId;
use super::passive_dissipation::PassiveEnergyDissipationPlan;

/// Visible transfer and loss at the shaft on one physical tick.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MechanicalTransmissionReceipt {
    coupling: StructuralElementId,
    segments: u32,
    source: EnergyStoreId,
    destination: EnergyStoreId,
    drawn: Energy,
    delivered: Energy,
}

impl MechanicalTransmissionReceipt {
    #[must_use]
    pub const fn coupling(self) -> StructuralElementId {
        self.coupling
    }
    /// Number of material-built rollers that transmitted this work.
    #[must_use]
    pub const fn segments(self) -> u32 {
        self.segments
    }
    #[must_use]
    pub const fn source(self) -> EnergyStoreId {
        self.source
    }
    #[must_use]
    pub const fn destination(self) -> EnergyStoreId {
        self.destination
    }
    #[must_use]
    pub const fn drawn(self) -> Energy {
        self.drawn
    }
    #[must_use]
    pub const fn delivered(self) -> Energy {
        self.delivered
    }
    #[must_use]
    pub fn lost(self) -> Energy {
        self.drawn
            .checked_sub(self.delivered)
            .expect("shaft delivery cannot exceed intake")
    }
}

#[derive(Default)]
pub(crate) struct MechanicalTransmissionPlan {
    transfers: Vec<MechanicalTransmissionReceipt>,
}

impl MechanicalTransmissionPlan {
    pub(crate) fn energy_revision_steps(&self) -> u64 {
        u64::from(!self.transfers.is_empty())
    }
}

mod physics;
use physics::plan_pair;
#[cfg(test)]
use physics::{
    bounded_shaft_transfer, compare_charge, delivered_after_rollers, equalizing_shaft_draw,
    minimum_draw_for_delivery,
};

/// The rollers must remain complete, grounded, material-backed and sound.
fn is_usable_roller(
    member: &StructuralElementRecord,
    dismantled_this_tick: Option<StructuralElementId>,
) -> bool {
    member.profile() == STRUCTURAL_PROFILE_TIMBER_DRIVE_COUPLING
        && dismantled_this_tick != Some(member.id())
        && member.material() == MATERIAL_WOOD
        && member.is_grounded()
        && member.length() == BASIC_TIMBER_DRIVE_COUPLING_LENGTH
        && member.cross_section() == BASIC_TIMBER_DRIVE_COUPLING_CROSS_SECTION
        && member.bounds().voxel_count() == Some(1)
        && member.lifecycle() == StructuralLifecycle::Active
        && !member.is_cracked()
        && !member.embodied_material().is_empty()
}

/// Narrow embodied structural members may legitimately overlap in one voxel.
/// Without an authored shaft axis or separate bearing positions, two rollers
/// in the same cell cannot establish one unambiguous continuous drive. Do not
/// silently select whichever ID happened to be inserted last.
fn unique_roller_cells(
    rollers: impl IntoIterator<Item = (VoxelCoord, StructuralElementId)>,
) -> BTreeMap<VoxelCoord, StructuralElementId> {
    let mut unique = BTreeMap::new();
    let mut disputed = BTreeSet::new();
    for (position, id) in rollers {
        if disputed.contains(&position) {
            continue;
        }
        match unique.entry(position) {
            Entry::Vacant(slot) => {
                slot.insert(id);
            }
            Entry::Occupied(slot) => {
                slot.remove();
                disputed.insert(position);
            }
        }
    }
    unique
}

/// A straight shaft can advance through a new cell only when that cell is
/// represented by one intact ground-supported timber roller.
fn shaft_neighbor(position: VoxelCoord, dx: i64, dz: i64, forward: bool) -> Option<VoxelCoord> {
    let (x, z) = if forward {
        (position.x().checked_add(dx)?, position.z().checked_add(dz)?)
    } else {
        (position.x().checked_sub(dx)?, position.z().checked_sub(dz)?)
    };
    Some(VoxelCoord::new(x, position.y(), z))
}

/// Returns exactly one maximal straight roller run and the external endpoint
/// voxels. Interior starts are excluded, so each segment is scanned at most
/// twice across horizontal axes and topology cost scales with built matter.
fn discover_shaft_run(
    rollers: &BTreeMap<VoxelCoord, StructuralElementId>,
    first: VoxelCoord,
    dx: i64,
    dz: i64,
) -> Option<(Vec<StructuralElementId>, VoxelCoord, VoxelCoord)> {
    let low = shaft_neighbor(first, dx, dz, false)?;
    if rollers.contains_key(&low) {
        return None;
    }
    let mut path = vec![*rollers.get(&first)?];
    let mut current = first;
    loop {
        let next = shaft_neighbor(current, dx, dz, true)?;
        match rollers.get(&next) {
            Some(id) => {
                path.push(*id);
                current = next;
            }
            None => return Some((path, low, next)),
        }
    }
}

mod planner;
pub(crate) use planner::decide_mechanical_transmission;

pub(crate) fn apply_mechanical_transmission(
    state: &mut AppState,
    plan: MechanicalTransmissionPlan,
) -> Vec<MechanicalTransmissionReceipt> {
    if plan.transfers.is_empty() {
        return Vec::new();
    }
    let next_revision = state
        .energy()
        .revision()
        .checked_add(1)
        .expect("prevalidated transmission energy revision exhausted");
    let energy = state.energy_state_mut();
    for transfer in &plan.transfers {
        energy.transfer_mechanical_energy(
            transfer.source,
            transfer.destination,
            transfer.drawn,
            transfer.delivered,
        );
    }
    energy.apply_revision(next_revision);
    plan.transfers
}

#[cfg(test)]
#[path = "mechanical_transmission_tests.rs"]
mod tests;
