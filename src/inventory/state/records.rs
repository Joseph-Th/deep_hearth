//! Persistent inventory records and storage-history value semantics.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::core::quantity::{Mass, Temperature};
use crate::core::time::SimulationTick;
use crate::material::{
    CommodityKey, MaterialComposition, MaterialPhase, ParticleSizeDistribution, ParticleSizeRange,
};
use crate::structural::StructuralElementId;

use crate::inventory::storage::StorageDefinitionId;

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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StockpileStorageProfile {
    can_store_solid: bool,
    can_store_liquid: bool,
    maximum_temperature: Temperature,
    preservation_multiplier_ppm: u32,
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

pub(crate) const AMBIENT_PRESERVATION_MULTIPLIER_PPM: u32 = 1_000_000;

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

pub(crate) const STORAGE_AGE_PARTS_PER_TICK: u128 = 1_000_000;

/// Ambient-equivalent storage age retained across stockpile moves.
///
/// `ambient_age_parts` records exposure accumulated before `last_transition_at`; the current
/// stockpile's preservation multiplier determines the rate after that tick. One ambient tick equals
/// `STORAGE_AGE_PARTS_PER_TICK` parts. This keeps preservation history independent from any one food
/// definition while preventing later movement into better storage from retroactively improving prior
/// exposure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MaterialStorageHistory {
    ambient_age_parts: u128,
    last_transition_at: SimulationTick,
}

impl MaterialStorageHistory {
    #[must_use]
    pub(crate) const fn new(at: SimulationTick) -> Self {
        Self {
            ambient_age_parts: 0,
            last_transition_at: at,
        }
    }

    #[must_use]
    pub(crate) const fn last_transition_at(self) -> SimulationTick {
        self.last_transition_at
    }

    pub(crate) fn project(
        self,
        at: SimulationTick,
        preservation_multiplier_ppm: u32,
    ) -> Option<u128> {
        let elapsed = at.value().checked_sub(self.last_transition_at.value())?;
        let numerator =
            u128::from(elapsed) * STORAGE_AGE_PARTS_PER_TICK * STORAGE_AGE_PARTS_PER_TICK;
        let increment = numerator.div_ceil(u128::from(preservation_multiplier_ppm));
        self.ambient_age_parts.checked_add(increment)
    }

    pub(crate) fn rebase(
        self,
        at: SimulationTick,
        preservation_multiplier_ppm: u32,
    ) -> Option<Self> {
        Some(Self {
            ambient_age_parts: self.project(at, preservation_multiplier_ppm)?,
            last_transition_at: at,
        })
    }

    #[must_use]
    pub(crate) const fn with_ambient_age_parts(
        ambient_age_parts: u128,
        at: SimulationTick,
    ) -> Self {
        Self {
            ambient_age_parts,
            last_transition_at: at,
        }
    }
}

/// Physical/provenance snapshot of one material slice consumed by an in-flight operation.
///
/// Source lot identity is omitted because a fully consumed lot may cease to exist. The trace records
/// physical and provenance facts only; it is neither an ownership reference nor a second matter owner.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsumedMaterialTrace {
    pub(in crate::inventory) mass: Mass,
    pub(in crate::inventory) profile: MaterialLotProfile,
    pub(in crate::inventory) provenance: MaterialLotProvenance,
}

impl ConsumedMaterialTrace {
    #[must_use]
    pub const fn mass(&self) -> Mass {
        self.mass
    }

    #[must_use]
    pub const fn profile(&self) -> &MaterialLotProfile {
        &self.profile
    }

    #[must_use]
    pub const fn provenance(&self) -> MaterialLotProvenance {
        self.provenance
    }
}

/// Sums exact material traces without widening or wrapping authoritative mass.
pub(crate) fn checked_consumed_material_mass(traces: &[ConsumedMaterialTrace]) -> Option<Mass> {
    traces
        .iter()
        .try_fold(Mass::ZERO, |total, trace| total.checked_add(trace.mass()))
}

/// Persistent identifier for one homogeneous runtime material lot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MaterialLotId(u64);

