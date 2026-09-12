//! Authored condition-dependent equipment capability curves.

use std::cmp::Ordering;

use crate::capability::{CapabilityId, CapabilityValue, CapabilityValueKind};
use crate::maintenance::Condition;

/// One authored effective capability value at a specific remaining-condition point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapabilityConditionPoint {
    condition: Condition,
    value: CapabilityValue,
}

impl CapabilityConditionPoint {
    #[must_use]
    pub const fn new(condition: Condition, value: CapabilityValue) -> Self {
        Self { condition, value }
    }

    #[must_use]
    pub const fn condition(self) -> Condition {
        self.condition
    }

    #[must_use]
    pub const fn value(self) -> CapabilityValue {
        self.value
    }
}

/// Authored piecewise-linear response of one capability to equipment degradation.
///
/// The pristine endpoint is the equipment's nominal capability value and is deliberately not
/// duplicated here. Curves begin at failed condition, cover one physical value kind, and use
/// strictly increasing condition points. Resolution clamps at the failed endpoint and interpolates
/// deterministically toward the nominal pristine value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityConditionCurve {
    capability: CapabilityId,
    points: Vec<CapabilityConditionPoint>,
}

impl CapabilityConditionCurve {
    #[must_use]
    pub fn new(capability: CapabilityId, mut points: Vec<CapabilityConditionPoint>) -> Self {
        assert!(
            !points.is_empty(),
            "equipment capability condition curve {} must contain at least one point",
            capability.value()
        );
        points.sort_by_key(|point| point.condition());
        assert_eq!(
            points[0].condition(),
            Condition::FAILED,
            "equipment capability condition curve {} must begin at failed condition",
            capability.value()
        );
        let kind = points[0].value().kind();
        for point in &points {
            assert!(
                point.condition() < Condition::PRISTINE,
                "equipment capability condition curve {} must not duplicate the implicit pristine endpoint",
                capability.value()
            );
            assert_eq!(
                point.value().kind(),
                kind,
                "equipment capability condition curve {} mixes physical value kinds",
                capability.value()
            );
        }
        for pair in points.windows(2) {
            assert!(
                pair[0].condition() < pair[1].condition(),
                "equipment capability condition curve {} contains duplicate condition points",
                capability.value()
            );
        }
        Self { capability, points }
    }

    #[must_use]
    pub const fn capability(&self) -> CapabilityId {
        self.capability
    }

    #[must_use]
    pub fn points(&self) -> &[CapabilityConditionPoint] {
        &self.points
    }

    pub(crate) fn value_kind(&self) -> CapabilityValueKind {
        self.points[0].value().kind()
    }

    pub(super) fn assert_monotonic_toward(&self, nominal: CapabilityValue) {
        let failed = self.points[0].value();
        let direction = failed
            .compare(nominal)
            .unwrap_or_else(|| panic!("condition curve and nominal capability kinds must match"));
        let mut previous = failed;
        for point in &self.points[1..] {
            let current = point.value();
            let previous_to_current = previous
                .compare(current)
                .unwrap_or_else(|| panic!("condition curve value kinds must match"));
            let current_to_nominal = current.compare(nominal).unwrap_or_else(|| {
                panic!("condition curve and nominal capability kinds must match")
            });
            let monotonic = match direction {
                Ordering::Less => {
                    previous_to_current != Ordering::Greater
                        && current_to_nominal != Ordering::Greater
                }
                Ordering::Greater => {
                    previous_to_current != Ordering::Less && current_to_nominal != Ordering::Less
                }
                Ordering::Equal => current == nominal,
            };
            assert!(
                monotonic,
                "equipment capability condition curve {} reverses or overshoots while approaching its nominal pristine value",
                self.capability.value()
            );
            previous = current;
        }
    }
}
