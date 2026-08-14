//! Equipment capability-provider resolution; sibling definitions and state remain separate static and runtime sources of truth.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::{
    CapabilityId, CapabilitySource, CapabilityValue, interpolate_capability_value,
};
use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::maintenance::{Condition, MaintenanceBand, MaintenanceThresholds};
use crate::registry::Registries;
use crate::structural::{StructuralElementId, StructuralLifecycle};

use super::definitions::{CapabilityConditionCurve, EquipmentDefinition, EquipmentDefinitionId};
use super::state::{EquipmentId, EquipmentOperationTrace, EquipmentRecord};

/// Revision-bound equipment provider selection carried by a resolved operation until start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ValidatedEquipmentUse {
    expected_equipment_revision: u64,
    expected_structure_revision: Option<u64>,
    support: Option<StructuralElementId>,
    trace: EquipmentOperationTrace,
}

impl ValidatedEquipmentUse {
    pub(crate) const fn expected_equipment_revision(self) -> u64 {
        self.expected_equipment_revision
    }

    pub(crate) const fn expected_structure_revision(self) -> Option<u64> {
        self.expected_structure_revision
    }

    pub(crate) const fn support(self) -> Option<StructuralElementId> {
        self.support
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
    expected_equipment_revision: u64,
    expected_structure_revision: Option<u64>,
}

impl<'state> ResolvedEquipmentProvider<'state> {
    #[must_use]
    pub const fn id(&self) -> EquipmentId {
        self.record.id()
    }

    #[must_use]
    pub const fn condition(&self) -> Condition {
        self.record.condition()
    }

    #[must_use]
    pub const fn mass(&self) -> Mass {
        self.definition.mass()
    }

    #[must_use]
    pub fn get_capability(&self, capability: CapabilityId) -> Option<CapabilityValue> {
        resolve_equipment_capability(self.definition, self.record.condition(), capability)
    }

    #[must_use]
    pub const fn maintenance_thresholds(&self) -> MaintenanceThresholds {
        self.definition.maintenance_thresholds()
    }

    #[must_use]
    pub fn maintenance_band(&self) -> MaintenanceBand {
        self.maintenance_thresholds().classify(self.condition())
    }

    pub(crate) const fn validated_use(&self) -> ValidatedEquipmentUse {
        ValidatedEquipmentUse {
            expected_equipment_revision: self.expected_equipment_revision,
            expected_structure_revision: self.expected_structure_revision,
            support: self.record.supported_by(),
            trace: EquipmentOperationTrace::new(
                self.record.id(),
                self.record.definition(),
                self.record.condition(),
            ),
        }
    }
}

impl CapabilitySource for ResolvedEquipmentProvider<'_> {
    fn get_capability(&self, capability: CapabilityId) -> Option<CapabilityValue> {
        ResolvedEquipmentProvider::get_capability(self, capability)
    }
}

fn resolve_curve_value(
    curve: &CapabilityConditionCurve,
    nominal: CapabilityValue,
    condition: Condition,
) -> CapabilityValue {
    let points = curve.points();
    let mut degraded = points[0];
    if condition <= degraded.condition() {
        return degraded.value();
    }

    for improved in &points[1..] {
        if condition <= improved.condition() {
            let numerator =
                condition.parts_per_million() - degraded.condition().parts_per_million();
            let denominator =
                improved.condition().parts_per_million() - degraded.condition().parts_per_million();
            return match interpolate_capability_value(
                degraded.value(),
                improved.value(),
                numerator,
                denominator,
            ) {
                Some(value) => value,
                None => panic!(
                    "equipment capability condition curve {} became invalid after registry assembly",
                    curve.capability().value()
                ),
            };
        }
        degraded = *improved;
    }

    let numerator = condition.parts_per_million() - degraded.condition().parts_per_million();
    let denominator =
        Condition::PRISTINE.parts_per_million() - degraded.condition().parts_per_million();
    match interpolate_capability_value(degraded.value(), nominal, numerator, denominator) {
        Some(value) => value,
        None => panic!(
            "equipment capability condition curve {} disagrees with its nominal capability",
            curve.capability().value()
        ),
    }
}

