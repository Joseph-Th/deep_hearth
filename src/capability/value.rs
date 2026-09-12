//! Typed physical capability values and deterministic interpolation.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::core::arithmetic::scale_u128_fraction_floor;
use crate::core::quantity::{Mass, MassFlow, Power, Pressure, Temperature};

/// Physical/value dimension carried by one authored capability.
/// Capability dimensions are explicit typed variants rather than generic numeric tiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CapabilityValueKind {
    Mass,
    Temperature,
    Pressure,
    Power,
    MassFlow,
}

/// Typed value exposed by a capability provider or required by an operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CapabilityValue {
    Mass(Mass),
    Temperature(Temperature),
    Pressure(Pressure),
    Power(Power),
    MassFlow(MassFlow),
}

impl CapabilityValue {
    #[must_use]
    pub const fn kind(self) -> CapabilityValueKind {
        match self {
            Self::Mass(_) => CapabilityValueKind::Mass,
            Self::Temperature(_) => CapabilityValueKind::Temperature,
            Self::Pressure(_) => CapabilityValueKind::Pressure,
            Self::Power(_) => CapabilityValueKind::Power,
            Self::MassFlow(_) => CapabilityValueKind::MassFlow,
        }
    }

    fn magnitude(self) -> u128 {
        match self {
            Self::Mass(value) => u128::from(value.milligrams()),
            Self::Temperature(value) => u128::from(value.millikelvin()),
            Self::Pressure(value) => u128::from(value.pascals()),
            Self::Power(value) => value.picowatts(),
            Self::MassFlow(value) => u128::from(value.milligrams_per_second()),
        }
    }

    pub(crate) fn compare(self, other: Self) -> Option<Ordering> {
        if self.kind() != other.kind() {
            return None;
        }
        Some(self.magnitude().cmp(&other.magnitude()))
    }
}

fn interpolate_magnitude_toward(
    degraded: u128,
    improved: u128,
    numerator: u32,
    denominator: u32,
) -> u128 {
    debug_assert!(denominator != 0);
    debug_assert!(numerator <= denominator);
    if numerator == 0 || degraded == improved {
        return degraded;
    }
    if numerator == denominator {
        return improved;
    }

    let delta = degraded.abs_diff(improved);
    // Rounding stays toward the degraded endpoint, never overstating recovery.
    let scaled_delta = scale_u128_fraction_floor(delta, numerator, denominator);
    if improved >= degraded {
        degraded + scaled_delta
    } else {
        degraded - scaled_delta
    }
}

pub(crate) fn interpolate_capability_value(
    degraded: CapabilityValue,
    improved: CapabilityValue,
    numerator: u32,
    denominator: u32,
) -> Option<CapabilityValue> {
    if degraded.kind() != improved.kind() || denominator == 0 || numerator > denominator {
        return None;
    }
    let magnitude = interpolate_magnitude_toward(
        degraded.magnitude(),
        improved.magnitude(),
        numerator,
        denominator,
    );

    match degraded {
        CapabilityValue::Mass(_) => u64::try_from(magnitude)
            .ok()
            .map(Mass::from_milligrams)
            .map(CapabilityValue::Mass),
        CapabilityValue::Temperature(_) => u32::try_from(magnitude)
            .ok()
            .map(Temperature::from_millikelvin)
            .map(CapabilityValue::Temperature),
        CapabilityValue::Pressure(_) => u64::try_from(magnitude)
            .ok()
            .map(Pressure::from_pascals)
            .map(CapabilityValue::Pressure),
        CapabilityValue::Power(_) => Some(CapabilityValue::Power(Power::from_picowatts(magnitude))),
        CapabilityValue::MassFlow(_) => u64::try_from(magnitude)
            .ok()
            .map(MassFlow::from_milligrams_per_second)
            .map(CapabilityValue::MassFlow),
    }
}
