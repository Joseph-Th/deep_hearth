//! Immutable definitions for finite energy stores; runtime state owns only changing stored energy.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::core::quantity::{Energy, Power};
use crate::core::time::{PhysicalTickDuration, TickSpan};
use crate::material::{MaterialAssemblyProfile, MaterialRegistry};

use super::integration::{PowerRemainder, integrate_power};

/// Stable authored identity for one energy-store class.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EnergyStoreDefinitionId(u32);

impl EnergyStoreDefinitionId {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        assert!(value != 0, "energy store definition id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Explicit carrier represented by a finite energy store.
///
/// Chemical energy is intentionally absent: fuels remain conserved material and must be resolved
/// through combustion/chemistry rather than becoming an abstract energy balance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EnergyCarrier {
    Electrical,
    Thermal,
    Mechanical,
}

/// Exact additive matter required to convert one energy-store definition into another while
/// preserving runtime identity and existing embodied material.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnergyStoreUpgradeProfile {
    from: EnergyStoreDefinitionId,
    additions: MaterialAssemblyProfile,
}

impl EnergyStoreUpgradeProfile {
    #[must_use]
    pub fn new(from: EnergyStoreDefinitionId, additions: MaterialAssemblyProfile) -> Self {
        Self { from, additions }
    }

    #[must_use]
    pub const fn from(&self) -> EnergyStoreDefinitionId {
        self.from
    }

    #[must_use]
    pub const fn additions(&self) -> &MaterialAssemblyProfile {
        &self.additions
    }
}

/// Immutable authored capacity and directional transfer envelopes for one energy-store class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnergyStoreDefinition {
    id: EnergyStoreDefinitionId,
    name: String,
    carrier: EnergyCarrier,
    capacity: Energy,
    max_input_power: Power,
    max_output_power: Power,
    passive_dissipation_power: Power,
    assembly_profile: Option<MaterialAssemblyProfile>,
    upgrade_profile: Option<EnergyStoreUpgradeProfile>,
}

impl EnergyStoreDefinition {
    /// Builds a finite energy store with explicit independent input and output power envelopes.
    ///
    /// Either direction may be zero, allowing a pure source or pure sink. A store with no transfer
    /// direction at all would be inert runtime state and is rejected.
    #[must_use]
    pub fn new_with_transfer_limits(
        id: EnergyStoreDefinitionId,
        name: impl Into<String>,
        carrier: EnergyCarrier,
        capacity: Energy,
        max_input_power: Power,
        max_output_power: Power,
    ) -> Self {
        let name = name.into();
        assert!(
            !name.trim().is_empty(),
            "energy store name must not be empty"
        );
        assert!(!capacity.is_zero(), "energy store capacity must be nonzero");
        assert!(
            !max_input_power.is_zero() || !max_output_power.is_zero(),
            "energy store must accept input, provide output, or both"
        );
        Self {
            id,
            name,
            carrier,
            capacity,
            max_input_power,
            max_output_power,
            passive_dissipation_power: Power::ZERO,
            assembly_profile: None,
            upgrade_profile: None,
        }
    }

    /// Adds an unavoidable loss rate from explicit storage into unmodeled environmental or loss
    /// domains. Passive dissipation is not controllable output power and does not make this store
    /// eligible as an operation energy supply.
    #[must_use]
    pub fn with_passive_dissipation_power(mut self, power: Power) -> Self {
        assert!(
            !power.is_zero(),
            "passive energy dissipation power must be nonzero when declared"
        );
        assert!(
            self.passive_dissipation_power.is_zero(),
            "energy store definition {} cannot define passive dissipation more than once",
            self.id.value()
        );
        self.passive_dissipation_power = power;
        self
    }

    /// Adds the exact conserved matter required to construct this store in gameplay.
    #[must_use]
    pub fn with_assembly_profile(mut self, profile: MaterialAssemblyProfile) -> Self {
        assert!(
            self.assembly_profile.is_none(),
            "energy store definition {} cannot define more than one assembly profile",
            self.id.value()
        );
        self.assembly_profile = Some(profile);
        self
    }

