//! Read-only feed planning for batched manual native-copper recovery.

use deep_hearth::content::PROCESS_HAND_SORT_NATIVE_COPPER;
use deep_hearth::core::quantity::Mass;
use deep_hearth::ore_processing::ManualConstituentSeparationProcessDefinition;
use deep_hearth::registry::Registries;

fn maximum_recoverable_target(
    sorting: ManualConstituentSeparationProcessDefinition,
    feed: Mass,
    constituent_ppm: u32,
) -> Mass {
    let mut lower = 0_u64;
    let mut upper = feed.milligrams();
    while lower < upper {
        let candidate = lower + (upper - lower).div_ceil(2);
        let supported = sorting
            .minimum_homogeneous_feed_mass_for_target_recovery(
                Mass::from_milligrams(candidate),
                constituent_ppm,
            )
            .is_some_and(|required| required <= feed);
        if supported {
            lower = candidate;
        } else {
            upper = candidate - 1;
        }
    }
    Mass::from_milligrams(lower)
}

pub(super) fn projected_batched_recovery(
    sorting: ManualConstituentSeparationProcessDefinition,
    feed: Mass,
    constituent_ppm: u32,
) -> Option<Mass> {
    let mut remaining = feed;
    let mut recovered = Mass::ZERO;
    while !remaining.is_zero() {
        let batch = remaining.min(sorting.max_batch_mass());
        recovered =
            recovered.checked_add(maximum_recoverable_target(sorting, batch, constituent_ppm))?;
        remaining = remaining.checked_sub(batch)?;
    }
    Some(recovered)
}

/// Smallest homogeneous ore feed whose runtime-sized hand-sorting batches recover `target`.
///
/// The production projection is intentionally applied once per authored runtime batch. This
/// preserves the same whole-milligram rounding groups used by execution instead of treating a
/// multi-batch campaign as one fictional homogeneous recovery event.
pub(super) fn minimum_manual_ore_feed_for_target_recovery(
    registries: &Registries,
    target: Mass,
    constituent_ppm: u32,
) -> Option<Mass> {
    if target.is_zero() {
        return Some(Mass::ZERO);
    }
    let sorting = registries
        .ore_processing()
        .get_manual_constituent_separation(PROCESS_HAND_SORT_NATIVE_COPPER)?;
    let lower =
        sorting.minimum_homogeneous_feed_mass_for_target_recovery(target, constituent_ppm)?;
    let sufficient = |feed| {
        projected_batched_recovery(sorting, feed, constituent_ppm)
            .is_some_and(|recovered| recovered >= target)
    };
    if sufficient(lower) {
        return Some(lower);
    }

    let mut upper = lower;
    loop {
        upper = Mass::from_milligrams(upper.milligrams().checked_mul(2)?);
        if sufficient(upper) {
            break;
        }
    }

    let mut lower_mg = lower.milligrams() + 1;
    let mut upper_mg = upper.milligrams();
    while lower_mg < upper_mg {
        let candidate = lower_mg + (upper_mg - lower_mg) / 2;
        if sufficient(Mass::from_milligrams(candidate)) {
            upper_mg = candidate;
        } else {
            lower_mg = candidate + 1;
        }
    }
    Some(Mass::from_milligrams(lower_mg))
}
