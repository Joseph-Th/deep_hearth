//! Schedule-derived checkpoints for persistence contracts.

use deep_hearth::core::time::SimulationTick;

/// Chooses an interior checkpoint from the admitted work schedule.
///
/// Persistence contracts care that work survives a mid-action boundary, not that an authored
/// action still lasts some historical hard-coded number of ticks.
pub(super) fn mid_work_checkpoint_ticks(
    started_at: SimulationTick,
    completes_at: SimulationTick,
    context: &'static str,
) -> u64 {
    let duration = completes_at
        .checked_duration_since(started_at)
        .unwrap_or_else(|| panic!("gameplay harness {context} completion precedes its start"))
        .value();
    assert!(
        duration > 1,
        "gameplay harness {context} requires multi-tick work for a mid-action checkpoint"
    );
    (duration / 2).max(1).min(duration - 1)
}
