//! Contract tests for normalized direct-control input.

use super::{HorizontalControlInput, HorizontalControlInputError};

#[test]
fn horizontal_control_rejects_diagonal_speed_amplification() {
    assert_eq!(
        HorizontalControlInput::new(1_000_000, 1_000_000),
        Err(HorizontalControlInputError::MagnitudeExceedsUnitCircle {
            x_ppm: 1_000_000,
            z_ppm: 1_000_000,
        })
    );
    assert_eq!(
        HorizontalControlInput::new(600_000, 800_000),
        Ok(HorizontalControlInput::new(600_000, 800_000)
            .unwrap_or_else(|error| panic!("normalized control fixture failed: {error}")))
    );
}
