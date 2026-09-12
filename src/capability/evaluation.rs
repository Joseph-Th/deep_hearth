//! Typed capability requirement declarations and provider evaluation.

use std::cmp::Ordering;
use std::error::Error;
use std::fmt::{Display, Formatter};

use super::{
    CapabilityId, CapabilityRegistry, CapabilitySource, CapabilityValue, CapabilityValueKind,
};

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
