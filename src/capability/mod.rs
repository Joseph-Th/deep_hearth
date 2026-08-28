//! Typed authored capability requirements and deterministic profile evaluation.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::core::arithmetic::scale_u128_fraction_floor;
use crate::core::quantity::{Mass, MassFlow, Power, Pressure, Temperature};

/// Stable authored identifier for one named capability dimension.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CapabilityId(u32);

impl CapabilityId {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        assert!(value != 0, "capability id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

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

/// Immutable authored metadata for one named capability dimension.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityDefinition {
    id: CapabilityId,
    name: String,
    kind: CapabilityValueKind,
}

impl CapabilityDefinition {
    #[must_use]
    pub fn new(id: CapabilityId, name: impl Into<String>, kind: CapabilityValueKind) -> Self {
        let name = name.into();
        assert!(
            !name.trim().is_empty(),
            "capability definition name must not be empty"
        );
        Self { id, name, kind }
    }

    #[must_use]
    pub const fn id(&self) -> CapabilityId {
        self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn kind(&self) -> CapabilityValueKind {
        self.kind
    }
}

/// Immutable deterministic authored capability lookup table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CapabilityRegistry {
    definitions: BTreeMap<CapabilityId, CapabilityDefinition>,
}

impl CapabilityRegistry {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            definitions: BTreeMap::new(),
        }
    }

    pub(crate) fn register_capability(&mut self, definition: CapabilityDefinition) {
        let id = definition.id();
        assert!(
            self.definitions.insert(id, definition).is_none(),
            "duplicate capability id {}",
            id.value()
        );
    }

    #[must_use]
    pub fn get_capability(&self, id: CapabilityId) -> Option<&CapabilityDefinition> {
        self.definitions.get(&id)
    }
}

/// Comparison applied to a provider value against an authored requirement threshold.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CapabilityComparison {
    AtLeast,
    AtMost,
}

/// One typed capability condition required before an operation may be resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CapabilityRequirement {
    capability: CapabilityId,
    comparison: CapabilityComparison,
    threshold: CapabilityValue,
}

impl CapabilityRequirement {
    #[must_use]
    pub const fn new(
        capability: CapabilityId,
        comparison: CapabilityComparison,
        threshold: CapabilityValue,
    ) -> Self {
        Self {
            capability,
            comparison,
            threshold,
        }
    }

    #[must_use]
    pub const fn capability(self) -> CapabilityId {
        self.capability
    }

    #[must_use]
    pub const fn comparison(self) -> CapabilityComparison {
        self.comparison
    }

    #[must_use]
    pub const fn threshold(self) -> CapabilityValue {
        self.threshold
    }
}

/// Runtime/view value containing the capabilities currently supplied by one provider or aggregate.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CapabilityProfile {
    values: BTreeMap<CapabilityId, CapabilityValue>,
}

/// Read-only source of typed capability values.
///
/// Static profiles and runtime-adjusted providers share this interface so evaluators do not need
/// to materialize temporary maps when capability values depend on condition or another owner.
pub trait CapabilitySource {
    fn get_capability(&self, capability: CapabilityId) -> Option<CapabilityValue>;
}

impl CapabilityProfile {
    /// Builds a deterministic profile and rejects repeated capability IDs.
    pub fn new(
        entries: impl IntoIterator<Item = (CapabilityId, CapabilityValue)>,
    ) -> Result<Self, CapabilityProfileError> {
        let mut values = BTreeMap::new();
        for (capability, value) in entries {
            if values.insert(capability, value).is_some() {
                return Err(CapabilityProfileError::DuplicateCapability { capability });
            }
        }
        Ok(Self { values })
    }

    #[must_use]
    pub fn get_capability(&self, capability: CapabilityId) -> Option<CapabilityValue> {
        self.values.get(&capability).copied()
    }

    /// Iterates deterministic capability/value pairs in stable authored-ID order.
    pub fn entries(&self) -> impl Iterator<Item = (CapabilityId, CapabilityValue)> + '_ {
        self.values
            .iter()
            .map(|(capability, value)| (*capability, *value))
    }
}

