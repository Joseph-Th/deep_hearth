//! Immutable player-labor definitions; subsystem execution owns runtime admission and mutation.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::capability::{CapabilityId, CapabilityRegistry, CapabilityValueKind};
use crate::core::time::TickSpan;
use crate::energy::EnergyCarrier;
use crate::geology::GeologicalEvidenceKind;
use crate::maintenance::assert_valid_condition_wear_ppm_per_tick;
use crate::survival::SurvivalExertion;

/// Stable authored identity for one direct player-powered mechanical method.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ManualPowerMethodId(u32);

impl ManualPowerMethodId {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        assert!(value != 0, "manual power method id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Stable authored identity for one direct geological observation method.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ProspectingMethodId(u32);

impl ProspectingMethodId {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        assert!(value != 0, "prospecting method id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Authored rule converting direct player labor through equipment into finite stored energy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManualPowerDefinition {
    id: ManualPowerMethodId,
    power_capability: CapabilityId,
    carrier: EnergyCarrier,
    metabolic_efficiency_ppm: u32,
    condition_wear_ppm_per_active_tick: u32,
    maximum_exertion: SurvivalExertion,
}

/// Authored rule for one bounded player-performed geological observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProspectingDefinition {
    id: ProspectingMethodId,
    evidence: GeologicalEvidenceKind,
    duration: TickSpan,
    maximum_region_voxels: u128,
    abundance_uncertainty_ppm: u32,
    exertion: SurvivalExertion,
}

impl ProspectingDefinition {
    #[must_use]
    pub fn new(
        id: ProspectingMethodId,
        evidence: GeologicalEvidenceKind,
        duration: TickSpan,
        maximum_region_voxels: u128,
        abundance_uncertainty_ppm: u32,
        exertion: SurvivalExertion,
    ) -> Self {
        assert!(
            duration.value() != 0,
            "prospecting duration must be nonzero"
        );
        assert!(
            maximum_region_voxels != 0,
            "prospecting maximum region must contain at least one voxel"
        );
        assert!(
            abundance_uncertainty_ppm <= 1_000_000,
            "prospecting abundance uncertainty must not exceed one million ppm"
        );
        assert!(
            !exertion.energy_cost_per_tick().is_zero(),
            "prospecting exertion must consume metabolic energy"
        );
        Self {
            id,
            evidence,
            duration,
            maximum_region_voxels,
            abundance_uncertainty_ppm,
            exertion,
        }
    }

    #[must_use]
    pub const fn id(self) -> ProspectingMethodId {
        self.id
    }

    #[must_use]
    pub const fn evidence(self) -> GeologicalEvidenceKind {
        self.evidence
    }

    #[must_use]
    pub const fn duration(self) -> TickSpan {
        self.duration
    }

    #[must_use]
    pub const fn maximum_region_voxels(self) -> u128 {
        self.maximum_region_voxels
    }

    #[must_use]
    pub const fn abundance_uncertainty_ppm(self) -> u32 {
        self.abundance_uncertainty_ppm
    }

    #[must_use]
    pub const fn exertion(self) -> SurvivalExertion {
        self.exertion
    }
}

impl ManualPowerDefinition {
    #[must_use]
    pub fn new(
        id: ManualPowerMethodId,
        power_capability: CapabilityId,
        carrier: EnergyCarrier,
        metabolic_efficiency_ppm: u32,
        condition_wear_ppm_per_active_tick: u32,
        maximum_exertion: SurvivalExertion,
    ) -> Self {
        assert!(
            (1..=1_000_000).contains(&metabolic_efficiency_ppm),
            "manual power metabolic efficiency must be inside 1..=1,000,000 ppm"
        );
        assert_valid_condition_wear_ppm_per_tick(condition_wear_ppm_per_active_tick);
        assert!(
            !maximum_exertion.energy_cost_per_tick().is_zero(),
            "manual power exertion must consume metabolic energy"
        );
        Self {
            id,
            power_capability,
            carrier,
            metabolic_efficiency_ppm,
            condition_wear_ppm_per_active_tick,
            maximum_exertion,
        }
    }

    #[must_use]
    pub const fn id(self) -> ManualPowerMethodId {
        self.id
    }

    #[must_use]
    pub const fn power_capability(self) -> CapabilityId {
        self.power_capability
    }

    #[must_use]
    pub const fn carrier(self) -> EnergyCarrier {
        self.carrier
    }

    #[must_use]
    pub const fn metabolic_efficiency_ppm(self) -> u32 {
        self.metabolic_efficiency_ppm
    }

    #[must_use]
    pub const fn condition_wear_ppm_per_active_tick(self) -> u32 {
        self.condition_wear_ppm_per_active_tick
    }

    #[must_use]
    /// Maximum sustainable physiological effort for this method.
    ///
    /// Runtime manual-power work scales this ceiling to the actual mechanical work required after
    /// equipment and destination power bottlenecks are known.
    pub const fn maximum_exertion(self) -> SurvivalExertion {
        self.maximum_exertion
    }
}

/// Immutable deterministic lookup for authored player-labor method semantics.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LaborRegistry {
    manual_power: BTreeMap<ManualPowerMethodId, ManualPowerDefinition>,
    prospecting: BTreeMap<ProspectingMethodId, ProspectingDefinition>,
}

impl LaborRegistry {
    pub(crate) fn new(
        manual_power_definitions: impl IntoIterator<Item = ManualPowerDefinition>,
        prospecting_definitions: impl IntoIterator<Item = ProspectingDefinition>,
    ) -> Self {
        let mut manual_power = BTreeMap::new();
        for definition in manual_power_definitions {
            let id = definition.id();
            assert!(
                manual_power.insert(id, definition).is_none(),
                "duplicate manual power method {}",
                id.value()
            );
        }
        let mut prospecting = BTreeMap::new();
        for definition in prospecting_definitions {
            let id = definition.id();
            assert!(
                prospecting.insert(id, definition).is_none(),
                "duplicate prospecting method {}",
                id.value()
            );
        }
        Self {
            manual_power,
            prospecting,
        }
    }

    #[must_use]
    pub fn get_manual_power(&self, id: ManualPowerMethodId) -> Option<&ManualPowerDefinition> {
        self.manual_power.get(&id)
    }

    #[must_use]
    pub fn get_prospecting(&self, id: ProspectingMethodId) -> Option<&ProspectingDefinition> {
        self.prospecting.get(&id)
    }

    pub fn prospecting_definitions(&self) -> impl Iterator<Item = &ProspectingDefinition> {
        self.prospecting.values()
    }

    pub(crate) fn validate_references(&self, capabilities: &CapabilityRegistry) {
        for definition in self.manual_power.values() {
            let capability = capabilities
                .get_capability(definition.power_capability())
                .unwrap_or_else(|| {
                    panic!(
                        "manual power method {} references missing capability {}",
                        definition.id().value(),
                        definition.power_capability().value()
                    )
                });
            assert_eq!(
                capability.kind(),
                CapabilityValueKind::Power,
                "manual power method {} capability {} must have Power value kind",
                definition.id().value(),
                definition.power_capability().value()
            );
        }
    }
}