pub(crate) fn resolve_equipment_capability(
    definition: &EquipmentDefinition,
    condition: Condition,
    capability: CapabilityId,
) -> Option<CapabilityValue> {
    let nominal = definition.capabilities().get_capability(capability)?;
    Some(
        match definition.get_capability_condition_curve(capability) {
            Some(curve) => resolve_curve_value(curve, nominal, condition),
            None => nominal,
        },
    )
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
    UnknownStructuralSupport {
        equipment: EquipmentId,
        element: StructuralElementId,
    },
    StructuralSupportNotActive {
        equipment: EquipmentId,
        element: StructuralElementId,
        lifecycle: StructuralLifecycle,
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
            Self::UnknownStructuralSupport { equipment, element } => write!(
                formatter,
                "equipment {} references missing structural support {}",
                equipment.value(),
                element.value()
            ),
            Self::StructuralSupportNotActive {
                equipment,
                element,
                lifecycle,
            } => write!(
                formatter,
                "equipment {} structural support {} is {lifecycle:?} and cannot authorize a new operation",
                equipment.value(),
                element.value()
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
    let expected_structure_revision = if let Some(element) = record.supported_by() {
        let Some(support) = state.structures().get_element(element) else {
            return Err(EquipmentProviderError::UnknownStructuralSupport { equipment, element });
        };
        if support.lifecycle() != StructuralLifecycle::Active {
            return Err(EquipmentProviderError::StructuralSupportNotActive {
                equipment,
                element,
                lifecycle: support.lifecycle(),
            });
        }
        Some(state.structures().revision())
    } else {
        None
    };
    Ok(ResolvedEquipmentProvider {
        record,
        definition,
        expected_equipment_revision: state.equipment().revision(),
        expected_structure_revision,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{
        CapabilityComparison, CapabilityDefinition, CapabilityId, CapabilityProfile,
        CapabilityRequirement, CapabilityValue, CapabilityValueKind, evaluate_capabilities,
    };
    use crate::content::make_test_registries_with_equipment;
    use crate::content::{FORM_LOG, MATERIAL_WOOD, STRUCTURAL_PROFILE_AXIAL_COMPRESSION};
    use crate::core::quantity::{Area, Force, Mass};
    use crate::core::time::WorldSeed;
    use crate::equipment::{
        CapabilityConditionCurve, CapabilityConditionPoint, EquipmentDefinition,
        EquipmentDefinitionId, add_equipment, validate_mount_equipment,
    };
    use crate::spatial::{VoxelBounds, VoxelCoord};
    use crate::structural::{
        StructuralLoadKind, add_structural_element, materialize_structural_element_for_test,
        validate_activate_structural_element, validate_set_structural_load,
    };

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
            provider.get_capability(TEST_CAPABILITY),
            Some(CapabilityValue::Mass(Mass::from_milligrams(75_000)))
        );
    }

    #[test]
    fn provider_resolution_derates_authored_capability_from_runtime_condition() {
        let profile = match CapabilityProfile::new([(
            TEST_CAPABILITY,
            CapabilityValue::Mass(Mass::from_milligrams(100_000)),
        )]) {
            Ok(profile) => profile,
            Err(error) => panic!("capability fixture failed: {error}"),
        };
        let thresholds = match MaintenanceThresholds::new(condition(600_000), condition(250_000)) {
            Ok(thresholds) => thresholds,
            Err(error) => panic!("maintenance fixture failed: {error}"),
        };
        let curve = CapabilityConditionCurve::new(
            TEST_CAPABILITY,
            vec![
                CapabilityConditionPoint::new(
                    Condition::FAILED,
                    CapabilityValue::Mass(Mass::from_milligrams(25_000)),
                ),
                CapabilityConditionPoint::new(
                    condition(500_000),
                    CapabilityValue::Mass(Mass::from_milligrams(50_000)),
                ),
            ],
        );
        let registries = make_test_registries_with_equipment(
            CapabilityDefinition::new(
                TEST_CAPABILITY,
                "test supported mass",
                CapabilityValueKind::Mass,
            ),
            EquipmentDefinition::new_with_capability_condition_curves(
                TEST_DEFINITION,
                "condition-sensitive fixture",
                Mass::from_milligrams(25_000),
                profile,
                thresholds,
                vec![curve],
            ),
        );
        let mut state = AppState::new(WorldSeed::new(31));
        let equipment =
            match add_equipment(&registries, &mut state, TEST_DEFINITION, condition(750_000)) {
                Ok(equipment) => equipment,
                Err(error) => panic!("equipment creation failed: {error}"),
            };

        let provider = match resolve_equipment_provider(&registries, &state, equipment) {
            Ok(provider) => provider,
            Err(error) => panic!("provider resolution failed: {error}"),
        };

        assert_eq!(
            provider.get_capability(TEST_CAPABILITY),
            Some(CapabilityValue::Mass(Mass::from_milligrams(75_000)))
        );
        assert_eq!(
            evaluate_capabilities(
                registries.capabilities(),
                &provider,
                &[CapabilityRequirement::new(
                    TEST_CAPABILITY,
                    CapabilityComparison::AtLeast,
                    CapabilityValue::Mass(Mass::from_milligrams(75_000)),
                )],
            ),
            Ok(())
        );
        assert!(
            evaluate_capabilities(
                registries.capabilities(),
                &provider,
                &[CapabilityRequirement::new(
                    TEST_CAPABILITY,
                    CapabilityComparison::AtLeast,
                    CapabilityValue::Mass(Mass::from_milligrams(75_001)),
                )],
            )
            .is_err()
        );
    }

    #[test]
    fn collapsed_structural_support_blocks_new_equipment_use() {
        let profile = match CapabilityProfile::new([(
            TEST_CAPABILITY,
            CapabilityValue::Mass(Mass::from_milligrams(100_000)),
        )]) {
            Ok(profile) => profile,
            Err(error) => panic!("support-aware capability fixture failed: {error}"),
        };
        let thresholds = match MaintenanceThresholds::new(condition(600_000), condition(250_000)) {
            Ok(thresholds) => thresholds,
            Err(error) => panic!("support-aware maintenance fixture failed: {error}"),
        };
        let registries = make_test_registries_with_equipment(
            CapabilityDefinition::new(
                TEST_CAPABILITY,
                "support-aware fixture capability",
                CapabilityValueKind::Mass,
            ),
            EquipmentDefinition::new(
                TEST_DEFINITION,
                "support-aware fixture",
                Mass::from_milligrams(25_000),
                profile,
                thresholds,
            ),
        );
        let mut state = AppState::new(WorldSeed::new(0x8200_0003));
        let bounds = match VoxelBounds::new(VoxelCoord::new(0, 0, 0), VoxelCoord::new(1, 1, 1)) {
            Ok(bounds) => bounds,
            Err(error) => panic!("support-aware structural bounds failed: {error}"),
        };
        let support = match add_structural_element(
            &registries,
            &mut state,
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            bounds,
            Area::from_square_millimeters(1_000),
            true,
        ) {
            Ok(element) => element,
            Err(error) => panic!("support-aware structural fixture failed: {error}"),
        };
        materialize_structural_element_for_test(
            &registries,
            &mut state,
            support,
            FORM_LOG,
            Mass::from_milligrams(1),
        );
        let activation = match validate_activate_structural_element(&registries, &state, support) {
            Ok(token) => token,
            Err(error) => panic!("support-aware activation validation failed: {error}"),
        };
        if let Err(error) = activation.commit(&mut state) {
            panic!("support-aware activation commit failed: {error}");
        }
        let equipment = match add_equipment(
            &registries,
            &mut state,
            TEST_DEFINITION,
            Condition::PRISTINE,
        ) {
            Ok(equipment) => equipment,
            Err(error) => panic!("support-aware equipment fixture failed: {error}"),
        };
        let mount = match validate_mount_equipment(&registries, &state, equipment, support) {
            Ok(token) => token,
            Err(error) => panic!("support-aware mount validation failed: {error}"),
        };
        if let Err(error) = mount.commit(&mut state) {
            panic!("support-aware mount commit failed: {error}");
        }
        let overload = match validate_set_structural_load(
            &registries,
            &state,
            support,
            StructuralLoadKind::Snow,
            Force::from_millinewtons(50_000_000),
        ) {
            Ok(token) => token,
            Err(error) => panic!("support-aware overload validation failed: {error}"),
        };
        if let Err(error) = overload.commit(&mut state) {
            panic!("support-aware overload commit failed: {error}");
        }

        assert!(matches!(
            resolve_equipment_provider(&registries, &state, equipment),
            Err(EquipmentProviderError::StructuralSupportNotActive {
                equipment: rejected_equipment,
                element,
                lifecycle: StructuralLifecycle::Failed,
            }) if rejected_equipment == equipment && element == support
        ));
    }
}
