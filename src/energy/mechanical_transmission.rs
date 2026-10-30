//! Material-built straight timber line shafts between finite mechanical accumulators.
//!
//! Every active, supported roller is physical transmission. Runs of contiguous
//! roller cells may connect opposed mechanical stores with one friction stage
//! per roller. No free global bus or invisible persistent cable topology exists.

use std::collections::{BTreeMap, BTreeSet};

use crate::content::{
    BASIC_TIMBER_DRIVE_COUPLING_CROSS_SECTION, BASIC_TIMBER_DRIVE_COUPLING_LENGTH, MATERIAL_WOOD,
    STRUCTURAL_PROFILE_TIMBER_DRIVE_COUPLING,
};
use crate::core::arithmetic::{checked_mul_div_with_remainder, scale_u128_fraction_floor};
use crate::core::quantity::{Energy, Power};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::structural::{StructuralElementId, StructuralElementRecord, StructuralLifecycle};

use super::integration::PowerRemainder;
use super::passive_dissipation::PassiveEnergyDissipationPlan;
use super::{EnergyCarrier, EnergyStoreId, energy_store_occupancy, integrate_power};

/// A rough wooden drive loses work in bearings, flex, and misalignment.
const WOODEN_TRANSMISSION_EFFICIENCY_PPM: u32 = 800_000;
/// Wooden coupling is a low-power beginning, not a steel factory shaft.
const WOODEN_TRANSMISSION_POWER: Power = Power::from_microwatts(50_000_000);
const FRACTION_SCALE: u128 = 1_000_000;

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

fn charge_ppm(available: Energy, capacity: Energy) -> u128 {
    checked_mul_div_with_remainder(
        available.nanojoules(),
        FRACTION_SCALE,
        capacity.nanojoules(),
        0,
    )
    .expect("finite store has a nonzero capacity and bounded charge")
    .0
}

fn delivered_after_rollers(mut drawn: u128, segments: u32) -> u128 {
    for _ in 0..segments {
        drawn = scale_u128_fraction_floor(drawn, WOODEN_TRANSMISSION_EFFICIENCY_PPM, 1_000_000);
        if drawn == 0 {
            break;
        }
    }
    drawn
}

/// Return an exact nonzero draw and delivery bounded by the sink's physical capacity.
/// Usually the source/power/equalization ceiling already fits. Only when friction-adjusted
/// delivery exceeds sink headroom do we search for the smallest draw that fills it,
/// avoiding both overflow and unnecessary work lost to fractional rounding.
fn bounded_shaft_transfer(
    maximum_draw: u128,
    headroom: u128,
    segments: u32,
) -> Option<(u128, u128)> {
    if maximum_draw == 0 || headroom == 0 {
        return None;
    }
    let maximum_delivery = delivered_after_rollers(maximum_draw, segments);
    if maximum_delivery == 0 {
        return None;
    }
    if maximum_delivery <= headroom {
        return Some((maximum_draw, maximum_delivery));
    }
    // Efficiency is below unity at every roller. An extra unit of input can raise
    // output by at most one unit, so all positive deliveries up to the maximum
    // are reachable. This search cannot overflow even at the full u128 width.
    let (mut insufficient, mut sufficient) = (0_u128, maximum_draw);
    while sufficient - insufficient > 1 {
        let middle = insufficient + (sufficient - insufficient) / 2;
        if delivered_after_rollers(middle, segments) >= headroom {
            sufficient = middle;
        } else {
            insufficient = middle;
        }
    }
    debug_assert_eq!(delivered_after_rollers(sufficient, segments), headroom);
    Some((sufficient, headroom))
}