    /// Adds one additive, material-conserving upgrade route from an existing store definition.
    #[must_use]
    pub fn with_upgrade_profile(mut self, profile: EnergyStoreUpgradeProfile) -> Self {
        assert!(
            self.upgrade_profile.is_none(),
            "energy store definition {} cannot define more than one upgrade profile",
            self.id.value()
        );
        assert_ne!(
            profile.from(),
            self.id,
            "energy store definition {} cannot upgrade from itself",
            self.id.value()
        );
        self.upgrade_profile = Some(profile);
        self
    }

    #[must_use]
    pub const fn id(&self) -> EnergyStoreDefinitionId {
        self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn carrier(&self) -> EnergyCarrier {
        self.carrier
    }

    #[must_use]
    pub const fn capacity(&self) -> Energy {
        self.capacity
    }

    #[must_use]
    pub const fn max_input_power(&self) -> Power {
        self.max_input_power
    }

    #[must_use]
    pub const fn max_output_power(&self) -> Power {
        self.max_output_power
    }

    /// Returns the unavoidable environmental/loss power removed from stored energy each tick.
    #[must_use]
    pub const fn passive_dissipation_power(&self) -> Power {
        self.passive_dissipation_power
    }

    #[must_use]
    pub fn assembly_profile(&self) -> Option<&MaterialAssemblyProfile> {
        self.assembly_profile.as_ref()
    }

    #[must_use]
    pub fn upgrade_profile(&self) -> Option<&EnergyStoreUpgradeProfile> {
        self.upgrade_profile.as_ref()
    }

    /// Returns whether ordinary gameplay declares a construction route for this store definition.
    /// Discovery/reporting code consumes this owner classification instead of inferring it from
    /// implementation fields.
    #[must_use]
    pub const fn has_runtime_assembly_route(&self) -> bool {
        self.assembly_profile.is_some()
    }
}

#[cfg(test)]
#[path = "definitions_tests.rs"]
mod tests;

/// Immutable deterministic authored lookup table for finite energy stores.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnergyRegistry {
    definitions: BTreeMap<EnergyStoreDefinitionId, EnergyStoreDefinition>,
}

impl EnergyRegistry {
    pub(crate) fn new(definitions: impl IntoIterator<Item = EnergyStoreDefinition>) -> Self {
        let mut by_id = BTreeMap::new();
        for definition in definitions {
            let id = definition.id();
            assert!(
                by_id.insert(id, definition).is_none(),
                "duplicate energy store definition id {}",
                id.value()
            );
        }
        Self { definitions: by_id }
    }

    #[must_use]
    pub fn get_store(&self, id: EnergyStoreDefinitionId) -> Option<&EnergyStoreDefinition> {
        self.definitions.get(&id)
    }

    /// Iterates authored storage definitions in stable ID order.
    pub fn definitions(&self) -> impl Iterator<Item = &EnergyStoreDefinition> {
        self.definitions.values()
    }

