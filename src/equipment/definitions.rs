//! Immutable maintainable-equipment definitions; sibling state stores only persistent references and changing condition.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::capability::{
    CapabilityId, CapabilityProfile, CapabilityRegistry, CapabilityValue, CapabilityValueKind,
};
use crate::core::quantity::Mass;
use crate::maintenance::{Condition, MaintenanceThresholds};
use crate::material::{
    CommodityKey, FormId, MaterialAssemblyProfile, MaterialPhase, MaterialRegistry,
    ParticleSizeStatePolicy,
};

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
    spent: CommodityKey,
    restored_condition: Condition,
}

impl EquipmentMaintenanceProfile {
    #[must_use]
    pub fn new(
        replacement: CommodityKey,
        replacement_mass: Mass,
        spent: CommodityKey,
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
        assert_eq!(
            replacement.material(),
            spent.material(),
            "equipment maintenance may change form but cannot change material identity"
        );
        assert_ne!(
            replacement, spent,
            "equipment maintenance spent output must differ from reusable replacement stock"
        );
        Self {
            replacement,
            replacement_mass,
            spent,
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
    pub const fn spent(self) -> CommodityKey {
        self.spent
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

    fn assert_monotonic_toward(&self, nominal: CapabilityValue) {
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
    worn_recovery_form: Option<FormId>,
}

/// Authored additive conversion from one existing equipment class into this definition.
///
/// An upgrade owns only the newly added matter. The base instance keeps its identity, condition,
/// creation history, and already-embodied material; runtime validation appends the exact consumed
/// traces and changes only the immutable-definition reference and total embodied mass.
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
            worn_recovery_form: None,
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
            self.assembly_profile.is_none(),
            "equipment definition {} cannot combine exact assembly traces with aggregate maintenance until component replacement is modeled",
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
            self.maintenance_profile.is_none(),
            "equipment definition {} cannot combine exact assembly traces with aggregate maintenance until component replacement is modeled",
            self.id.value()
        );
        self.assembly_profile = Some(profile);
        self
    }

    /// Adds a destructive same-material recovery form for worn assembled equipment.
    #[must_use]
    pub fn with_worn_recovery_form(mut self, form: FormId) -> Self {
        assert!(
            self.worn_recovery_form.is_none(),
            "equipment definition {} cannot define more than one worn-recovery form",
            self.id.value()
        );
        assert!(
            self.assembly_profile.is_some(),
            "equipment definition {} cannot author worn recovery without embodied assembly matter",
            self.id.value()
        );
        self.worn_recovery_form = Some(form);
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

    #[must_use]
    pub const fn worn_recovery_form(&self) -> Option<FormId> {
        self.worn_recovery_form
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

    /// Iterates authored equipment definitions in stable definition-ID order.
    pub fn definitions(&self) -> impl Iterator<Item = &EquipmentDefinition> {
        self.definitions.values()
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
                    definition.assembly_profile().is_none(),
                    "equipment definition {} cannot combine exact assembly traces with aggregate maintenance until component replacement is modeled",
                    definition.id().value()
                );
                for commodity in [maintenance.replacement(), maintenance.spent()] {
                    assert!(
                        materials.has_commodity(commodity),
                        "equipment definition {} maintenance profile references unauthored material {} form {}",
                        definition.id().value(),
                        commodity.material().value(),
                        commodity.form().value()
                    );
                }
                let replacement_form = materials
                    .get_form(maintenance.replacement().form())
                    .unwrap_or_else(|| {
                        unreachable!("validated maintenance replacement commodity has its form")
                    });
                let spent_form = materials
                    .get_form(maintenance.spent().form())
                    .unwrap_or_else(|| {
                        unreachable!("validated maintenance spent commodity has its form")
                    });
                assert_eq!(
                    replacement_form.phase(),
                    spent_form.phase(),
                    "equipment definition {} maintenance cannot change material phase without a thermal process",
                    definition.id().value()
                );
                assert_eq!(
                    replacement_form.particle_size_policy(),
                    spent_form.particle_size_policy(),
                    "equipment definition {} maintenance cannot change particle-size state without a particle-transform process",
                    definition.id().value()
                );
            }
            if let Some(assembly) = definition.assembly_profile() {
                assert_eq!(
                    assembly.input_mass(),
                    definition.mass(),
                    "equipment definition {} assembly mass disagrees with authored equipment mass",
                    definition.id().value()
                );
                assert!(
                    assembly
                        .validate_infrastructure_references(materials)
                        .is_ok(),
                    "equipment definition {} assembly profile must use existing consolidated solid commodities",
                    definition.id().value()
                );
            }
            if let Some(recovery_form) = definition.worn_recovery_form() {
                let assembly = definition.assembly_profile().unwrap_or_else(|| {
                    panic!(
                        "equipment definition {} has worn recovery but no assembly profile",
                        definition.id().value()
                    )
                });
                let form = materials.get_form(recovery_form).unwrap_or_else(|| {
                    panic!(
                        "equipment definition {} references missing worn-recovery form {}",
                        definition.id().value(),
                        recovery_form.value()
                    )
                });
                assert_eq!(
                    form.phase(),
                    MaterialPhase::Solid,
                    "equipment definition {} worn-recovery form {} must be solid",
                    definition.id().value(),
                    recovery_form.value()
                );
                assert_eq!(
                    form.particle_size_policy(),
                    ParticleSizeStatePolicy::Untracked,
                    "equipment definition {} worn-recovery form {} must not require particulate state",
                    definition.id().value(),
                    recovery_form.value()
                );
                assert!(
                    assembly
                        .inputs()
                        .iter()
                        .all(|input| input.commodity().form() != recovery_form),
                    "equipment definition {} worn-recovery form {} cannot also be a direct assembly input",
                    definition.id().value(),
                    recovery_form.value()
                );
                assert!(
                    assembly.inputs().iter().all(|input| {
                        materials.has_commodity(CommodityKey::new(
                            input.commodity().material(),
                            recovery_form,
                        ))
                    }),
                    "equipment definition {} worn-recovery form {} must be authored for every embodied assembly material",
                    definition.id().value(),
                    recovery_form.value()
                );
            }
        }

        for target in self.definitions.values() {
            let Some(upgrade) = target.upgrade_profile() else {
                continue;
            };
            let base = self.definitions.get(&upgrade.from()).unwrap_or_else(|| {
                panic!(
                    "equipment definition {} upgrade references missing base definition {}",
                    target.id().value(),
                    upgrade.from().value()
                )
            });
            assert!(
                upgrade
                    .additions()
                    .validate_infrastructure_references(materials)
                    .is_ok(),
                "equipment definition {} upgrade additions must use existing consolidated solid commodities",
                target.id().value()
            );
            let base_assembly = base.assembly_profile().unwrap_or_else(|| {
                panic!(
                    "equipment definition {} upgrade base {} has no material assembly profile",
                    target.id().value(),
                    base.id().value()
                )
            });
            let target_assembly = target.assembly_profile().unwrap_or_else(|| {
                panic!(
                    "equipment definition {} has an upgrade profile but no material assembly profile",
                    target.id().value()
                )
            });
            let expected_mass = base
                .mass()
                .checked_add(upgrade.additions().input_mass())
                .unwrap_or_else(|| {
                    panic!(
                        "equipment definition {} upgrade mass overflows",
                        target.id().value()
                    )
                });
            assert_eq!(
                target.mass(),
                expected_mass,
                "equipment definition {} upgrade mass must equal base mass plus additive material",
                target.id().value()
            );

            let mut expected_inputs = std::collections::BTreeMap::new();
            for input in base_assembly
                .inputs()
                .iter()
                .chain(upgrade.additions().inputs())
            {
                let previous = expected_inputs
                    .get(&input.commodity())
                    .copied()
                    .unwrap_or(Mass::ZERO);
                let combined = previous.checked_add(input.mass()).unwrap_or_else(|| {
                    panic!(
                        "equipment definition {} upgrade material quantity overflows for commodity {}",
                        target.id().value(),
                        input.commodity().value()
                    )
                });
                expected_inputs.insert(input.commodity(), combined);
            }
            assert_eq!(
                expected_inputs.len(),
                target_assembly.inputs().len(),
                "equipment definition {} upgrade target assembly has extra or missing commodities",
                target.id().value()
            );
            for input in target_assembly.inputs() {
                assert_eq!(
                    expected_inputs.get(&input.commodity()).copied(),
                    Some(input.mass()),
                    "equipment definition {} upgrade target assembly disagrees with base plus additive material for commodity {}",
                    target.id().value(),
                    input.commodity().value()
                );
            }
        }
    }
}
