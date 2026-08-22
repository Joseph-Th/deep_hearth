//! Persistent finite geological deposits; child validation audits durable geological ownership.

use std::collections::BTreeMap;
#[cfg(any(test, feature = "test-gameplay"))]
use std::error::Error;
#[cfg(any(test, feature = "test-gameplay"))]
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::core::quantity::{Mass, Pressure, Temperature};
use crate::core::time::SimulationTick;
use crate::material::{CommodityKey, MaterialComposition};
#[cfg(any(test, feature = "test-gameplay"))]
use crate::material::{CompositionError, MaterialId};
use crate::spatial::VoxelBounds;

/// Persistent identifier for one finite geological matter owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeologicalDepositId(u32);

impl GeologicalDepositId {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        assert!(value != 0, "geological deposit id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Persistent lifecycle derived from whether extractable geological matter remains.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GeologicalDepositLifecycle {
    Available,
    Depleted,
}

/// Opaque world-generation authorization for one finite geological deposit.
///
/// The type is public so a future geological generator can pass an authorized plan into the
/// canonical admission function, but production callers cannot construct one directly. This keeps
/// geological matter creation behind a physical world-generation owner rather than a general spawn
/// API.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedDepositSpec {
    bounds: VoxelBounds,
    commodity: CommodityKey,
    mass: Mass,
    temperature: Temperature,
    excavation_hardness: Pressure,
    composition: MaterialComposition,
}

impl GeneratedDepositSpec {
    /// Test-side stand-in for a future regional world-generation resolver.
    ///
    /// Production code deliberately has no constructor until a real geological generator can
    /// establish this source authorization without exposing arbitrary matter creation.
    #[cfg(any(test, feature = "test-gameplay"))]
    pub(crate) fn new(
        bounds: VoxelBounds,
        commodity: CommodityKey,
        mass: Mass,
        temperature: Temperature,
        excavation_hardness: Pressure,
        composition: MaterialComposition,
    ) -> Result<Self, GeneratedDepositSpecError> {
        if mass.is_zero() {
            return Err(GeneratedDepositSpecError::ZeroMass);
        }
        if excavation_hardness == Pressure::ZERO {
            return Err(GeneratedDepositSpecError::ZeroExcavationHardness);
        }
        composition
            .validate()
            .map_err(GeneratedDepositSpecError::InvalidComposition)?;
        if composition.parts_per_million(commodity.material()) == 0 {
            return Err(GeneratedDepositSpecError::MissingHostMaterial {
                host: commodity.material(),
            });
        }
        Ok(Self {
            bounds,
            commodity,
            mass,
            temperature,
            excavation_hardness,
            composition,
        })
    }

    #[must_use]
    pub(crate) const fn bounds(&self) -> VoxelBounds {
        self.bounds
    }

    #[must_use]
    pub(crate) const fn commodity(&self) -> CommodityKey {
        self.commodity
    }

    #[must_use]
    pub(crate) const fn mass(&self) -> Mass {
        self.mass
    }

    #[must_use]
    pub(crate) const fn temperature(&self) -> Temperature {
        self.temperature
    }

    #[must_use]
    pub(crate) const fn excavation_hardness(&self) -> Pressure {
        self.excavation_hardness
    }

    #[must_use]
    pub(crate) const fn composition(&self) -> &MaterialComposition {
        &self.composition
    }
}

/// Invalid generated-deposit specification before registry resolution.
#[cfg(any(test, feature = "test-gameplay"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GeneratedDepositSpecError {
    ZeroMass,
    ZeroExcavationHardness,
    InvalidComposition(CompositionError),
    MissingHostMaterial { host: MaterialId },
}

#[cfg(any(test, feature = "test-gameplay"))]
impl Display for GeneratedDepositSpecError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroMass => {
                formatter.write_str("generated geological deposit mass must be nonzero")
            }
            Self::ZeroExcavationHardness => formatter
                .write_str("generated geological deposit excavation hardness must be nonzero"),
            Self::InvalidComposition(error) => {
                write!(
                    formatter,
                    "generated geological deposit has invalid composition: {error}"
                )
            }
            Self::MissingHostMaterial { host } => write!(
                formatter,
                "generated geological deposit composition omits host material {}",
                host.value()
            ),
        }
    }
}