    pub(crate) fn validate_references(
        &self,
        materials: &MaterialRegistry,
        physical_tick_duration: PhysicalTickDuration,
    ) {
        for definition in self.definitions.values() {
            if let Some(assembly) = definition.assembly_profile() {
                assert!(
                    assembly
                        .validate_infrastructure_references(materials)
                        .is_ok(),
                    "energy store definition {} assembly profile must use existing consolidated solid commodities",
                    definition.id().value()
                );
            }
            let dissipation_power = definition.passive_dissipation_power();
            if dissipation_power.is_zero() {
                continue;
            }
            let integration = integrate_power(
                dissipation_power,
                TickSpan::new(1),
                physical_tick_duration,
                PowerRemainder::ZERO,
            )
            .unwrap_or_else(|error| {
                panic!(
                    "energy store definition {} passive dissipation cannot be integrated for one authoritative tick: {error}",
                    definition.id().value()
                )
            });
            assert_eq!(
                integration.remainder(),
                PowerRemainder::ZERO,
                "energy store definition {} passive dissipation must resolve to exact whole nanojoules per authoritative tick",
                definition.id().value()
            );
        }
        self.validate_upgrade_ancestry();
        for target in self.definitions.values() {
            let Some(upgrade) = target.upgrade_profile() else {
                continue;
            };
            let base = self.definitions.get(&upgrade.from()).unwrap_or_else(|| {
                panic!(
                    "energy store definition {} upgrade references missing base definition {}",
                    target.id().value(),
                    upgrade.from().value()
                )
            });
            assert!(
                upgrade
                    .additions()
                    .validate_infrastructure_references(materials)
                    .is_ok(),
                "energy store definition {} upgrade additions must use existing consolidated solid commodities",
                target.id().value()
            );
            assert_eq!(
                target.carrier(),
                base.carrier(),
                "energy store definition {} additive upgrade cannot change energy carrier",
                target.id().value()
            );
            assert!(
                target.capacity() >= base.capacity(),
                "energy store definition {} additive upgrade cannot reduce capacity",
                target.id().value()
            );
            assert!(
                target.max_input_power() >= base.max_input_power(),
                "energy store definition {} additive upgrade cannot reduce input power",
                target.id().value()
            );
            assert!(
                target.max_output_power() >= base.max_output_power(),
                "energy store definition {} additive upgrade cannot reduce output power",
                target.id().value()
            );
            assert!(
                target.passive_dissipation_power() <= base.passive_dissipation_power(),
                "energy store definition {} additive upgrade cannot increase passive loss",
                target.id().value()
            );
            let base_assembly = base.assembly_profile().unwrap_or_else(|| {
                panic!(
                    "energy store definition {} upgrade base {} has no material assembly profile",
                    target.id().value(),
                    base.id().value()
                )
            });
            let target_assembly = target.assembly_profile().unwrap_or_else(|| {
                panic!(
                    "energy store definition {} has an upgrade profile but no material assembly profile",
                    target.id().value()
                )
            });
            let mut expected_inputs = BTreeMap::new();
            for input in base_assembly
                .inputs()
                .iter()
                .chain(upgrade.additions().inputs())
            {
                let previous = expected_inputs
                    .get(&input.commodity())
                    .copied()
                    .unwrap_or(crate::core::quantity::Mass::ZERO);
                let combined = previous.checked_add(input.mass()).unwrap_or_else(|| {
                    panic!(
                        "energy store definition {} upgrade material quantity overflows for commodity {}",
                        target.id().value(),
                        input.commodity().value()
                    )
                });
                expected_inputs.insert(input.commodity(), combined);
            }
            assert_eq!(
                expected_inputs.len(),
                target_assembly.inputs().len(),
                "energy store definition {} upgrade target assembly has extra or missing commodities",
                target.id().value()
            );
            for input in target_assembly.inputs() {
                assert_eq!(
                    expected_inputs.get(&input.commodity()).copied(),
                    Some(input.mass()),
                    "energy store definition {} upgrade target assembly disagrees with base plus additive material for commodity {}",
                    target.id().value(),
                    input.commodity().value()
                );
            }
        }
    }

    fn validate_upgrade_ancestry(&self) {
        for definition in self.definitions.values() {
            let mut visited = BTreeSet::new();
            let mut current = definition;
            loop {
                assert!(
                    visited.insert(current.id()),
                    "energy store upgrade ancestry contains a cycle at definition {}",
                    current.id().value()
                );
                let Some(upgrade) = current.upgrade_profile() else {
                    break;
                };
                current = self.definitions.get(&upgrade.from()).unwrap_or_else(|| {
                    panic!(
                        "energy store definition {} upgrade references missing base definition {}",
                        current.id().value(),
                        upgrade.from().value()
                    )
                });
            }
        }
    }
}
