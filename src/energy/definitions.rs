//! Immutable definitions for finite energy stores; runtime state owns only changing stored energy.

use serde::{Deserialize, Serialize};

use crate::core::quantity::{Energy, Power};
use crate::material::MaterialAssemblyProfile;

mod registry;

pub use registry::EnergyRegistry;

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

    /// Returns whether this store declares a direct authored assembly edge.
    ///
    /// This local classification does not prove a transitive material path, ordinary reachability,
    /// a current world opportunity, or authorization.
    #[must_use]
    pub const fn has_authored_assembly_edge(&self) -> bool {
        self.assembly_profile.is_some()
    }
}

#[cfg(test)]
#[path = "definitions_tests.rs"]
mod tests;
