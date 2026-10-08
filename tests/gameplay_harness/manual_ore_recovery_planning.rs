//! Pre-action attention projection for the manual ore-recovery fallback.

use deep_hearth::content::{PROCESS_HAND_BREAK_ORE, PROCESS_HAND_SORT_NATIVE_COPPER};
use deep_hearth::core::quantity::Mass;
use deep_hearth::ore_processing::{ManualOreProcessProfile, project_manual_ore_duration};
use deep_hearth::registry::Registries;

pub(super) fn project_manual_ore_recovery_attention(
    registries: &Registries,
    feed_mass: Mass,
) -> u64 {
    let breaking = registries
        .ore_processing()
        .get_manual_comminution(PROCESS_HAND_BREAK_ORE)
        .unwrap_or_else(|| panic!("manual ore recovery lost its hand-breaking definition"));
    let sorting = registries
        .ore_processing()
        .get_manual_constituent_separation(PROCESS_HAND_SORT_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("manual ore recovery lost its hand-sorting definition"));
    let mut attention = 0_u64;
    for (profile, maximum, context) in [
        (
            ManualOreProcessProfile::new(
                breaking.processing_rate(),
                breaking.max_batch_mass(),
                breaking.exertion(),
            ),
            breaking.max_batch_mass(),
            "breaking",
        ),
        (
            ManualOreProcessProfile::new(
                sorting.processing_rate(),
                sorting.max_batch_mass(),
                sorting.exertion(),
            ),
            sorting.max_batch_mass(),
            "sorting",
        ),
    ] {
        let mut remaining = feed_mass;
        while !remaining.is_zero() {
            let batch = remaining.min(maximum);
            let duration = project_manual_ore_duration(
                registries.core().physical_tick_duration(),
                profile,
                batch,
            )
            .unwrap_or_else(|error| {
                panic!("manual ore recovery {context} projection failed: {error}")
            });
            attention = attention
                .checked_add(duration.value())
                .unwrap_or_else(|| panic!("manual ore recovery projection attention overflowed"));
            remaining = remaining
                .checked_sub(batch)
                .unwrap_or_else(|| unreachable!("projected manual ore batch cannot exceed feed"));
        }
    }
    attention
}
