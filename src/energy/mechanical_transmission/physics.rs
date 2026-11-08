//! Exact finite-work transfer, roller loss, and charge equalization for timber shafts.

use std::cmp::Ordering;

use crate::core::arithmetic::{
    checked_mul_div_ceil, checked_mul_div_with_remainder, scale_u128_fraction_floor,
};
use crate::core::quantity::{Energy, Power};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::registry::Registries;
use crate::structural::StructuralElementId;

use super::super::integration::PowerRemainder;
use super::super::passive_dissipation::PassiveEnergyDissipationPlan;
use super::super::{EnergyCarrier, EnergyStoreId, energy_store_occupancy, integrate_power};
use super::MechanicalTransmissionReceipt;

/// A rough wooden drive loses work in bearings, flex, and misalignment.
const WOODEN_TRANSMISSION_EFFICIENCY_PPM: u32 = 800_000;
/// Wooden coupling is a low-power beginning, not a steel factory shaft.
const WOODEN_TRANSMISSION_POWER: Power = Power::from_microwatts(50_000_000);

/// Compare exact fractions without forming a potentially overflowing cross product.
/// Charge is stored energy divided by the store's authored capacity.
pub(super) fn compare_charge(
    left: u128,
    left_capacity: u128,
    right: u128,
    right_capacity: u128,
) -> Ordering {
    let (whole, remainder) = checked_mul_div_with_remainder(left, right_capacity, left_capacity, 0)
        .expect("validated stores have nonzero capacity and bounded charge");
    let ordering = whole.cmp(&right);
    if ordering == Ordering::Equal && remainder != 0 {
        Ordering::Greater
    } else {
        ordering
    }
}

pub(super) fn delivered_after_rollers(mut drawn: u128, segments: u32) -> u128 {
    for _ in 0..segments {
        drawn = scale_u128_fraction_floor(drawn, WOODEN_TRANSMISSION_EFFICIENCY_PPM, 1_000_000);
        if drawn == 0 {
            break;
        }
    }
    drawn
}

/// Invert each lossy stage in reverse order to find the smallest physical
/// input producing at least the chosen whole-nanojoule delivery. The ceiling
/// is the exact inverse of a stage's floored efficiency. This avoids spending
/// source energy that integer quantization would only dissipate without
/// increasing the sink's received work.
pub(super) fn minimum_draw_for_delivery(mut delivered: u128, segments: u32) -> u128 {
    for _ in 0..segments {
        delivered = checked_mul_div_ceil(
            delivered,
            1_000_000,
            u128::from(WOODEN_TRANSMISSION_EFFICIENCY_PPM),
        )
        .expect("reachable output has a representable minimum input at every roller");
    }
    delivered
}

/// Return the smallest physical draw that delivers the greatest whole work
/// allowed by the input and sink headroom. Inverting each roller's floored
/// efficiency avoids a full-width binary search and cannot overfill the sink.
pub(super) fn bounded_shaft_transfer(
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
    let delivery = maximum_delivery.min(headroom);
    let draw = minimum_draw_for_delivery(delivery, segments);
    debug_assert!(draw <= maximum_draw);
    debug_assert_eq!(delivered_after_rollers(draw, segments), delivery);
    Some((draw, delivery))
}

/// The source may not end less charged than the destination. Roller friction
/// makes delivered work a stepwise function of intake, so a nominal percentage
/// difference is not a safe equalization bound. Binary search the largest
/// representable draw that respects the exact post-transfer charge ordering.
pub(super) fn equalizing_shaft_draw(
    maximum_draw: u128,
    segments: u32,
    source_stored: u128,
    source_capacity: u128,
    destination_stored: u128,
    destination_capacity: u128,
) -> Option<(u128, u128)> {
    let preserves_order = |draw: u128| {
        let Some(source_after) = source_stored.checked_sub(draw) else {
            return false;
        };
        let Some(destination_after) =
            destination_stored.checked_add(delivered_after_rollers(draw, segments))
        else {
            return false;
        };
        destination_after <= destination_capacity
            && compare_charge(
                source_after,
                source_capacity,
                destination_after,
                destination_capacity,
            ) != Ordering::Less
    };
    let draw = if preserves_order(maximum_draw) {
        maximum_draw
    } else {
        let (mut permissible, mut excessive) = (0_u128, maximum_draw);
        while excessive - permissible > 1 {
            let middle = permissible + (excessive - permissible) / 2;
            if preserves_order(middle) {
                permissible = middle;
            } else {
                excessive = middle;
            }
        }
        permissible
    };
    let delivered = delivered_after_rollers(draw, segments);
    (delivered != 0).then(|| {
        // The equalization search finds the greatest *permissible* draw, not
        // necessarily the cheapest draw that delivers the same whole energy.
        // Reducing input within the final output plateau preserves the charge
        // gradient and delivered work while avoiding unnecessary friction loss.
        let minimum_draw = minimum_draw_for_delivery(delivered, segments);
        debug_assert!(minimum_draw <= draw);
        (minimum_draw, delivered)
    })
}

pub(super) fn plan_pair(
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
    let charge_order = compare_charge(
        a_remaining.nanojoules(),
        da.capacity().nanojoules(),
        b_remaining.nanojoules(),
        db.capacity().nanojoules(),
    );
    let (source, destination, source_def, dest_def, source_stored, dest_stored) =
        if charge_order == Ordering::Greater {
            (left, right, da, db, a_remaining, b_remaining)
        } else if charge_order == Ordering::Less {
            (right, left, db, da, b_remaining, a_remaining)
        } else {
            return None;
        };
    // Shaft and source power limit work drawn before friction; the receiver's
    // input power limits useful work delivered after the rollers' losses.
    let draw_power = WOODEN_TRANSMISSION_POWER.min(source_def.max_output_power());
    let delivery_power = dest_def.max_input_power();
    if draw_power.is_zero() || delivery_power.is_zero() {
        return None;
    }
    let maximum_draw_per_tick = integrate_power(
        draw_power,
        TickSpan::new(1),
        registries.core().physical_tick_duration(),
        PowerRemainder::ZERO,
    )
    .ok()?
    .energy();
    let maximum_delivery_per_tick = integrate_power(
        delivery_power,
        TickSpan::new(1),
        registries.core().physical_tick_duration(),
        PowerRemainder::ZERO,
    )
    .ok()?
    .energy();
    let headroom = dest_def
        .capacity()
        .checked_sub(dest_stored)?
        .min(maximum_delivery_per_tick);
    let maximum_draw = maximum_draw_per_tick
        .nanojoules()
        .min(source_stored.nanojoules());
    // Each embodied roller loses real work. Headroom bounds the resulting
    // delivery, not the larger draw before friction.
    let (capacity_limited_draw, _) =
        bounded_shaft_transfer(maximum_draw, headroom.nanojoules(), segments)?;
    let (intake, delivered) = equalizing_shaft_draw(
        capacity_limited_draw,
        segments,
        source_stored.nanojoules(),
        source_def.capacity().nanojoules(),
        dest_stored.nanojoules(),
        dest_def.capacity().nanojoules(),
    )?;
    Some(MechanicalTransmissionReceipt {
        coupling,
        segments,
        source,
        destination,
        drawn: Energy::from_nanojoules(intake),
        delivered: Energy::from_nanojoules(delivered),
    })
}
