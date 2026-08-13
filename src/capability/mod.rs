//! Typed authored capability requirements and deterministic profile evaluation for future physical resolvers.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::core::quantity::{
    ElectricCurrent, ElectricPotential, ElectricalResistance, Energy, Mass, Power, Pressure,
    Temperature, Volume, VolumetricFlow,
};
use crate::maintenance::Condition;

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
///
/// New dimensions such as pressure, power, voltage, or precision must become explicit enum variants
/// backed by typed quantities. The capability layer intentionally has no generic numeric "tier".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CapabilityValueKind {
    Presence,
    Mass,
    Temperature,
    Energy,
    Pressure,
    Power,
    ElectricPotential,
    ElectricCurrent,
    ElectricalResistance,
    Volume,
    VolumetricFlow,
    Condition,
}

/// Typed value exposed by a capability provider or required by an operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CapabilityValue {
    Present,
    Mass(Mass),
    Temperature(Temperature),
    Energy(Energy),
    Pressure(Pressure),
    Power(Power),
    ElectricPotential(ElectricPotential),
    ElectricCurrent(ElectricCurrent),
    ElectricalResistance(ElectricalResistance),
    Volume(Volume),
    VolumetricFlow(VolumetricFlow),
    Condition(Condition),
}

impl CapabilityValue {
    #[must_use]
    pub const fn kind(self) -> CapabilityValueKind {
        match self {
            Self::Present => CapabilityValueKind::Presence,
            Self::Mass(_) => CapabilityValueKind::Mass,
            Self::Temperature(_) => CapabilityValueKind::Temperature,
            Self::Energy(_) => CapabilityValueKind::Energy,
            Self::Pressure(_) => CapabilityValueKind::Pressure,
            Self::Power(_) => CapabilityValueKind::Power,
            Self::ElectricPotential(_) => CapabilityValueKind::ElectricPotential,
            Self::ElectricCurrent(_) => CapabilityValueKind::ElectricCurrent,
            Self::ElectricalResistance(_) => CapabilityValueKind::ElectricalResistance,
            Self::Volume(_) => CapabilityValueKind::Volume,
            Self::VolumetricFlow(_) => CapabilityValueKind::VolumetricFlow,
            Self::Condition(_) => CapabilityValueKind::Condition,
        }
    }

    fn magnitude(self) -> u128 {
        match self {
            Self::Present => 1,
            Self::Mass(value) => u128::from(value.milligrams()),
            Self::Temperature(value) => u128::from(value.millikelvin()),
            Self::Energy(value) => value.nanojoules(),
            Self::Pressure(value) => u128::from(value.pascals()),
            Self::Power(value) => value.picowatts(),
            Self::ElectricPotential(value) => u128::from(value.microvolts()),
            Self::ElectricCurrent(value) => u128::from(value.microamperes()),
            Self::ElectricalResistance(value) => u128::from(value.microohms()),
            Self::Volume(value) => u128::from(value.microliters()),
            Self::VolumetricFlow(value) => u128::from(value.microliters_per_second()),
            Self::Condition(value) => u128::from(value.parts_per_million()),
        }
    }

    fn compare(self, other: Self) -> Option<Ordering> {
        if self.kind() != other.kind() {
            return None;
        }
        Some(self.magnitude().cmp(&other.magnitude()))
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

    #[cfg(test)]
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

/// Validates and evaluates requirements against one explicit provider profile.
pub fn evaluate_capabilities(
    registry: &CapabilityRegistry,
    profile: &CapabilityProfile,
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
        let Some(provided) = profile.get_capability(capability) else {
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
mod tests {
    use super::*;

    const CHAMBER_TEMPERATURE: CapabilityId = CapabilityId::new(1);
    const LOAD_CAPACITY: CapabilityId = CapabilityId::new(2);

    fn make_registry() -> CapabilityRegistry {
        let mut registry = CapabilityRegistry::new();
        registry.register_capability(CapabilityDefinition::new(
            CHAMBER_TEMPERATURE,
            "test chamber temperature",
            CapabilityValueKind::Temperature,
        ));
        registry.register_capability(CapabilityDefinition::new(
            LOAD_CAPACITY,
            "test load capacity",
            CapabilityValueKind::Mass,
        ));
        registry
    }

    #[test]
    fn typed_capabilities_enforce_at_least_and_at_most_without_generic_tiers() {
        let registry = make_registry();
        let profile = match CapabilityProfile::new([
            (
                CHAMBER_TEMPERATURE,
                CapabilityValue::Temperature(Temperature::from_millikelvin(1_500_000)),
            ),
            (
                LOAD_CAPACITY,
                CapabilityValue::Mass(Mass::from_milligrams(20_000)),
            ),
        ]) {
            Ok(profile) => profile,
            Err(error) => panic!("profile fixture failed: {error}"),
        };
        let requirements = [
            CapabilityRequirement::new(
                CHAMBER_TEMPERATURE,
                CapabilityComparison::AtLeast,
                CapabilityValue::Temperature(Temperature::from_millikelvin(1_200_000)),
            ),
            CapabilityRequirement::new(
                LOAD_CAPACITY,
                CapabilityComparison::AtMost,
                CapabilityValue::Mass(Mass::from_milligrams(25_000)),
            ),
        ];

        assert_eq!(
            evaluate_capabilities(&registry, &profile, &requirements),
            Ok(())
        );
    }

    #[test]
    fn wrong_physical_dimension_is_rejected_before_threshold_comparison() {
        let registry = make_registry();
        let profile = match CapabilityProfile::new([(
            CHAMBER_TEMPERATURE,
            CapabilityValue::Temperature(Temperature::from_millikelvin(1_500_000)),
        )]) {
            Ok(profile) => profile,
            Err(error) => panic!("profile fixture failed: {error}"),
        };
        let requirement = CapabilityRequirement::new(
            CHAMBER_TEMPERATURE,
            CapabilityComparison::AtLeast,
            CapabilityValue::Energy(Energy::from_nanojoules(1)),
        );

        assert_eq!(
            evaluate_capabilities(&registry, &profile, &[requirement]),
            Err(CapabilityEvaluationError::RequirementKindMismatch {
                capability: CHAMBER_TEMPERATURE,
                expected: CapabilityValueKind::Temperature,
                found: CapabilityValueKind::Energy,
            })
        );
    }

    #[test]
    fn insufficient_capability_reports_requirement_and_provided_values() {
        let registry = make_registry();
        let provided = CapabilityValue::Temperature(Temperature::from_millikelvin(900_000));
        let required = CapabilityValue::Temperature(Temperature::from_millikelvin(1_200_000));
        let profile = match CapabilityProfile::new([(CHAMBER_TEMPERATURE, provided)]) {
            Ok(profile) => profile,
            Err(error) => panic!("profile fixture failed: {error}"),
        };
        let requirement = CapabilityRequirement::new(
            CHAMBER_TEMPERATURE,
            CapabilityComparison::AtLeast,
            required,
        );

        assert_eq!(
            evaluate_capabilities(&registry, &profile, &[requirement]),
            Err(CapabilityEvaluationError::ThresholdNotMet {
                capability: CHAMBER_TEMPERATURE,
                comparison: CapabilityComparison::AtLeast,
                required,
                provided,
            })
        );
    }
}