impl CapabilitySource for CapabilityProfile {
    fn get_capability(&self, capability: CapabilityId) -> Option<CapabilityValue> {
        CapabilityProfile::get_capability(self, capability)
    }
}

/// Invalid capability profile construction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityProfileError {
    DuplicateCapability { capability: CapabilityId },
}

impl Display for CapabilityProfileError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateCapability { capability } => write!(
                formatter,
                "capability profile contains duplicate capability {}",
                capability.value()
            ),
        }
    }
}

impl Error for CapabilityProfileError {}

/// Reason a provider profile cannot satisfy an authored capability requirement set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityEvaluationError {
    UnknownDefinition {
        capability: CapabilityId,
    },
    RequirementKindMismatch {
        capability: CapabilityId,
        expected: CapabilityValueKind,
        found: CapabilityValueKind,
    },
    MissingCapability {
        capability: CapabilityId,
    },
    ProfileKindMismatch {
        capability: CapabilityId,
        expected: CapabilityValueKind,
        found: CapabilityValueKind,
    },
    ThresholdNotMet {
        capability: CapabilityId,
        comparison: CapabilityComparison,
        required: CapabilityValue,
        provided: CapabilityValue,
    },
}

impl Display for CapabilityEvaluationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownDefinition { capability } => write!(
                formatter,
                "unknown capability definition {}",
                capability.value()
            ),
            Self::RequirementKindMismatch {
                capability,
                expected,
                found,
            } => write!(
                formatter,
                "capability {} requirement uses {found:?} but definition requires {expected:?}",
                capability.value()
            ),
            Self::MissingCapability { capability } => write!(
                formatter,
                "capability profile does not provide capability {}",
                capability.value()
            ),
            Self::ProfileKindMismatch {
                capability,
                expected,
                found,
            } => write!(
                formatter,
                "capability {} profile uses {found:?} but definition requires {expected:?}",
                capability.value()
            ),
            Self::ThresholdNotMet {
                capability,
                comparison,
                ..
            } => write!(
                formatter,
                "capability {} does not satisfy {comparison:?} threshold",
                capability.value()
            ),
        }
    }
}

impl Error for CapabilityEvaluationError {}

/// Validates and evaluates requirements against one explicit capability source.
pub fn evaluate_capabilities(
    registry: &CapabilityRegistry,
    source: &(impl CapabilitySource + ?Sized),
    requirements: &[CapabilityRequirement],
) -> Result<(), CapabilityEvaluationError> {
    for requirement in requirements {
        let capability = requirement.capability();
        let Some(definition) = registry.get_capability(capability) else {
            return Err(CapabilityEvaluationError::UnknownDefinition { capability });
        };
        if requirement.threshold().kind() != definition.kind() {
            return Err(CapabilityEvaluationError::RequirementKindMismatch {
                capability,
                expected: definition.kind(),
                found: requirement.threshold().kind(),
            });
        }
        let Some(provided) = source.get_capability(capability) else {
            return Err(CapabilityEvaluationError::MissingCapability { capability });
        };
        if provided.kind() != definition.kind() {
            return Err(CapabilityEvaluationError::ProfileKindMismatch {
                capability,
                expected: definition.kind(),
                found: provided.kind(),
            });
        }
        let Some(ordering) = provided.compare(requirement.threshold()) else {
            return Err(CapabilityEvaluationError::ProfileKindMismatch {
                capability,
                expected: definition.kind(),
                found: provided.kind(),
            });
        };
        let is_satisfied = match requirement.comparison() {
            CapabilityComparison::AtLeast => ordering != Ordering::Less,
            CapabilityComparison::AtMost => ordering != Ordering::Greater,
        };
        if !is_satisfied {
            return Err(CapabilityEvaluationError::ThresholdNotMet {
                capability,
                comparison: requirement.comparison(),
                required: requirement.threshold(),
                provided,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
