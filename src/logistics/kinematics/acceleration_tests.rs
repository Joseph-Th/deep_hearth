//! Euclidean acceleration-limit contracts for non-axis-aligned direct movement.

use super::*;
use crate::content::build_registries;

#[test]
fn near_axis_acceleration_does_not_exceed_authored_magnitude() {
    let registries = build_registries();
    let locomotion = registries.survival().physiology().locomotion();
    let maximum = horizontal_acceleration_velocity_delta(locomotion)
        .unwrap_or_else(|error| panic!("authored acceleration invalid: {error}"));

    // The norm is just over the limit even though integer sqrt rounds down to it.
    for (x, z) in [(maximum, 1), (-maximum, 1), (1, maximum), (1, -maximum)] {
        let result = accelerate_horizontal_velocity(
            HorizontalVelocity::ZERO,
            HorizontalVelocity::new(x, z),
            locomotion,
        )
        .unwrap_or_else(|error| panic!("near-axis acceleration failed: {error}"));
        let dx = i128::from(result.x_nanometers_per_second());
        let dz = i128::from(result.z_nanometers_per_second());
        assert!(dx * dx + dz * dz <= i128::from(maximum) * i128::from(maximum));
    }
}

#[test]
fn acceleration_keeps_exactly_reachable_vectors() {
    let registries = build_registries();
    let locomotion = registries.survival().physiology().locomotion();
    let maximum = horizontal_acceleration_velocity_delta(locomotion)
        .unwrap_or_else(|error| panic!("authored acceleration invalid: {error}"));
    let desired = HorizontalVelocity::new(maximum, 0);
    assert_eq!(
        accelerate_horizontal_velocity(HorizontalVelocity::ZERO, desired, locomotion),
        Ok(desired)
    );
}
