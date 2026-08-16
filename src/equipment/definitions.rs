//! Immutable maintainable-equipment definitions; sibling state stores only persistent references and changing condition.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::capability::{
    CapabilityId, CapabilityProfile, CapabilityRegistry, CapabilityValue, CapabilityValueKind,
};
use crate::core::quantity::Mass;
use crate::maintenance::{Condition, MaintenanceThresholds};
use crate::material::{CommodityKey, MaterialRegistry};

/// Stable authored identifier for one equipment definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EquipmentDefinitionId(u32);

impl EquipmentDefinitionId {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        assert!(value != 0, "equipment definition id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Authored replacement-material service for one equipment class.
///
/// The profile deliberately models the physical consequence visible to the current game: exact
/// replacement stock leaves inventory and the maintained machine returns to an authored condition.
/// Labor/tool/time requirements can extend this resolver when those owners exist without reopening a
/// free condition mutation path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EquipmentMaintenanceProfile {
    replacement: CommodityKey,
    replacement_mass: Mass,
    restored_condition: Condition,
}

impl EquipmentMaintenanceProfile {
    #[must_use]
    pub fn new(
        replacement: CommodityKey,
        replacement_mass: Mass,
        restored_condition: Condition,
    ) -> Self {
        assert!(
            !replacement_mass.is_zero(),
            "equipment maintenance replacement mass must be nonzero"
        );
        assert!(
            restored_condition > Condition::FAILED,
            "equipment maintenance restored condition must be above failed"
        );
        Self {
            replacement,
            replacement_mass,
            restored_condition,
        }
    }

    #[must_use]
    pub const fn replacement(self) -> CommodityKey {
        self.replacement
    }

    #[must_use]
    pub const fn replacement_mass(self) -> Mass {
        self.replacement_mass
    }

    #[must_use]
    pub const fn restored_condition(self) -> Condition {
        self.restored_condition
    }
}

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
        assert_ne!(
            kind,
            CapabilityValueKind::Presence,
            "equipment capability condition curve {} cannot interpolate a Presence capability; discrete capability availability requires an explicit policy",
            capability.value()
        );
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

    pub(crate) fn value_kind(&self) -> crate::capability::CapabilityValueKind {
        self.points[0].value().kind()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continuous_condition_curve_rejects_presence_capability() {
        let capability = CapabilityId::new(810_001);
        let result = std::panic::catch_unwind(|| {
            CapabilityConditionCurve::new(
                capability,
                vec![CapabilityConditionPoint::new(
                    Condition::FAILED,
                    CapabilityValue::Present,
                )],
            )
        });

        assert!(result.is_err());
    }
}

/// Immutable authored properties shared by all runtime instances of one equipment class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EquipmentDefinition {
    id: EquipmentDefinitionId,
    name: String,
    mass: Mass,
    capabilities: CapabilityProfile,
    capability_condition_curves: BTreeMap<CapabilityId, CapabilityConditionCurve>,
    maintenance_thresholds: MaintenanceThresholds,
    maintenance_profile: Option<EquipmentMaintenanceProfile>,
}

impl EquipmentDefinition {
    #[must_use]
    pub fn new(
        id: EquipmentDefinitionId,
        name: impl Into<String>,
        mass: Mass,
        capabilities: CapabilityProfile,
        maintenance_thresholds: MaintenanceThresholds,
    ) -> Self {
        Self::new_with_capability_condition_curves(
            id,
            name,
            mass,
            capabilities,
            maintenance_thresholds,
            Vec::new(),
        )
    }

