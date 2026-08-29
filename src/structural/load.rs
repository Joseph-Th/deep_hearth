//! Converts owner-supplied mass into conservative structural force.

use crate::core::quantity::{Acceleration, AggregateMass, Force, Mass};

/// Converts a world-scale aggregate mass under explicit acceleration into structural force.
///
/// The calculation decomposes the division before multiplication so totals beyond one `Mass`
/// record can be resolved without overflowing an otherwise representable `Force`.
#[must_use]
pub fn calculate_aggregate_weight_force_ceiling(
    mass: AggregateMass,
    acceleration: Acceleration,
) -> Option<Force> {
    const MILLIGRAM_MICROMETER_PER_MILLI_NEWTON: u128 = 1_000_000_000;

    let mass_milligrams = mass.milligrams();
    let acceleration_value = u128::from(acceleration.micrometers_per_second_squared());
    let whole_mass_units = mass_milligrams / MILLIGRAM_MICROMETER_PER_MILLI_NEWTON;
    let remainder_mass = mass_milligrams % MILLIGRAM_MICROMETER_PER_MILLI_NEWTON;
    let whole_force = whole_mass_units.checked_mul(acceleration_value)?;
    let remainder_numerator = remainder_mass.checked_mul(acceleration_value)?;
    let remainder_force = remainder_numerator.div_ceil(MILLIGRAM_MICROMETER_PER_MILLI_NEWTON);
    whole_force
        .checked_add(remainder_force)
        .map(Force::from_millinewtons)
}

/// Converts mass under an explicit acceleration into a conservative whole-millinewton load.
///
/// Mass is authoritative in milligrams and acceleration in micrometers/second squared. Their
/// product is `1e-9` millinewtons, so division by one billion gives the resulting force. Rounding
/// upward prevents storage, snow, fluid, or equipment loads from gaining structural capacity by
/// repeated truncation. The caller owns acceleration; no universal gravity constant is assumed.
#[must_use]
pub fn calculate_weight_force_ceiling(mass: Mass, acceleration: Acceleration) -> Force {
    let numerator =
        u128::from(mass.milligrams()) * u128::from(acceleration.micrometers_per_second_squared());
    Force::from_millinewtons(numerator.div_ceil(1_000_000_000))
}

#[cfg(test)]
#[path = "load_tests.rs"]
mod tests;
