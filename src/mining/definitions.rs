//! Immutable physical mining-method definitions.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::capability::{CapabilityId, CapabilityRegistry, CapabilityValueKind};
use crate::maintenance::assert_valid_condition_wear_ppm_per_tick;
use crate::survival::SurvivalExertion;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MiningMethodId(u32);

impl MiningMethodId {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        assert!(value != 0, "mining method id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MiningMethodDefinition {
    id: MiningMethodId,
    name: String,
    mass_flow_capability: CapabilityId,
    max_batch_mass_capability: CapabilityId,
    max_hardness_capability: CapabilityId,
    condition_wear_ppm_per_active_tick: u32,
    exertion: SurvivalExertion,
}

impl MiningMethodDefinition {
    #[must_use]
    pub fn new(
        id: MiningMethodId,
        name: impl Into<String>,
        mass_flow_capability: CapabilityId,
        max_batch_mass_capability: CapabilityId,
        max_hardness_capability: CapabilityId,
        condition_wear_ppm_per_active_tick: u32,
        exertion: SurvivalExertion,
    ) -> Self {
        let name = name.into();
        assert!(
            !name.trim().is_empty(),
            "mining method name must not be empty"
        );
        assert_valid_condition_wear_ppm_per_tick(condition_wear_ppm_per_active_tick);
        exertion.assert_active_player_work();
        Self {
            id,
            name,
            mass_flow_capability,
            max_batch_mass_capability,
            max_hardness_capability,
            condition_wear_ppm_per_active_tick,
            exertion,
        }
    }
    #[must_use]
    pub const fn id(&self) -> MiningMethodId {
        self.id
    }
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    #[must_use]
    pub const fn mass_flow_capability(&self) -> CapabilityId {
        self.mass_flow_capability
    }
    #[must_use]
    pub const fn max_batch_mass_capability(&self) -> CapabilityId {
        self.max_batch_mass_capability
    }
    #[must_use]
    pub const fn max_hardness_capability(&self) -> CapabilityId {
        self.max_hardness_capability
    }
    #[must_use]
    pub const fn condition_wear_ppm_per_active_tick(&self) -> u32 {
        self.condition_wear_ppm_per_active_tick
    }
    #[must_use]
    pub const fn exertion(&self) -> SurvivalExertion {
        self.exertion
    }
}

#[cfg(test)]
#[path = "definitions_tests.rs"]
mod tests;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MiningRegistry {
    methods: BTreeMap<MiningMethodId, MiningMethodDefinition>,
}

impl MiningRegistry {
    pub(crate) fn new(definitions: impl IntoIterator<Item = MiningMethodDefinition>) -> Self {
        let mut methods = BTreeMap::new();
        for definition in definitions {
            let id = definition.id();
            assert!(
                methods.insert(id, definition).is_none(),
                "duplicate mining method {}",
                id.value()
            );
        }
        Self { methods }
    }
    #[must_use]
    pub fn get_method(&self, id: MiningMethodId) -> Option<&MiningMethodDefinition> {
        self.methods.get(&id)
    }
    pub fn definitions(&self) -> impl Iterator<Item = &MiningMethodDefinition> {
        self.methods.values()
    }
    pub(crate) fn validate_references(&self, capabilities: &CapabilityRegistry) {
        for method in self.methods.values() {
            for (capability, kind) in [
                (method.mass_flow_capability(), CapabilityValueKind::MassFlow),
                (
                    method.max_batch_mass_capability(),
                    CapabilityValueKind::Mass,
                ),
                (
                    method.max_hardness_capability(),
                    CapabilityValueKind::Pressure,
                ),
            ] {
                let definition = capabilities.get_capability(capability).unwrap_or_else(|| {
                    panic!(
                        "mining method {} references missing capability {}",
                        method.id().value(),
                        capability.value()
                    )
                });
                assert_eq!(
                    definition.kind(),
                    kind,
                    "mining method {} capability {} has wrong physical kind",
                    method.id().value(),
                    capability.value()
                );
            }
        }
    }
}