    #[must_use]
    pub fn new_with_capability_condition_curves(
        id: EquipmentDefinitionId,
        name: impl Into<String>,
        mass: Mass,
        capabilities: CapabilityProfile,
        maintenance_thresholds: MaintenanceThresholds,
        capability_condition_curves: Vec<CapabilityConditionCurve>,
    ) -> Self {
        let name = name.into();
        assert!(
            !name.trim().is_empty(),
            "equipment definition name must not be empty"
        );
        assert!(!mass.is_zero(), "equipment definition mass must be nonzero");
        let mut curves_by_capability = BTreeMap::new();
        for curve in capability_condition_curves {
            let capability = curve.capability();
            let nominal = match capabilities.get_capability(capability) {
                Some(value) => value,
                None => panic!(
                    "equipment definition {} condition curve references missing nominal capability {}",
                    id.value(),
                    capability.value()
                ),
            };
            assert_eq!(
                nominal.kind(),
                curve.value_kind(),
                "equipment definition {} condition curve {} has wrong physical value kind",
                id.value(),
                capability.value()
            );
            assert!(
                curves_by_capability.insert(capability, curve).is_none(),
                "equipment definition {} contains duplicate condition curves for capability {}",
                id.value(),
                capability.value()
            );
        }
        Self {
            id,
            name,
            mass,
            capabilities,
            capability_condition_curves: curves_by_capability,
            maintenance_thresholds,
            maintenance_profile: None,
        }
    }

    /// Adds the authored replacement-material service available to runtime maintenance resolution.
    #[must_use]
    pub fn with_maintenance_profile(mut self, profile: EquipmentMaintenanceProfile) -> Self {
        assert!(
            profile.restored_condition() > self.maintenance_thresholds.warning_below(),
            "equipment definition {} maintenance service must restore into its normal condition band",
            self.id.value()
        );
        self.maintenance_profile = Some(profile);
        self
    }

    #[must_use]
    pub const fn id(&self) -> EquipmentDefinitionId {
        self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn mass(&self) -> Mass {
        self.mass
    }

    #[must_use]
    pub const fn capabilities(&self) -> &CapabilityProfile {
        &self.capabilities
    }

    #[must_use]
    pub fn get_capability_condition_curve(
        &self,
        capability: CapabilityId,
    ) -> Option<&CapabilityConditionCurve> {
        self.capability_condition_curves.get(&capability)
    }

    #[must_use]
    pub const fn maintenance_thresholds(&self) -> MaintenanceThresholds {
        self.maintenance_thresholds
    }

    #[must_use]
    pub const fn maintenance_profile(&self) -> Option<EquipmentMaintenanceProfile> {
        self.maintenance_profile
    }
}

/// Immutable deterministic authored equipment lookup table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EquipmentRegistry {
    definitions: BTreeMap<EquipmentDefinitionId, EquipmentDefinition>,
}

impl EquipmentRegistry {
    pub(crate) fn new(definitions: impl IntoIterator<Item = EquipmentDefinition>) -> Self {
        let mut by_id = BTreeMap::new();
        for definition in definitions {
            let id = definition.id();
            assert!(
                by_id.insert(id, definition).is_none(),
                "duplicate equipment definition id {}",
                id.value()
            );
        }
        Self { definitions: by_id }
    }

    #[must_use]
    pub fn get_equipment(&self, id: EquipmentDefinitionId) -> Option<&EquipmentDefinition> {
        self.definitions.get(&id)
    }

    pub(crate) fn validate_references(
        &self,
        capabilities: &CapabilityRegistry,
        materials: &MaterialRegistry,
    ) {
        for definition in self.definitions.values() {
            for (capability, value) in definition.capabilities().entries() {
                let Some(capability_definition) = capabilities.get_capability(capability) else {
                    panic!(
                        "equipment definition {} references missing capability {}",
                        definition.id().value(),
                        capability.value()
                    );
                };
                assert_eq!(
                    value.kind(),
                    capability_definition.kind(),
                    "equipment definition {} capability {} has wrong physical value kind",
                    definition.id().value(),
                    capability.value()
                );
            }
            if let Some(maintenance) = definition.maintenance_profile() {
                assert!(
                    materials
                        .get_material(maintenance.replacement().material())
                        .is_some(),
                    "equipment definition {} maintenance profile references missing material {}",
                    definition.id().value(),
                    maintenance.replacement().material().value()
                );
                assert!(
                    materials
                        .get_form(maintenance.replacement().form())
                        .is_some(),
                    "equipment definition {} maintenance profile references missing form {}",
                    definition.id().value(),
                    maintenance.replacement().form().value()
                );
            }
        }
    }
}
