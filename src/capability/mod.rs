//! Typed authored capability requirements and deterministic profile evaluation.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

mod evaluation;
mod value;

pub use evaluation::{
    CapabilityComparison, CapabilityEvaluationError, CapabilityRequirement, evaluate_capabilities,
};
pub(crate) use value::interpolate_capability_value;
pub use value::{CapabilityValue, CapabilityValueKind};

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

/// Authored direction in which a capability becomes strictly more useful.
///
/// This is intentionally separate from one process requirement's `AtLeast`/`AtMost` comparison:
/// requirements decide whether a provider can perform a specific operation, while improvement
/// direction describes the capability dimension itself for upgrade and degradation semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CapabilityImprovement {
    Higher,
    Lower,
}

impl CapabilityImprovement {
    #[must_use]
    pub(crate) const fn is_improvement(self, ordering: Ordering) -> bool {
        matches!(
            (self, ordering),
            (Self::Higher, Ordering::Greater) | (Self::Lower, Ordering::Less)
        )
    }

    #[must_use]
    pub(crate) const fn is_regression(self, ordering: Ordering) -> bool {
        matches!(
            (self, ordering),
            (Self::Higher, Ordering::Less) | (Self::Lower, Ordering::Greater)
        )
    }
}

/// Immutable authored metadata for one named capability dimension.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityDefinition {
    id: CapabilityId,
    name: String,
    kind: CapabilityValueKind,
    improvement: Option<CapabilityImprovement>,
}

impl CapabilityDefinition {
    #[must_use]
    pub fn new(id: CapabilityId, name: impl Into<String>, kind: CapabilityValueKind) -> Self {
        Self::new_internal(id, name, kind, None)
    }

    /// Builds a capability dimension whose better/worse ordering is physically meaningful.
    #[must_use]
    pub fn new_with_improvement(
        id: CapabilityId,
        name: impl Into<String>,
        kind: CapabilityValueKind,
        improvement: CapabilityImprovement,
    ) -> Self {
        Self::new_internal(id, name, kind, Some(improvement))
    }

    fn new_internal(
        id: CapabilityId,
        name: impl Into<String>,
        kind: CapabilityValueKind,
        improvement: Option<CapabilityImprovement>,
    ) -> Self {
        let name = name.into();
        assert!(
            !name.trim().is_empty(),
            "capability definition name must not be empty"
        );
        Self {
            id,
            name,
            kind,
            improvement,
        }
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

    /// Returns the authored direction of improvement when this capability has one.
    #[must_use]
    pub const fn improvement(&self) -> Option<CapabilityImprovement> {
        self.improvement
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

/// Runtime/view value containing the capabilities currently supplied by one provider or aggregate.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CapabilityProfile {
    values: BTreeMap<CapabilityId, CapabilityValue>,
}

/// Read-only source of typed capability values.
///
/// Static profiles and runtime-adjusted providers share this interface so evaluators do not need
/// to materialize intermediate maps when capability values depend on condition or another owner.
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

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