fn plan_pair(
    registries: &Registries,
    state: &AppState,
    passive: &PassiveEnergyDissipationPlan,
    coupling: StructuralElementId,
    segments: u32,
    left: EnergyStoreId,
    right: EnergyStoreId,
) -> Option<MechanicalTransmissionReceipt> {
    if left == right
        || energy_store_occupancy(state, left).is_some()
        || energy_store_occupancy(state, right).is_some()
    {
        return None;
    }
    let a = state.energy().get_store(left)?;
    let b = state.energy().get_store(right)?;
    let da = registries.energy().get_store(a.definition())?;
    let db = registries.energy().get_store(b.definition())?;
    if da.carrier() != EnergyCarrier::Mechanical || db.carrier() != EnergyCarrier::Mechanical {
        return None;
    }
    // Passive windage owns pre-tick losses. Only energy left *after* that
    // unavoidable loss may be transmitted; producer ingress can add more
    // later, but must not be borrowed from this decision's snapshot.
    let a_remaining = a.stored().checked_sub(passive.dissipated_for_store(left))?;
    let b_remaining = b
        .stored()
        .checked_sub(passive.dissipated_for_store(right))?;
    let fill_a = charge_ppm(a_remaining, da.capacity());
    let fill_b = charge_ppm(b_remaining, db.capacity());
    let (source, destination, source_def, dest_def, source_stored, dest_stored, excess_ppm) =
        if fill_a > fill_b {
            (
                left,
                right,
                da,
                db,
                a_remaining,
                b_remaining,
                fill_a - fill_b,
            )
        } else if fill_b > fill_a {
            (
                right,
                left,
                db,
                da,
                b_remaining,
                a_remaining,
                fill_b - fill_a,
            )
        } else {
            return None;
        };
    let power = [
        WOODEN_TRANSMISSION_POWER,
        source_def.max_output_power(),
        dest_def.max_input_power(),
    ]
    .into_iter()
    .min()?;
    if power.is_zero() {
        return None;
    }
    let per_tick = integrate_power(
        power,
        TickSpan::new(1),
        registries.core().physical_tick_duration(),
        PowerRemainder::ZERO,
    )
    .ok()?
    .energy();
    // A conservative normalized equalization bound prevents power ping-pong.
    let equalizing_ceiling = checked_mul_div_with_remainder(
        source_def.capacity().min(dest_def.capacity()).nanojoules(),
        excess_ppm,
        FRACTION_SCALE * 2,
        0,
    )?
    .0;
    let headroom = dest_def.capacity().checked_sub(dest_stored)?;
    let maximum_draw = per_tick
        .nanojoules()
        .min(equalizing_ceiling)
        .min(source_stored.nanojoules());
    // Each embodied roller loses real work. Headroom bounds the resulting
    // delivery, not the larger draw before friction.
    let (intake, delivered) =
        bounded_shaft_transfer(maximum_draw, headroom.nanojoules(), segments)?;
    Some(MechanicalTransmissionReceipt {
        coupling,
        segments,
        source,
        destination,
        drawn: Energy::from_nanojoules(intake),
        delivered: Energy::from_nanojoules(delivered),
    })
}

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

/// Binds one transfer to one physical run. A roller cannot simultaneously
/// serve two powered crosses, and a store cannot participate in two transfers
/// in a tick. Stable endpoint identity determines one eligible pair.
fn plan_coupling_axis(
    registries: &Registries,
    state: &AppState,
    passive: &PassiveEnergyDissipationPlan,
    rollers: &BTreeMap<VoxelCoord, StructuralElementId>,
    first: VoxelCoord,
    dx: i64,
    dz: i64,
    used_stores: &mut BTreeSet<EnergyStoreId>,
    used_rollers: &mut BTreeSet<StructuralElementId>,
    plan: &mut MechanicalTransmissionPlan,
) {
    let Some((path, low, high)) = discover_shaft_run(rollers, first, dx, dz) else {
        return;
    };
    if path.iter().any(|id| used_rollers.contains(id)) {
        return;
    }
    let Ok(segments) = u32::try_from(path.len()) else {
        return;
    };
    for left in state.logistics().energy_stores_at(low) {
        if used_stores.contains(&left) {
            continue;
        }
        for right in state.logistics().energy_stores_at(high) {
            if used_stores.contains(&right) {
                continue;
            }
            if let Some(transfer) =
                plan_pair(registries, state, passive, path[0], segments, left, right)
            {
                used_stores.insert(left);
                used_stores.insert(right);
                used_rollers.extend(path);
                plan.transfers.push(transfer);
                return;
            }
        }
    }
}

/// Active embodied rollers form exact straight physical shaft runs. Only
/// opposed endpoint stores receive work. There is no T-junction splitter or
/// virtual bus: each store and each roller settles at most one flow per tick.
pub(crate) fn decide_mechanical_transmission(
    registries: &Registries,
    state: &AppState,
    passive: &PassiveEnergyDissipationPlan,
    dismantled_this_tick: Option<StructuralElementId>,
) -> MechanicalTransmissionPlan {
    let rollers = state
        .structures()
        .elements()
        .filter(|member| is_usable_roller(member, dismantled_this_tick))
        .map(|member| (member.bounds().min(), member.id()))
        .collect::<BTreeMap<_, _>>();
    let mut plan = MechanicalTransmissionPlan::default();
    let mut used_stores = BTreeSet::new();
    let mut used_rollers = BTreeSet::new();
    for position in rollers.keys().copied() {
        for (dx, dz) in [(1_i64, 0_i64), (0, 1)] {
            plan_coupling_axis(
                registries,
                state,
                passive,
                &rollers,
                position,
                dx,
                dz,
                &mut used_stores,
                &mut used_rollers,
                &mut plan,
            );
        }
    }
    plan
}

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
