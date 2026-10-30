//! Material-built, one-cell timber rollers between finite mechanical accumulators.
//!
//! The completed structural member is the physical connection. No free global
//! bus, inventory teleportation, or unpersisted cable topology exists.

use std::collections::BTreeSet;

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
use crate::structural::{StructuralElementId, StructuralLifecycle};

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

fn plan_pair(
    registries: &Registries,
    state: &AppState,
    passive: &PassiveEnergyDissipationPlan,
    coupling: StructuralElementId,
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
    let intake = per_tick
        .nanojoules()
        .min(equalizing_ceiling)
        .min(source_stored.nanojoules())
        .min(headroom.nanojoules());
    let delivered =
        scale_u128_fraction_floor(intake, WOODEN_TRANSMISSION_EFFICIENCY_PPM, 1_000_000);
    if delivered == 0 {
        return None;
    }
    Some(MechanicalTransmissionReceipt {
        coupling,
        source,
        destination,
        drawn: Energy::from_nanojoules(intake),
        delivered: Energy::from_nanojoules(delivered),
    })
}

fn plan_coupling_axis(
    registries: &Registries,
    state: &AppState,
    passive: &PassiveEnergyDissipationPlan,
    coupling: StructuralElementId,
    middle: VoxelCoord,
    dx: i64,
    dz: i64,
    used: &mut BTreeSet<EnergyStoreId>,
    plan: &mut MechanicalTransmissionPlan,
) {
    let (Some(low_x), Some(low_z), Some(high_x), Some(high_z)) = (
        middle.x().checked_sub(dx),
        middle.z().checked_sub(dz),
        middle.x().checked_add(dx),
        middle.z().checked_add(dz),
    ) else {
        return;
    };
    let low = VoxelCoord::new(low_x, middle.y(), low_z);
    let high = VoxelCoord::new(high_x, middle.y(), high_z);
    for left in state.logistics().energy_stores_at(low) {
        if used.contains(&left) {
            continue;
        }
        for right in state.logistics().energy_stores_at(high) {
            if used.contains(&right) {
                continue;
            }
            if let Some(transfer) = plan_pair(registries, state, passive, coupling, left, right) {
                used.insert(left);
                used.insert(right);
                plan.transfers.push(transfer);
                break;
            }
        }
    }
}

/// An active, embodied coupling at the middle cell connects two mechanical
/// stores on opposed horizontal sides. A shaft can serve either X or Z;
/// each endpoint takes at most one transfer per tick, so chains advance in
/// successive ticks rather than creating free instantaneous infinite buses.
pub(crate) fn decide_mechanical_transmission(
    registries: &Registries,
    state: &AppState,
    passive: &PassiveEnergyDissipationPlan,
    dismantled_this_tick: Option<StructuralElementId>,
) -> MechanicalTransmissionPlan {
    let mut plan = MechanicalTransmissionPlan::default();
    let mut used = BTreeSet::new();
    for member in state.structures().elements() {
        if member.profile() != STRUCTURAL_PROFILE_TIMBER_DRIVE_COUPLING
            || dismantled_this_tick == Some(member.id())
            || member.material() != MATERIAL_WOOD
            || !member.is_grounded()
            || member.length() != BASIC_TIMBER_DRIVE_COUPLING_LENGTH
            || member.cross_section() != BASIC_TIMBER_DRIVE_COUPLING_CROSS_SECTION
            || member.bounds().voxel_count() != Some(1)
            || member.lifecycle() != StructuralLifecycle::Active
            || member.is_cracked()
            || member.embodied_material().is_empty()
        {
            continue;
        }
        let middle = member.bounds().min();
        for (dx, dz) in [(1_i64, 0_i64), (0, 1)] {
            plan_coupling_axis(
                registries,
                state,
                passive,
                member.id(),
                middle,
                dx,
                dz,
                &mut used,
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
