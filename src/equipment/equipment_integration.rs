//! Equipment capability-provider resolution; sibling definitions and state remain separate static and runtime sources of truth.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::CapabilityProfile;
use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::maintenance::{Condition, MaintenanceBand, MaintenanceThresholds};
use crate::registry::Registries;

use super::definitions::{EquipmentDefinition, EquipmentDefinitionId};
use super::state::{EquipmentId, EquipmentOperationTrace, EquipmentRecord};

/// Revision-bound equipment provider selection carried by a resolved operation until start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ValidatedEquipmentUse {
    expected_revision: u64,
    trace: EquipmentOperationTrace,
}

impl ValidatedEquipmentUse {
    pub(crate) const fn expected_revision(self) -> u64 {
        self.expected_revision
    }

    pub(crate) const fn trace(self) -> EquipmentOperationTrace {
        self.trace
    }
}

/// Read-only resolved provider joining one runtime record to its immutable definition.
#[derive(Clone, Copy, Debug)]
pub struct ResolvedEquipmentProvider<'state> {
    record: &'state EquipmentRecord,
    definition: &'state EquipmentDefinition,
    expected_revision: u64,
}

impl<'state> ResolvedEquipmentProvider<'state> {
    #[must_use]
    pub const fn id(self) -> EquipmentId {
        self.record.id()
    }

    #[must_use]
    pub const fn condition(self) -> Condition {
        self.record.condition()
    }

    #[must_use]
    pub const fn mass(self) -> Mass {
        self.definition.mass()
    }

    #[must_use]
    pub const fn capabilities(self) -> &'state CapabilityProfile {
        self.definition.capabilities()
    }

    #[must_use]
    pub const fn maintenance_thresholds(self) -> MaintenanceThresholds {
        self.definition.maintenance_thresholds()
    }

    #[must_use]
    pub fn maintenance_band(self) -> MaintenanceBand {
        self.maintenance_thresholds().classify(self.condition())
    }

    pub(crate) const fn validated_use(self) -> ValidatedEquipmentUse {
        ValidatedEquipmentUse {
            expected_revision: self.expected_revision,
            trace: EquipmentOperationTrace::new(
                self.record.id(),
                self.record.definition(),
                self.record.condition(),
            ),
        }
    }
}

/// Failure to resolve a runtime equipment record into its immutable provider definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EquipmentProviderError {
    UnknownEquipment {
        equipment: EquipmentId,
    },
    UnknownDefinition {
        equipment: EquipmentId,
        definition: EquipmentDefinitionId,
    },
}

impl Display for EquipmentProviderError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownEquipment { equipment } => {
                write!(formatter, "unknown equipment id {}", equipment.value())
            }
            Self::UnknownDefinition {
                equipment,
                definition,
            } => write!(
                formatter,
                "equipment {} references unknown definition {}",
                equipment.value(),
                definition.value()
            ),
        }
    }
}

impl Error for EquipmentProviderError {}

/// Resolves static capability data and current condition without duplicating either source of truth.
pub fn resolve_equipment_provider<'state>(
    registries: &'state Registries,
    state: &'state AppState,
    equipment: EquipmentId,
) -> Result<ResolvedEquipmentProvider<'state>, EquipmentProviderError> {
    let Some(record) = state.equipment().get_equipment(equipment) else {
        return Err(EquipmentProviderError::UnknownEquipment { equipment });
    };
    let Some(definition) = registries.equipment().get_equipment(record.definition()) else {
        return Err(EquipmentProviderError::UnknownDefinition {
            equipment,
            definition: record.definition(),
        });
    };
    Ok(ResolvedEquipmentProvider {
        record,
        definition,
        expected_revision: state.equipment().revision(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{
        CapabilityDefinition, CapabilityId, CapabilityProfile, CapabilityValue, CapabilityValueKind,
    };
    use crate::content::make_test_registries_with_equipment;
    use crate::core::quantity::Mass;
    use crate::core::time::WorldSeed;
    use crate::equipment::{EquipmentDefinition, EquipmentDefinitionId, add_equipment};

    const TEST_CAPABILITY: CapabilityId = CapabilityId::new(820_001);
    const TEST_DEFINITION: EquipmentDefinitionId = EquipmentDefinitionId::new(820_001);

    fn condition(parts_per_million: u32) -> Condition {
        match Condition::new(parts_per_million) {
            Ok(condition) => condition,
            Err(error) => panic!("condition fixture failed: {error}"),
        }
    }

    #[test]
    fn provider_resolution_keeps_static_capability_and_runtime_condition_separate() {
        let profile = match CapabilityProfile::new([(
            TEST_CAPABILITY,
            CapabilityValue::Mass(Mass::from_milligrams(75_000)),
        )]) {
            Ok(profile) => profile,
            Err(error) => panic!("capability fixture failed: {error}"),
        };
        let thresholds = match MaintenanceThresholds::new(condition(600_000), condition(250_000)) {
            Ok(thresholds) => thresholds,
            Err(error) => panic!("maintenance fixture failed: {error}"),
        };
        let registries = make_test_registries_with_equipment(
            CapabilityDefinition::new(
                TEST_CAPABILITY,
                "test supported mass",
                CapabilityValueKind::Mass,
            ),
            EquipmentDefinition::new(
                TEST_DEFINITION,
                "test fixture",
                Mass::from_milligrams(25_000),
                profile,
                thresholds,
            ),
        );
        let mut state = AppState::new(WorldSeed::new(29));
        let equipment =
            match add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000)) {
                Ok(equipment) => equipment,
                Err(error) => panic!("equipment creation failed: {error}"),
            };

        let provider = match resolve_equipment_provider(&registries, &state, equipment) {
            Ok(provider) => provider,
            Err(error) => panic!("provider resolution failed: {error}"),
        };
        assert_eq!(provider.condition(), condition(500_000));
        assert_eq!(provider.mass(), Mass::from_milligrams(25_000));
        assert_eq!(provider.maintenance_band(), MaintenanceBand::Warning);
        assert_eq!(
            provider.capabilities().get_capability(TEST_CAPABILITY),
            Some(CapabilityValue::Mass(Mass::from_milligrams(75_000)))
        );
    }
}
