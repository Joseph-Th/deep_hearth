//! Familiar revision-bound stack projection over authoritative material lots.
//!
//! Lots remain the inventory owner's physical records. This read surface groups only matter that
//! the same owner already considers merge-compatible, so presentation and direct manipulation can
//! expose ordinary stacks without copying custody or leaking runtime lot identities.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Mass, Temperature};
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::material::{
    CommodityKey, MaterialComposition, ParticleSizeDistribution, ParticleSizeRange,
};
use crate::registry::Registries;

use super::coalescing::{LotMergePolicy, lots_are_merge_compatible};
use super::{
    MaterialLotProfile, MaterialLotProvenance, MaterialLotSelection, MaterialStorageHistory,
    StockpileId,
};

/// Failure while projecting or resolving a familiar material stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialStackProjectionError {
    UnknownStockpile {
        stockpile: StockpileId,
    },
    StaleInventoryRevision {
        expected: u64,
        actual: u64,
    },
    SourceMismatch {
        expected: StockpileId,
        actual: StockpileId,
    },
    ZeroMass,
    MassExceedsStack {
        requested: Mass,
        available: Mass,
    },
}

impl Display for MaterialStackProjectionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownStockpile { stockpile } => {
                write!(
                    formatter,
                    "unknown material-stack stockpile {}",
                    stockpile.value()
                )
            }
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "material stack expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::SourceMismatch { expected, actual } => write!(
                formatter,
                "material stack belongs to stockpile {} but transfer source is {}",
                actual.value(),
                expected.value()
            ),
            Self::ZeroMass => formatter.write_str("material stack transfer mass must be nonzero"),
            Self::MassExceedsStack {
                requested,
                available,
            } => write!(
                formatter,
                "material stack contains {} mg but transfer requests {} mg",
                available.milligrams(),
                requested.milligrams()
            ),
        }
    }
}

impl Error for MaterialStackProjectionError {}

/// One familiar stack projected from one inventory revision.
///
/// Exact lot membership is intentionally private. Callers may inspect the physical distinctions
/// that make this stack separate and pass the projection back to an owner action, but do not need
/// to persist or manipulate lot IDs themselves.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterialStackProjection {
    expected_inventory_revision: u64,
    stockpile: StockpileId,
    profile: MaterialLotProfile,
    provenance: MaterialLotProvenance,
    storage_history: MaterialStorageHistory,
    ambient_storage_age_parts: u128,
    mass: Mass,
    selections: Vec<MaterialLotSelection>,
}

impl MaterialStackProjection {
    #[must_use]
    pub const fn stockpile(&self) -> StockpileId {
        self.stockpile
    }

    #[must_use]
    pub const fn commodity(&self) -> CommodityKey {
        self.profile.commodity()
    }

    #[must_use]
    pub const fn mass(&self) -> Mass {
        self.mass
    }

    #[must_use]
    pub const fn temperature(&self) -> Temperature {
        self.profile.temperature()
    }

    #[must_use]
    pub const fn composition(&self) -> &MaterialComposition {
        self.profile.composition()
    }

    #[must_use]
    pub fn particle_size(&self) -> Option<ParticleSizeRange> {
        self.profile.particle_size()
    }

    #[must_use]
    pub const fn particle_size_distribution(&self) -> Option<&ParticleSizeDistribution> {
        self.profile.particle_size_distribution()
    }

    #[must_use]
    pub const fn created_at(&self) -> SimulationTick {
        self.provenance.earliest_created_at()
    }

    #[must_use]
    pub const fn latest_created_at(&self) -> SimulationTick {
        self.provenance.latest_created_at()
    }

    /// Current ambient-equivalent storage exposure used by perishability rules.
    ///
    /// One ambient simulation tick equals `STORAGE_AGE_PARTS_PER_TICK` parts. This is a read-side
    /// physical freshness fact, not a lot identity or a second storage-history owner.
    #[must_use]
    pub const fn ambient_storage_age_parts(&self) -> u128 {
        self.ambient_storage_age_parts
    }

    pub(crate) const fn storage_history(&self) -> MaterialStorageHistory {
        self.storage_history
    }

