//! Immutable player-labor definitions; lifecycle and power execution own runtime admission and mutation.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::capability::{CapabilityId, CapabilityRegistry, CapabilityValueKind};
use crate::energy::EnergyCarrier;
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

/// Immutable deterministic lookup for direct player-power semantics.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LaborRegistry {
    manual_power: BTreeMap<ManualPowerMethodId, ManualPowerDefinition>,
}

impl LaborRegistry {
    pub(crate) fn new(definitions: impl IntoIterator<Item = ManualPowerDefinition>) -> Self {
        let mut manual_power = BTreeMap::new();
        for definition in definitions {
            let id = definition.id();
            assert!(
                manual_power.insert(id, definition).is_none(),
                "duplicate manual power method {}",
                id.value()
            );
        }
        Self { manual_power }
    }

    #[must_use]
    pub fn get_manual_power(&self, id: ManualPowerMethodId) -> Option<&ManualPowerDefinition> {
        self.manual_power.get(&id)
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
