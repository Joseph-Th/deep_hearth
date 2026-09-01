//! Player-facing formatting for authoritative simulation duration.

use deep_hearth::registry::Registries;

/// Formats authoritative simulation ticks as physical time without floating point.
pub(super) fn format_physical_duration(registries: &Registries, ticks: u64) -> String {
    let microseconds = u128::from(registries.core().physical_tick_duration().microseconds())
        .checked_mul(u128::from(ticks))
        .unwrap_or_else(|| panic!("gameplay harness physical-duration conversion overflowed"));
    if microseconds >= 60_000_000 {
        let tenths = microseconds / 6_000_000;
        format!("{}.{:01}m", tenths / 10, tenths % 10)
    } else {
        let tenths = microseconds / 100_000;
        format!("{}.{:01}s", tenths / 10, tenths % 10)
    }
}
