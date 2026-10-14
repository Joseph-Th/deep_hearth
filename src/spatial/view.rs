//! Fixed-point view-direction vocabulary shared by gameplay targeting and adapters.

use std::error::Error;
use std::fmt::{Display, Formatter};

use super::Axis;

pub const VIEW_DIRECTION_PARTS_PER_MILLION: i32 = 1_000_000;
const NORMALIZATION_SQUARED_TOLERANCE: u64 = 3_000_000;

/// Rounded unit direction in millionths of one Cartesian unit.
///
/// Adapters may derive this from camera yaw/pitch using platform math, but gameplay receives only
/// this bounded fixed-point value. The normalization tolerance is tight enough to admit ordinary
/// component rounding while rejecting materially shorter or longer vectors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ViewDirection {
    x_parts_per_million: i32,
    y_parts_per_million: i32,
    z_parts_per_million: i32,
}

impl ViewDirection {
    pub fn new(
        x_parts_per_million: i32,
        y_parts_per_million: i32,
        z_parts_per_million: i32,
    ) -> Result<Self, ViewDirectionError> {
        for (axis, value) in [
            (Axis::X, x_parts_per_million),
            (Axis::Y, y_parts_per_million),
            (Axis::Z, z_parts_per_million),
        ] {
            if !(-VIEW_DIRECTION_PARTS_PER_MILLION..=VIEW_DIRECTION_PARTS_PER_MILLION)
                .contains(&value)
            {
                return Err(ViewDirectionError::ComponentOutsideUnitRange { axis, value });
            }
        }
        let squared_length = squared_length_parts(
            x_parts_per_million,
            y_parts_per_million,
            z_parts_per_million,
        );
        let expected = u64::try_from(VIEW_DIRECTION_PARTS_PER_MILLION)
            .unwrap_or_else(|_| unreachable!("positive view-direction scale fits u64"))
            .pow(2);
        if squared_length.abs_diff(expected) > NORMALIZATION_SQUARED_TOLERANCE {
            return Err(ViewDirectionError::NotNormalized { squared_length });
        }
        Ok(Self {
            x_parts_per_million,
            y_parts_per_million,
            z_parts_per_million,
        })
    }

    #[must_use]
    pub const fn x_parts_per_million(self) -> i32 {
        self.x_parts_per_million
    }

    #[must_use]
    pub const fn y_parts_per_million(self) -> i32 {
        self.y_parts_per_million
    }

    #[must_use]
    pub const fn z_parts_per_million(self) -> i32 {
        self.z_parts_per_million
    }
}

fn squared_length_parts(x: i32, y: i32, z: i32) -> u64 {
    let squared =
        i64::from(x) * i64::from(x) + i64::from(y) * i64::from(y) + i64::from(z) * i64::from(z);
    u64::try_from(squared).unwrap_or_else(|_| unreachable!("sum of integer squares is nonnegative"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewDirectionError {
    ComponentOutsideUnitRange { axis: Axis, value: i32 },
    NotNormalized { squared_length: u64 },
}

impl Display for ViewDirectionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ComponentOutsideUnitRange { axis, value } => write!(
                formatter,
                "view direction {axis:?} component {value} exceeds +/-{} parts per million",
                VIEW_DIRECTION_PARTS_PER_MILLION
            ),
            Self::NotNormalized { squared_length } => write!(
                formatter,
                "view direction squared length {squared_length} is not a rounded unit vector"
            ),
        }
    }
}

impl Error for ViewDirectionError {}
