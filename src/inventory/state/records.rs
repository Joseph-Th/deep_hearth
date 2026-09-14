//! Persistent stockpile records with material-lot record routing.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Deserializer, Serialize};

use crate::core::quantity::{Mass, Temperature};
use crate::core::time::SimulationTick;
use crate::material::{CommodityKey, MaterialPhase};
use crate::structural::StructuralElementId;

use crate::inventory::storage::StorageDefinitionId;

mod material_lot;

pub use material_lot::{
    ConsumedMaterialTrace, MaterialLotId, MaterialLotProfile, MaterialLotProvenance,
    MaterialLotRecord,
};
pub(crate) use material_lot::{PureMaterialTraceValidationError, checked_consumed_material_mass};

/// Persistent identifier for a runtime stockpile record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StockpileId(u32);

impl StockpileId {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        assert!(value != 0, "stockpile id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Physical containment envelope for one stockpile's directly owned material lots.
///
/// This is intentionally explicit runtime state rather than an implicit property of the UI label
/// "stockpile". A dry pile may hold hot or cold solids, while a crucible-like store can explicitly
/// admit liquid matter up to an authored thermal limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct StockpileStorageProfile {
    can_store_solid: bool,
    can_store_liquid: bool,
    maximum_temperature: Temperature,
    preservation_multiplier_ppm: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StockpileStorageProfileRepresentation {
    can_store_solid: bool,
    can_store_liquid: bool,
    maximum_temperature: Temperature,
    preservation_multiplier_ppm: u32,
}

impl<'de> Deserialize<'de> for StockpileStorageProfile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let representation = StockpileStorageProfileRepresentation::deserialize(deserializer)?;
        Self::with_preservation(
            representation.can_store_solid,
            representation.can_store_liquid,
            representation.maximum_temperature,
            representation.preservation_multiplier_ppm,
        )
        .map_err(serde::de::Error::custom)
    }
}

impl StockpileStorageProfile {
    /// Builds a validated material-containment envelope.
    pub fn new(
        can_store_solid: bool,
        can_store_liquid: bool,
        maximum_temperature: Temperature,
    ) -> Result<Self, StockpileStorageProfileError> {
        let profile = Self {
            can_store_solid,
            can_store_liquid,
            maximum_temperature,
            preservation_multiplier_ppm: AMBIENT_PRESERVATION_MULTIPLIER_PPM,
        };
        profile.validate()?;
        Ok(profile)
    }

    /// Builds containment with an explicit multiplier applied to authored perishability lifetimes.
    /// One million ppm is ambient storage; larger values extend shelf life.
    pub fn with_preservation(
        can_store_solid: bool,
        can_store_liquid: bool,
        maximum_temperature: Temperature,
        preservation_multiplier_ppm: u32,
    ) -> Result<Self, StockpileStorageProfileError> {
        let profile = Self {
            can_store_solid,
            can_store_liquid,
            maximum_temperature,
            preservation_multiplier_ppm,
        };
        profile.validate()?;
        Ok(profile)
    }

    /// Unbounded-temperature containment for dry storage that accepts solid matter only.
    #[must_use]
    pub const fn unbounded_solid_only() -> Self {
        Self {
            can_store_solid: true,
            can_store_liquid: false,
            maximum_temperature: Temperature::from_millikelvin(u32::MAX),
            preservation_multiplier_ppm: AMBIENT_PRESERVATION_MULTIPLIER_PPM,
        }
    }

    #[must_use]
    pub const fn can_store_phase(self, phase: MaterialPhase) -> bool {
        match phase {
            MaterialPhase::Solid => self.can_store_solid,
            MaterialPhase::Liquid => self.can_store_liquid,
        }
    }

    #[must_use]
    pub const fn maximum_temperature(self) -> Temperature {
        self.maximum_temperature
    }

    /// Returns the multiplier applied to food or other perishable shelf-life definitions.
    #[must_use]
    pub const fn preservation_multiplier_ppm(self) -> u32 {
        self.preservation_multiplier_ppm
    }

    pub(crate) fn validate(self) -> Result<(), StockpileStorageProfileError> {
        if !self.can_store_solid && !self.can_store_liquid {
            return Err(StockpileStorageProfileError::NoAcceptedPhase);
        }
        if self.maximum_temperature.millikelvin() == 0 {
            return Err(StockpileStorageProfileError::ZeroMaximumTemperature);
        }
        if self.preservation_multiplier_ppm == 0 {
            return Err(StockpileStorageProfileError::ZeroPreservationMultiplier);
        }
        Ok(())
    }
}

pub(crate) const AMBIENT_PRESERVATION_MULTIPLIER_PPM: u32 =
    crate::core::arithmetic::NORMALIZED_PARTS_PER_MILLION;

/// Invalid stockpile containment envelope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StockpileStorageProfileError {
    NoAcceptedPhase,
    ZeroMaximumTemperature,
    ZeroPreservationMultiplier,
}

impl Display for StockpileStorageProfileError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoAcceptedPhase => formatter
                .write_str("stockpile storage profile must accept at least one material phase"),
            Self::ZeroMaximumTemperature => formatter.write_str(
                "stockpile storage profile maximum temperature must be above absolute zero",
            ),
            Self::ZeroPreservationMultiplier => {
                formatter.write_str("stockpile preservation multiplier must be nonzero")
            }
        }
    }
}

impl Error for StockpileStorageProfileError {}

