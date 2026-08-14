//! Exact force conversions for structural load providers; sibling analysis consumes force contributions without owning their causes.

use crate::core::quantity::{Acceleration, Area, Force, Mass, Pressure};

fn divide_ceiling(numerator: u128, denominator: u128) -> u128 {
    if numerator == 0 {
        0
    } else {
        1 + (numerator - 1) / denominator
    }
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
    Force::from_millinewtons(divide_ceiling(numerator, 1_000_000_000))
}

/// Converts uniform pressure over explicit area into conservative whole-millinewton force.
///
/// `1 Pa * 1 mm^2 = 0.001 mN`, supporting future wind, water, soil, and contact-pressure providers
/// without duplicating unit conversion or rounding policy in each subsystem.
#[must_use]
pub fn calculate_pressure_force_ceiling(pressure: Pressure, area: Area) -> Force {
    let numerator = u128::from(pressure.pascals()) * u128::from(area.square_millimeters());
    Force::from_millinewtons(divide_ceiling(numerator, 1_000))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kilogram_under_explicit_standard_gravity_rounds_up_conservatively() {
        let force = calculate_weight_force_ceiling(
            Mass::from_milligrams(1_000_000),
            Acceleration::from_micrometers_per_second_squared(9_806_650),
        );

        assert_eq!(force, Force::from_millinewtons(9_807));
    }

    #[test]
    fn pressure_area_conversion_is_exact_when_millinewton_aligned() {
        let force = calculate_pressure_force_ceiling(
            Pressure::from_pascals(2_000),
            Area::from_square_millimeters(500),
        );

        assert_eq!(force, Force::from_millinewtons(1_000));
    }

    #[test]
    fn fractional_pressure_force_rounds_up_instead_of_erasing_load() {
        let force = calculate_pressure_force_ceiling(
            Pressure::from_pascals(1),
            Area::from_square_millimeters(1),
        );

        assert_eq!(force, Force::from_millinewtons(1));
    }

    #[test]
    fn zero_source_quantities_produce_zero_force() {
        let acceleration = Acceleration::from_micrometers_per_second_squared(9_806_650);
        assert_eq!(
            calculate_weight_force_ceiling(Mass::ZERO, acceleration),
            Force::ZERO
        );
        assert_eq!(
            calculate_pressure_force_ceiling(Pressure::ZERO, Area::from_square_millimeters(1)),
            Force::ZERO
        );
    }
}
