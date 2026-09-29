//! Defines immutable equipment classes, capabilities, assembly, upgrades, and maintenance.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::capability::{CapabilityId, CapabilityProfile};
use crate::core::quantity::Mass;
use crate::maintenance::MaintenanceThresholds;
use crate::material::MaterialAssemblyProfile;

mod condition;
mod maintenance;

pub use condition::{CapabilityConditionCurve, CapabilityConditionPoint};
pub use maintenance::EquipmentMaintenanceProfile;

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

#[cfg(test)]
#[path = "definitions_tests.rs"]
mod tests;

/// Immutable authored properties shared by all runtime instances of one equipment class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EquipmentDefinition {
    id: EquipmentDefinitionId,
    name: String,
    mass: Mass,
    requires_structural_support: bool,
    capabilities: CapabilityProfile,
    capability_condition_curves: BTreeMap<CapabilityId, CapabilityConditionCurve>,
    maintenance_thresholds: MaintenanceThresholds,
    maintenance_profile: Option<EquipmentMaintenanceProfile>,
    assembly_profile: Option<MaterialAssemblyProfile>,
    upgrade_profile: Option<EquipmentUpgradeProfile>,
}

/// Authored additive conversion from one existing equipment class into this definition.
///
/// An upgrade owns only added matter. The instance retains identity, condition, creation metadata,
/// and existing embodied material; validation appends exact consumed traces and updates only the
/// definition reference and total embodied mass.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EquipmentUpgradeProfile {
    from: EquipmentDefinitionId,
    additions: MaterialAssemblyProfile,
}

impl EquipmentUpgradeProfile {
    #[must_use]
    pub fn new(from: EquipmentDefinitionId, additions: MaterialAssemblyProfile) -> Self {
        Self { from, additions }
    }

    #[must_use]
    pub const fn from(&self) -> EquipmentDefinitionId {
        self.from
    }

    #[must_use]
    pub const fn additions(&self) -> &MaterialAssemblyProfile {
        &self.additions
    }
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
            curve.assert_monotonic_toward(nominal);
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
            requires_structural_support: false,
            capabilities,
            capability_condition_curves: curves_by_capability,
            maintenance_thresholds,
            maintenance_profile: None,
            assembly_profile: None,
            upgrade_profile: None,
        }
    }

    /// Requires an active structural support assignment before this equipment can authorize work.
    ///
    /// Portable tools and small devices remain usable unmounted. Installed machinery uses this
    /// requirement so its mass, site condition, and structural consequences cannot be bypassed by
    /// simply leaving the runtime support field empty.
    #[must_use]
    pub const fn with_required_structural_support(mut self) -> Self {
        self.requires_structural_support = true;
        self
    }

    /// Adds the authored replacement-material service available to runtime maintenance resolution.
    #[must_use]
    pub fn with_maintenance_profile(mut self, profile: EquipmentMaintenanceProfile) -> Self {
        assert!(
            self.maintenance_profile.is_none(),
            "equipment definition {} cannot define more than one maintenance profile",
            self.id.value()
        );
        assert!(
            self.assembly_profile.is_none() || profile.is_component_replacement(),
            "equipment definition {} cannot apply aggregate maintenance to exact assembly traces",
            self.id.value()
        );
        assert!(
            profile.restored_condition() > self.maintenance_thresholds.warning_below(),
            "equipment definition {} maintenance service must restore into its normal condition band",
            self.id.value()
        );
        self.maintenance_profile = Some(profile);
        self
    }

    /// Adds the exact conserved commodity from which this equipment may be assembled at runtime.
    #[must_use]
    pub fn with_assembly_profile(mut self, profile: MaterialAssemblyProfile) -> Self {
        assert!(
            self.assembly_profile.is_none(),
            "equipment definition {} cannot define more than one assembly profile",
            self.id.value()
        );
        assert!(
            self.maintenance_profile
                .is_none_or(EquipmentMaintenanceProfile::is_component_replacement),
            "equipment definition {} cannot apply aggregate maintenance to exact assembly traces",
            self.id.value()
        );
        self.assembly_profile = Some(profile);
        self
    }

    /// Adds one additive, material-conserving upgrade route from an existing equipment definition.
    #[must_use]
    pub fn with_upgrade_profile(mut self, profile: EquipmentUpgradeProfile) -> Self {
        assert!(
            self.upgrade_profile.is_none(),
            "equipment definition {} cannot define more than one upgrade profile",
            self.id.value()
        );
        assert_ne!(
            profile.from(),
            self.id,
            "equipment definition {} cannot upgrade from itself",
            self.id.value()
        );
        self.upgrade_profile = Some(profile);
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
    pub const fn requires_structural_support(&self) -> bool {
        self.requires_structural_support
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

    #[must_use]
    pub fn assembly_profile(&self) -> Option<&MaterialAssemblyProfile> {
        self.assembly_profile.as_ref()
    }

    #[must_use]
    pub fn upgrade_profile(&self) -> Option<&EquipmentUpgradeProfile> {
        self.upgrade_profile.as_ref()
    }

    /// Returns whether this definition declares a direct authored equipment-acquisition edge.
    ///
    /// This local classification does not prove a transitive material path, ordinary reachability,
    /// a current world opportunity, or authorization.
    #[must_use]
    pub const fn has_authored_acquisition_edge(&self) -> bool {
        self.assembly_profile.is_some() || self.upgrade_profile.is_some()
    }
}

/// Immutable deterministic authored equipment lookup table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EquipmentRegistry {
    definitions: BTreeMap<EquipmentDefinitionId, EquipmentDefinition>,
}

mod validation;

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

    /// Iterates authored equipment definitions in stable definition-ID order.
    pub fn definitions(&self) -> impl Iterator<Item = &EquipmentDefinition> {
        self.definitions.values()
    }
}