/// One capacity-constrained aggregate store for fungible material mass.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StockpileRecord {
    pub(in crate::inventory) id: StockpileId,
    pub(in crate::inventory) capacity: Mass,
    pub(in crate::inventory) storage_profile: StockpileStorageProfile,
    pub(in crate::inventory) enclosure: Option<StockpileEnclosureRecord>,
    pub(in crate::inventory) supported_by: Option<StructuralElementId>,
    pub(in crate::inventory) stored_mass: Mass,
    pub(in crate::inventory) reserved_inbound: Mass,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    pub(in crate::inventory) contents: BTreeMap<CommodityKey, Mass>,
}

/// Checked stockpile-capacity projection around one atomic outgoing/incoming exchange.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::inventory) struct StockpileMassProjection {
    pub(in crate::inventory) committed_before_incoming: Mass,
    pub(in crate::inventory) after_incoming: Mass,
}

/// Exact physical enclosure currently embodied around one stockpile.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StockpileEnclosureRecord {
    definition: StorageDefinitionId,
    embodied_material: Vec<ConsumedMaterialTrace>,
    created_at: SimulationTick,
}

impl StockpileEnclosureRecord {
    #[must_use]
    pub(crate) fn new(
        definition: StorageDefinitionId,
        embodied_material: Vec<ConsumedMaterialTrace>,
        created_at: SimulationTick,
    ) -> Self {
        Self {
            definition,
            embodied_material,
            created_at,
        }
    }

    #[must_use]
    pub const fn definition(&self) -> StorageDefinitionId {
        self.definition
    }

    #[must_use]
    pub fn embodied_mass(&self) -> Mass {
        checked_consumed_material_mass(&self.embodied_material).unwrap_or_else(|| {
            panic!(
                "validated storage enclosure {} embodied trace mass overflowed",
                self.definition.value()
            )
        })
    }

    #[must_use]
    pub fn embodied_material(&self) -> &[ConsumedMaterialTrace] {
        &self.embodied_material
    }

    #[must_use]
    pub const fn created_at(&self) -> SimulationTick {
        self.created_at
    }
}

impl StockpileRecord {
    #[must_use]
    pub const fn id(&self) -> StockpileId {
        self.id
    }

    #[must_use]
    pub const fn capacity(&self) -> Mass {
        self.capacity
    }

    #[must_use]
    pub const fn storage_profile(&self) -> StockpileStorageProfile {
        self.storage_profile
    }

    /// Returns the material-backed storage enclosure, if this stockpile has been improved.
    #[must_use]
    pub const fn enclosure(&self) -> Option<&StockpileEnclosureRecord> {
        self.enclosure.as_ref()
    }

    /// Returns matter embodied in this stockpile's enclosure rather than stored as contents.
    #[must_use]
    pub fn embodied_mass(&self) -> Mass {
        match &self.enclosure {
            Some(enclosure) => enclosure.embodied_mass(),
            None => Mass::ZERO,
        }
    }

    /// Returns the structural member currently carrying this stockpile's stored matter, if assigned.
    #[must_use]
    pub const fn supported_by(&self) -> Option<StructuralElementId> {
        self.supported_by
    }

    #[must_use]
    pub const fn stored_mass(&self) -> Mass {
        self.stored_mass
    }

    #[must_use]
    pub const fn reserved_inbound(&self) -> Mass {
        self.reserved_inbound
    }

    /// Returns capacity not already occupied by stored matter or committed inbound reservations.
    ///
    /// This is an observation, not an authorization. Callers must still use the canonical
    /// reservation or ingress operation because another transition can consume this capacity.
    #[must_use]
    pub fn available_capacity(&self) -> Mass {
        let committed = self
            .stored_mass
            .checked_add(self.reserved_inbound)
            .unwrap_or_else(|| {
                panic!(
                    "validated stockpile {} committed mass overflowed",
                    self.id.value()
                )
            });
        self.capacity.checked_sub(committed).unwrap_or_else(|| {
            panic!(
                "validated stockpile {} committed mass exceeds capacity",
                self.id.value()
            )
        })
    }

    /// Projects committed capacity after one outgoing amount and one new incoming amount.
    ///
    /// Existing inbound reservations remain committed throughout the exchange. Callers pass zero
    /// outgoing mass when the withdrawal occurs from another stockpile.
    pub(in crate::inventory) fn project_mass_exchange(
        &self,
        outgoing: Mass,
        incoming: Mass,
    ) -> Option<StockpileMassProjection> {
        let stored_after_outgoing = self.stored_mass.checked_sub(outgoing)?;
        let committed_before_incoming = stored_after_outgoing.checked_add(self.reserved_inbound)?;
        let after_incoming = committed_before_incoming.checked_add(incoming)?;
        Some(StockpileMassProjection {
            committed_before_incoming,
            after_incoming,
        })
    }

    /// Returns currently stored mass for one exact material/form key.
    #[must_use]
    pub fn get_mass(&self, commodity: CommodityKey) -> Mass {
        self.contents.get(&commodity).copied().unwrap_or(Mass::ZERO)
    }

    /// Iterates stock deterministically in material/form key order.
    pub fn contents(&self) -> impl Iterator<Item = (CommodityKey, Mass)> + '_ {
        self.contents.iter().map(|(key, mass)| (*key, *mass))
    }
}

#[cfg(test)]
#[path = "records_tests.rs"]
mod tests;