#[cfg(any(test, feature = "test-gameplay"))]
impl Error for GeneratedDepositSpecError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidComposition(error) => Some(error),
            Self::ZeroMass | Self::ZeroExcavationHardness => None,
            Self::MissingHostMaterial { host: _host } => None,
        }
    }
}

/// One finite geological matter owner in persistent world space.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeologicalDepositRecord {
    pub(super) id: GeologicalDepositId,
    pub(super) bounds: VoxelBounds,
    pub(super) commodity: CommodityKey,
    pub(super) initial_mass: Mass,
    pub(super) remaining_mass: Mass,
    pub(super) temperature: Temperature,
    pub(super) excavation_hardness: Pressure,
    pub(super) composition: MaterialComposition,
    pub(super) lifecycle: GeologicalDepositLifecycle,
    pub(super) generated_at: SimulationTick,
}

impl GeologicalDepositRecord {
    #[must_use]
    pub const fn commodity(&self) -> CommodityKey {
        self.commodity
    }

    #[must_use]
    pub const fn remaining_mass(&self) -> Mass {
        self.remaining_mass
    }

    #[must_use]
    pub const fn temperature(&self) -> Temperature {
        self.temperature
    }

    /// Returns the deposit-scale resistance that extraction tooling must overcome.
    ///
    /// This geological property is intentionally independent of both the coarse commodity label and
    /// the assay composition. Material hardness describes constituents; excavation hardness describes
    /// the physical geological body that contains them.
    #[must_use]
    pub const fn excavation_hardness(&self) -> Pressure {
        self.excavation_hardness
    }

    #[must_use]
    pub const fn composition(&self) -> &MaterialComposition {
        &self.composition
    }

    #[must_use]
    pub const fn lifecycle(&self) -> GeologicalDepositLifecycle {
        self.lifecycle
    }
}

/// Runtime owner for finite geological deposits and their generated identities.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeologyState {
    revision: u64,
    next_deposit_id: u32,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    deposits: BTreeMap<GeologicalDepositId, GeologicalDepositRecord>,
}

impl GeologyState {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            revision: 0,
            next_deposit_id: 1,
            deposits: BTreeMap::new(),
        }
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub(super) const fn next_deposit_id(&self) -> u32 {
        self.next_deposit_id
    }

    #[must_use]
    pub fn get_deposit(&self, id: GeologicalDepositId) -> Option<&GeologicalDepositRecord> {
        self.deposits.get(&id)
    }

    /// Iterates deposits deterministically by persistent identity.
    pub fn deposits(&self) -> impl Iterator<Item = &GeologicalDepositRecord> {
        self.deposits.values()
    }

    pub(super) fn insert_deposit(
        &mut self,
        record: GeologicalDepositRecord,
        next_deposit_id: u32,
        next_revision: u64,
    ) {
        assert!(
            !self.deposits.contains_key(&record.id),
            "geological deposit ID allocation must be unique"
        );
        let previous = self.deposits.insert(record.id, record);
        assert!(
            previous.is_none(),
            "prechecked geological deposit insertion unexpectedly replaced a record"
        );
        self.next_deposit_id = next_deposit_id;
        self.revision = next_revision;
    }

    pub(crate) fn apply_extraction(
        &mut self,
        deposit: GeologicalDepositId,
        remaining_after: Mass,
        next_revision: u64,
    ) {
        let record = self.deposits.get_mut(&deposit).unwrap_or_else(|| {
            panic!("validated geological deposit disappeared without revision change")
        });
        record.remaining_mass = remaining_after;
        if remaining_after.is_zero() {
            record.lifecycle = GeologicalDepositLifecycle::Depleted;
        }
        self.revision = next_revision;
    }

    pub(crate) const fn has_valid_id_cursor(&self) -> bool {
        self.next_deposit_id != 0
    }
}

mod validation;

pub use validation::GeologyValidationError;
pub(crate) use validation::validate_loaded_geology;

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
