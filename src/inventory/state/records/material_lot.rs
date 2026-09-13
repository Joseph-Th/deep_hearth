//! Persistent material-lot identity, physical profile, provenance, and in-flight trace values.

use serde::{Deserialize, Deserializer, Serialize};

use crate::core::quantity::{Mass, Temperature};
use crate::core::time::SimulationTick;
use crate::material::{
    CommodityKey, MaterialComposition, MaterialPhaseStateError, MaterialRegistry,
    ParticleSizeDistribution, ParticleSizeRange, ParticleSizeStateError,
    validate_material_particle_size_state, validate_material_phase_state,
};

use super::super::storage_history::MaterialStorageHistory;
use super::StockpileId;

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

/// Invalid physical state for a consumed trace expected to represent pure authored matter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PureMaterialTraceValidationError {
    ZeroMass,
    UnknownCommodity {
        commodity: CommodityKey,
    },
    ImpureMaterial {
        commodity: CommodityKey,
    },
    InvalidPhaseState(MaterialPhaseStateError),
    InvalidParticleSizeState(ParticleSizeStateError),
    ProvenanceInFuture {
        latest_created_at: SimulationTick,
        current: SimulationTick,
    },
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

    /// Validates common persisted-state invariants for exact pure-material embodiment traces.
    ///
    /// Equipment, energy stores, and storage enclosures all embody authored pure-material assembly
    /// inputs. Construction-history rules remain with those owners, while this value owns the
    /// repeated physical validity checks for the trace itself.
    pub(crate) fn validate_pure_material_state(
        &self,
        materials: &MaterialRegistry,
        current: SimulationTick,
    ) -> Result<CommodityKey, PureMaterialTraceValidationError> {
        if self.mass.is_zero() {
            return Err(PureMaterialTraceValidationError::ZeroMass);
        }
        let commodity = self.profile.commodity();
        if !materials.has_commodity(commodity) {
            return Err(PureMaterialTraceValidationError::UnknownCommodity { commodity });
        }
        if self.profile.composition().pure_material() != Some(commodity.material()) {
            return Err(PureMaterialTraceValidationError::ImpureMaterial { commodity });
        }
        validate_material_phase_state(
            materials,
            commodity,
            self.profile.composition(),
            self.profile.temperature(),
        )
        .map_err(PureMaterialTraceValidationError::InvalidPhaseState)?;
        validate_material_particle_size_state(
            materials,
            commodity,
            self.profile.particle_size_distribution(),
        )
        .map_err(PureMaterialTraceValidationError::InvalidParticleSizeState)?;
        let latest_created_at = self.provenance.latest_created_at();
        if latest_created_at > current {
            return Err(PureMaterialTraceValidationError::ProvenanceInFuture {
                latest_created_at,
                current,
            });
        }
        Ok(commodity)
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
/// equivalent for current and future projection under the destination preservation rate, preserving
/// exact perishability cohorts instead of collapsing histories that only coincide at one tick.
/// Commodities without authored age-dependent behavior may coalesce conservatively across exposure
/// histories to keep lot fragmentation bounded.
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct MaterialLotProvenance {
    pub(in crate::inventory) earliest_created_at: SimulationTick,
    pub(in crate::inventory) latest_created_at: SimulationTick,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MaterialLotProvenanceRepresentation {
    earliest_created_at: SimulationTick,
    latest_created_at: SimulationTick,
}

impl<'de> Deserialize<'de> for MaterialLotProvenance {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let representation = MaterialLotProvenanceRepresentation::deserialize(deserializer)?;
        if representation.latest_created_at < representation.earliest_created_at {
            return Err(serde::de::Error::custom(
                "material provenance latest creation tick precedes its earliest creation tick",
            ));
        }
        Ok(Self {
            earliest_created_at: representation.earliest_created_at,
            latest_created_at: representation.latest_created_at,
        })
    }
}

impl MaterialLotProvenance {
    #[must_use]
    pub(in crate::inventory) const fn single(created_at: SimulationTick) -> Self {
        Self {
            earliest_created_at: created_at,
            latest_created_at: created_at,
        }
    }

    #[must_use]
    pub(in crate::inventory) fn merged(self, other: Self) -> Self {
        Self {
            earliest_created_at: self.earliest_created_at.min(other.earliest_created_at),
            latest_created_at: self.latest_created_at.max(other.latest_created_at),
        }
    }

    #[must_use]
    pub const fn earliest_created_at(self) -> SimulationTick {
        self.earliest_created_at
    }

    #[must_use]
    pub const fn latest_created_at(self) -> SimulationTick {
        self.latest_created_at
    }
}

#[cfg(test)]
#[path = "material_lot_tests.rs"]
mod tests;

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
