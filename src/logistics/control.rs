//! Transient world-space direct-control input for continuous player motion.

use std::error::Error;
use std::fmt::{Display, Formatter};

pub const CONTROL_INPUT_PARTS_PER_MILLION: i32 = 1_000_000;

/// Validation failure for one fixed-point horizontal control vector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HorizontalControlInputError {
    ComponentOutsideRange { x_ppm: i32, z_ppm: i32 },
    MagnitudeExceedsUnitCircle { x_ppm: i32, z_ppm: i32 },
}

impl Display for HorizontalControlInputError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ComponentOutsideRange { x_ppm, z_ppm } => write!(
                formatter,
                "horizontal control components ({x_ppm},{z_ppm}) exceed +/-{CONTROL_INPUT_PARTS_PER_MILLION} ppm",
            ),
            Self::MagnitudeExceedsUnitCircle { x_ppm, z_ppm } => write!(
                formatter,
                "horizontal control vector ({x_ppm},{z_ppm}) exceeds normalized unit magnitude",
            ),
        }
    }
}

impl Error for HorizontalControlInputError {}

/// Camera-independent horizontal intent in authoritative world axes.
///
/// Adapters may rotate view-relative input into this fixed-point vector before simulation. The
/// vector must stay inside the normalized unit circle, preventing diagonal input from increasing
/// authored walking speed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HorizontalControlInput {
    x_ppm: i32,
    z_ppm: i32,
}

impl HorizontalControlInput {
    pub const NEUTRAL: Self = Self { x_ppm: 0, z_ppm: 0 };

    pub fn new(x_ppm: i32, z_ppm: i32) -> Result<Self, HorizontalControlInputError> {
        let limit = i64::from(CONTROL_INPUT_PARTS_PER_MILLION);
        let x = i64::from(x_ppm);
        let z = i64::from(z_ppm);
        if x.abs() > limit || z.abs() > limit {
            return Err(HorizontalControlInputError::ComponentOutsideRange { x_ppm, z_ppm });
        }
        let magnitude_squared = i128::from(x) * i128::from(x) + i128::from(z) * i128::from(z);
        if magnitude_squared > i128::from(limit) * i128::from(limit) {
            return Err(HorizontalControlInputError::MagnitudeExceedsUnitCircle { x_ppm, z_ppm });
        }
        Ok(Self { x_ppm, z_ppm })
    }

    #[must_use]
    pub const fn x_ppm(self) -> i32 {
        self.x_ppm
    }

    #[must_use]
    pub const fn z_ppm(self) -> i32 {
        self.z_ppm
    }

    #[must_use]
    pub const fn is_neutral(self) -> bool {
        self.x_ppm == 0 && self.z_ppm == 0
    }
}

/// Direct-control sample consumed by one authoritative 20 Hz control tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerControlInput {
    horizontal: HorizontalControlInput,
    jump: bool,
    crouch: bool,
    sprint: bool,
}

impl PlayerControlInput {
    pub const NEUTRAL: Self = Self {
        horizontal: HorizontalControlInput::NEUTRAL,
        jump: false,
        crouch: false,
        sprint: false,
    };

    #[must_use]
    pub const fn new(horizontal: HorizontalControlInput) -> Self {
        Self {
            horizontal,
            jump: false,
            crouch: false,
            sprint: false,
        }
    }

    #[must_use]
    pub const fn horizontal(self) -> HorizontalControlInput {
        self.horizontal
    }

    #[must_use]
    pub const fn with_jump(mut self, jump: bool) -> Self {
        self.jump = jump;
        self
    }

    #[must_use]
    pub const fn with_crouch(mut self, crouch: bool) -> Self {
        self.crouch = crouch;
        self
    }

    #[must_use]
    pub const fn with_sprint(mut self, sprint: bool) -> Self {
        self.sprint = sprint;
        self
    }

    #[must_use]
    pub const fn jump(self) -> bool {
        self.jump
    }

    #[must_use]
    pub const fn crouch(self) -> bool {
        self.crouch
    }

    #[must_use]
    pub const fn sprint(self) -> bool {
        self.sprint
    }
}