    pub(crate) fn validate_current(
        &self,
        state: &AppState,
    ) -> Result<(), MaterialStackProjectionError> {
        let actual_revision = state.inventory().revision();
        if actual_revision != self.expected_inventory_revision {
            return Err(MaterialStackProjectionError::StaleInventoryRevision {
                expected: self.expected_inventory_revision,
                actual: actual_revision,
            });
        }
        Ok(())
    }

    pub(crate) fn selections_for_mass(
        &self,
        state: &AppState,
        expected_source: StockpileId,
        requested: Mass,
    ) -> Result<Vec<MaterialLotSelection>, MaterialStackProjectionError> {
        self.validate_current(state)?;
        if self.stockpile != expected_source {
            return Err(MaterialStackProjectionError::SourceMismatch {
                expected: expected_source,
                actual: self.stockpile,
            });
        }
        if requested.is_zero() {
            return Err(MaterialStackProjectionError::ZeroMass);
        }
        if requested > self.mass {
            return Err(MaterialStackProjectionError::MassExceedsStack {
                requested,
                available: self.mass,
            });
        }

        let mut remaining = requested;
        let mut resolved = Vec::new();
        for selection in &self.selections {
            if remaining.is_zero() {
                break;
            }
            let mass = if remaining < selection.mass() {
                remaining
            } else {
                selection.mass()
            };
            resolved.push(MaterialLotSelection::new(selection.lot(), mass));
            remaining = remaining.checked_sub(mass).unwrap_or_else(|| {
                unreachable!("selected stack slice cannot exceed remaining mass")
            });
        }
        assert!(
            remaining.is_zero(),
            "projected stack membership must cover its reported mass"
        );
        Ok(resolved)
    }
}

/// Projects one stockpile as stable familiar stacks without changing inventory ownership.
///
/// A stack boundary is exactly a boundary the inventory owner would preserve while coalescing:
/// profile differences always split, and age-sensitive materials additionally split when their
/// storage histories do not have identical current and future projections under this stockpile's
/// preservation rate.
pub fn project_material_stacks(
    registries: &Registries,
    state: &AppState,
    stockpile: StockpileId,
) -> Result<Vec<MaterialStackProjection>, MaterialStackProjectionError> {
    let record = state
        .inventory()
        .get_stockpile(stockpile)
        .ok_or(MaterialStackProjectionError::UnknownStockpile { stockpile })?;
    let preservation = record.storage_profile().preservation_multiplier_ppm();
    let at = state.tick();
    let revision = state.inventory().revision();
    let mut stacks = Vec::<MaterialStackProjection>::new();

    for lot_id in state.inventory().lot_ids(stockpile) {
        let lot = state.inventory().get_lot(lot_id).unwrap_or_else(|| {
            panic!("runtime invariant broken: indexed material lot disappeared")
        });
        let merge_policy = LotMergePolicy::for_commodity(registries, lot.commodity());
        if let Some(stack) = stacks.iter_mut().find(|stack| {
            stack.commodity() == lot.commodity()
                && lots_are_merge_compatible(
                    &stack.profile,
                    stack.storage_history,
                    lot.profile(),
                    lot.storage_history(),
                    at,
                    preservation,
                    merge_policy,
                )
        }) {
            stack.mass = stack
                .mass
                .checked_add(lot.mass())
                .unwrap_or_else(|| panic!("validated stockpile stack mass overflowed"));
            stack.provenance = stack.provenance.merged(lot.provenance());
            stack
                .selections
                .push(MaterialLotSelection::new(lot.id(), lot.mass()));
            continue;
        }
        stacks.push(MaterialStackProjection {
            expected_inventory_revision: revision,
            stockpile,
            profile: lot.profile().clone(),
            provenance: lot.provenance(),
            storage_history: lot.storage_history(),
            ambient_storage_age_parts: lot
                .storage_history()
                .project(at, preservation)
                .unwrap_or_else(|| panic!("validated material storage-age projection overflowed")),
            mass: lot.mass(),
            selections: vec![MaterialLotSelection::new(lot.id(), lot.mass())],
        });
    }
    Ok(stacks)
}

#[cfg(test)]
#[path = "stack_projection_tests.rs"]
mod tests;
