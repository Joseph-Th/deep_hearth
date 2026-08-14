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
// Rotational torque uses micronewton-meters. Together with microradians/second this multiplies
// exactly into picowatts, avoiding floating-point angular conversions in authoritative physics.
unsigned_quantity!(Torque, u64, from_micronewton_meters, micronewton_meters);
// Angular speed uses microradians per second. Radians are dimensionless for power calculation.
unsigned_quantity!(
    AngularSpeed,
    u64,
    from_microradians_per_second,
    microradians_per_second
);
unsigned_quantity!(ElectricPotential, u64, from_microvolts, microvolts);
unsigned_quantity!(ElectricCurrent, u64, from_microamperes, microamperes);
unsigned_quantity!(ElectricalResistance, u64, from_microohms, microohms);
unsigned_quantity!(Volume, u64, from_microliters, microliters);
unsigned_quantity!(
    VolumetricFlow,
    u64,
    from_microliters_per_second,
    microliters_per_second
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
mod tests {
    use super::*;

    #[test]
    fn mass_arithmetic_rejects_overflow_and_underflow() {
        assert_eq!(
            Mass::from_milligrams(7).checked_add(Mass::from_milligrams(5)),
            Some(Mass::from_milligrams(12))
        );
        assert_eq!(
            Mass::from_milligrams(7).checked_sub(Mass::from_milligrams(5)),
            Some(Mass::from_milligrams(2))
        );
        assert_eq!(
            Mass::from_milligrams(u64::MAX).checked_add(Mass::from_milligrams(1)),
            None
        );
        assert_eq!(
            Mass::from_milligrams(1).checked_sub(Mass::from_milligrams(2)),
            None
        );
    }

    #[test]
    fn aggregate_volume_accumulates_beyond_single_store_range() {
        let largest_store = AggregateVolume::from_volume(Volume::from_microliters(u64::MAX));

        assert_eq!(
            largest_store.checked_add(largest_store),
            Some(AggregateVolume::from_microliters(u128::from(u64::MAX) * 2))
        );
    }

    #[test]
    fn absolute_temperature_arithmetic_cannot_cross_zero_or_overflow() {
        let temperature = Temperature::from_millikelvin(293_150);

        assert_eq!(
            temperature.checked_add_millikelvin(1_000),
            Some(Temperature::from_millikelvin(294_150))
        );
        assert_eq!(
            temperature.checked_sub_millikelvin(1_000),
            Some(Temperature::from_millikelvin(292_150))
        );
        assert_eq!(Temperature::ZERO.checked_sub_millikelvin(1), None);
        assert_eq!(
            Temperature::from_millikelvin(u32::MAX).checked_add_millikelvin(1),
            None
        );
    }

    #[test]
    fn energy_arithmetic_is_exact_and_checked() {
        let first = Energy::from_nanojoules(9);
        let second = Energy::from_nanojoules(4);

        assert_eq!(first.checked_add(second), Some(Energy::from_nanojoules(13)));
        assert_eq!(first.checked_sub(second), Some(Energy::from_nanojoules(5)));
        assert_eq!(second.checked_sub(first), None);
        assert_eq!(
            Energy::from_nanojoules(u128::MAX).checked_add(Energy::from_nanojoules(1)),
            None
        );
    }

    #[test]
    fn aggregate_mass_accumulates_beyond_single_record_range() {
        let largest_record = AggregateMass::from_mass(Mass::from_milligrams(u64::MAX));

        assert_eq!(
            largest_record.checked_add(largest_record),
            Some(AggregateMass::from_milligrams(u128::from(u64::MAX) * 2))
        );
    }

    #[test]
    fn physical_rate_and_electrical_quantities_use_explicit_units() {
        assert_eq!(Pressure::from_pascals(101_325).pascals(), 101_325);
        assert_eq!(Area::from_square_millimeters(250).square_millimeters(), 250);
        assert_eq!(Length::from_micrometers(2_500).micrometers(), 2_500);
        assert_eq!(
            Acceleration::from_micrometers_per_second_squared(9_806_650)
                .micrometers_per_second_squared(),
            9_806_650
        );
        assert_eq!(Force::from_millinewtons(4_000).millinewtons(), 4_000);
        assert_eq!(Power::from_microwatts(3).picowatts(), 3_000_000);
        assert_eq!(
            Torque::from_micronewton_meters(2_000_000).micronewton_meters(),
            2_000_000
        );
        assert_eq!(
            AngularSpeed::from_microradians_per_second(3_000_000).microradians_per_second(),
            3_000_000
        );
        assert_eq!(ElectricPotential::from_microvolts(12).microvolts(), 12);
        assert_eq!(ElectricCurrent::from_microamperes(5).microamperes(), 5);
        assert_eq!(ElectricalResistance::from_microohms(7).microohms(), 7);
        assert_eq!(Volume::from_microliters(9).microliters(), 9);
        assert_eq!(
            VolumetricFlow::from_microliters_per_second(11).microliters_per_second(),
            11
        );
    }
}