impl MaterialLotId {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        assert!(value != 0, "material lot id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// Runtime properties that determine whether two collocated lots are physically fungible.
///
/// Physical properties that determine process interchangeability belong here. Storage age and provenance
/// stay outside this profile. Age-sensitive commodities only coalesce when projected storage exposure is
/// identical, preserving exact perishability cohorts instead of aging newer matter to match older
/// matter. Commodities without authored age-dependent behavior may coalesce conservatively across
/// exposure histories to keep lot fragmentation bounded.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialLotProfile {
    pub(in crate::inventory) commodity: CommodityKey,
    pub(in crate::inventory) temperature: Temperature,
    pub(in crate::inventory) composition: MaterialComposition,
    pub(in crate::inventory) particle_size: Option<ParticleSizeDistribution>,
}

impl MaterialLotProfile {
    #[must_use]
    pub const fn commodity(&self) -> CommodityKey {
        self.commodity
    }

    #[must_use]
    pub const fn temperature(&self) -> Temperature {
        self.temperature
    }

    #[must_use]
    pub const fn composition(&self) -> &MaterialComposition {
        &self.composition
    }

    #[must_use]
    pub fn particle_size(&self) -> Option<ParticleSizeRange> {
        self.particle_size
            .as_ref()
            .map(ParticleSizeDistribution::envelope)
    }

    /// Returns the authoritative weighted particulate profile, if present.
    #[must_use]
    pub const fn particle_size_distribution(&self) -> Option<&ParticleSizeDistribution> {
        self.particle_size.as_ref()
    }
}

/// Provenance range retained when compatible matter coalesces into an existing lot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialLotProvenance {
    pub(in crate::inventory) earliest_created_at: SimulationTick,
    pub(in crate::inventory) latest_created_at: SimulationTick,
}

impl MaterialLotProvenance {
    #[must_use]
    pub const fn earliest_created_at(self) -> SimulationTick {
        self.earliest_created_at
    }

    #[must_use]
    pub const fn latest_created_at(self) -> SimulationTick {
        self.latest_created_at
    }
}

/// One homogeneous batch of matter whose local runtime properties must remain distinguishable.
///
/// Lots are the authoritative source for matter identity, mass, thermal state, and ownership.
/// Stockpile commodity totals and runtime lot-routing indexes are derived state maintained
/// atomically by the inventory owner.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialLotRecord {
    pub(in crate::inventory) id: MaterialLotId,
    pub(in crate::inventory) stockpile: StockpileId,
    pub(in crate::inventory) mass: Mass,
    pub(in crate::inventory) profile: MaterialLotProfile,
    pub(in crate::inventory) provenance: MaterialLotProvenance,
    pub(in crate::inventory) storage_history: MaterialStorageHistory,
}

impl MaterialLotRecord {
    #[must_use]
    pub const fn id(&self) -> MaterialLotId {
        self.id
    }

    #[must_use]
    pub const fn stockpile(&self) -> StockpileId {
        self.stockpile
    }

    #[must_use]
    pub const fn commodity(&self) -> CommodityKey {
        self.profile.commodity
    }

    #[must_use]
    pub const fn mass(&self) -> Mass {
        self.mass
    }

    #[must_use]
    pub const fn temperature(&self) -> Temperature {
        self.profile.temperature
    }

    #[must_use]
    pub const fn composition(&self) -> &MaterialComposition {
        &self.profile.composition
    }

    #[must_use]
    pub fn particle_size(&self) -> Option<ParticleSizeRange> {
        self.profile.particle_size()
    }

    /// Returns the authoritative weighted particulate profile, if present.
    #[must_use]
    pub const fn particle_size_distribution(&self) -> Option<&ParticleSizeDistribution> {
        self.profile.particle_size_distribution()
    }

    #[must_use]
    pub const fn created_at(&self) -> SimulationTick {
        self.provenance.earliest_created_at
    }

    /// Returns the latest creation tick represented after compatible matter was coalesced.
    #[must_use]
    pub const fn latest_created_at(&self) -> SimulationTick {
        self.provenance.latest_created_at
    }

    pub(crate) const fn storage_history(&self) -> MaterialStorageHistory {
        self.storage_history
    }
}

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
