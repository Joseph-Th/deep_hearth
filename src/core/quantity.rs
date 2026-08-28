//! Exact integer physical quantities used by authoritative simulation state and definitions.

use serde::{Deserialize, Serialize};

/// Mass stored as integer milligrams so authoritative matter accounting never depends on floats.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct Mass(u64);

impl Mass {
    /// Zero mass.
    pub const ZERO: Self = Self(0);

    /// Builds an exact mass from milligrams.
    #[must_use]
    pub const fn from_milligrams(milligrams: u64) -> Self {
        Self(milligrams)
    }

    /// Returns the exact milligram representation.
    #[must_use]
    pub const fn milligrams(self) -> u64 {
        self.0
    }

    /// Returns whether the quantity is zero.
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// Checked addition without saturating or wrapping authoritative quantities.
    #[must_use]
    pub const fn checked_add(self, other: Self) -> Option<Self> {
        match self.0.checked_add(other.0) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// Checked subtraction without allowing negative authoritative quantities.
    #[must_use]
    pub const fn checked_sub(self, other: Self) -> Option<Self> {
        match self.0.checked_sub(other.0) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}

/// World-scale fluid volume aggregate in microliters with wider accumulation than one store.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct AggregateVolume(u128);

impl AggregateVolume {
    pub const ZERO: Self = Self(0);

    #[must_use]
    pub const fn from_microliters(microliters: u128) -> Self {
        Self(microliters)
    }

    #[must_use]
    pub fn from_volume(volume: Volume) -> Self {
        Self(u128::from(volume.microliters()))
    }

    #[must_use]
    pub const fn microliters(self) -> u128 {
        self.0
    }

    #[must_use]
    pub const fn checked_add(self, other: Self) -> Option<Self> {
        match self.0.checked_add(other.0) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}

/// World-scale mass aggregate stored in milligrams with wider accumulation than individual lots.
///
/// Individual runtime records intentionally use compact `u64` mass. Projections that sum matter
/// across a persistent world use this `u128` type so accounting does not inherit a single-record
/// capacity limit.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct AggregateMass(u128);

impl AggregateMass {
    pub const ZERO: Self = Self(0);

    #[must_use]
    pub const fn from_milligrams(milligrams: u128) -> Self {
        Self(milligrams)
    }

    #[must_use]
    pub fn from_mass(mass: Mass) -> Self {
        Self(u128::from(mass.milligrams()))
    }

    #[must_use]
    pub const fn milligrams(self) -> u128 {
        self.0
    }

    #[must_use]
    pub const fn checked_add(self, other: Self) -> Option<Self> {
        match self.0.checked_add(other.0) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}

macro_rules! unsigned_quantity {
    ($name:ident, $backing:ty, $constructor:ident, $getter:ident) => {
        #[derive(
            Clone,
            Copy,
            Debug,
            Default,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            Serialize,
            Deserialize,
        )]
        pub struct $name($backing);

        impl $name {
            pub const ZERO: Self = Self(0);

            #[must_use]
            pub const fn $constructor(value: $backing) -> Self {
                Self(value)
            }

            #[must_use]
            pub const fn $getter(self) -> $backing {
                self.0
            }

            #[must_use]
            pub const fn is_zero(self) -> bool {
                self.0 == 0
            }

            #[must_use]
            pub const fn checked_add(self, other: Self) -> Option<Self> {
                match self.0.checked_add(other.0) {
                    Some(value) => Some(Self(value)),
                    None => None,
                }
            }

            #[must_use]
            pub const fn checked_sub(self, other: Self) -> Option<Self> {
                match self.0.checked_sub(other.0) {
                    Some(value) => Some(Self(value)),
                    None => None,
                }
            }
        }
    };
}

unsigned_quantity!(Pressure, u64, from_pascals, pascals);
unsigned_quantity!(Area, u64, from_square_millimeters, square_millimeters);
unsigned_quantity!(Length, u64, from_micrometers, micrometers);
unsigned_quantity!(
    Acceleration,
    u64,
    from_micrometers_per_second_squared,
    micrometers_per_second_squared
);
unsigned_quantity!(Power, u128, from_picowatts, picowatts);
unsigned_quantity!(Force, u128, from_millinewtons, millinewtons);
unsigned_quantity!(Volume, u64, from_microliters, microliters);
unsigned_quantity!(
    MassSpecificEnergy,
    u64,
    from_nanojoules_per_milligram,
    nanojoules_per_milligram
);
// Mass flow uses milligrams per second so material-throughput systems can derive authoritative
// duration without introducing floating point or conflating throughput with one-time batch mass.
unsigned_quantity!(
    MassFlow,
    u64,
    from_milligrams_per_second,
    milligrams_per_second
);

impl Power {
    /// Convenience constructor for whole microwatts while retaining picowatt storage precision.
    #[must_use]
    pub const fn from_microwatts(microwatts: u64) -> Self {
        Self((microwatts as u128) * 1_000_000)
    }

    /// Returns whole microwatts when exact; callers needing full precision use `picowatts`.
    #[must_use]
    pub const fn whole_microwatts(self) -> u128 {
        self.0 / 1_000_000
    }
}

/// Nonnegative energy stored as integer nanojoules.
///
/// Nanojoules make sensible-heat calculations exact at the existing milligram, millikelvin, and
/// joule-per-kilogram-kelvin material-property scales: for a pure material,
/// `mass_mg * specific_heat * delta_mK` is directly an energy in nanojoules.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct Energy(u128);

impl Energy {
    pub const ZERO: Self = Self(0);

    #[must_use]
    pub const fn from_nanojoules(nanojoules: u128) -> Self {
        Self(nanojoules)
    }

    #[must_use]
    pub const fn nanojoules(self) -> u128 {
        self.0
    }

    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    #[must_use]
    pub const fn checked_add(self, other: Self) -> Option<Self> {
        match self.0.checked_add(other.0) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    #[must_use]
    pub const fn checked_sub(self, other: Self) -> Option<Self> {
        match self.0.checked_sub(other.0) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}

/// Absolute temperature stored as unsigned integer millikelvin.
///
/// Negative absolute temperatures are intentionally unrepresentable. Subsystems that need a
/// signed temperature difference should use a dedicated delta type rather than overloading this
/// absolute quantity.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct Temperature(u32);

impl Temperature {
    /// Absolute zero.
    pub const ZERO: Self = Self(0);

    /// Builds an exact temperature from millikelvin.
    #[must_use]
    pub const fn from_millikelvin(millikelvin: u32) -> Self {
        Self(millikelvin)
    }

    /// Returns the exact millikelvin representation.
    #[must_use]
    pub const fn millikelvin(self) -> u32 {
        self.0
    }

    /// Checked increase of an absolute temperature.
    #[must_use]
    pub const fn checked_add_millikelvin(self, delta: u32) -> Option<Self> {
        match self.0.checked_add(delta) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// Checked decrease that cannot pass below absolute zero.
    #[must_use]
    pub const fn checked_sub_millikelvin(self, delta: u32) -> Option<Self> {
        match self.0.checked_sub(delta) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}

#[cfg(test)]
#[path = "quantity_tests.rs"]
mod tests;
