//! Canonical admission of already-owned material into inventory.
//!
//! Source systems normalize both single-lot and multi-lot transfers into `MaterialIngressEntry`
//! values. Validation allocates destination lot identities and binds one inventory revision; apply
//! performs the corresponding owner mutation exactly once. No alternate single-lot ingress path is
//! retained.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::time::SimulationTick;
#[cfg(any(test, feature = "test-gameplay"))]
use crate::material::MaterialLotSpec;
use crate::material::{CommodityKey, CompositionError, FormId, MaterialId};
use crate::registry::Registries;

use super::coalescing::LotMergePolicy;
use super::lot_identity::LotIdentityPlanner;
use super::state::{
    ConsumedMaterialTrace, InventoryState, MaterialLotId, MaterialLotProfile,
    MaterialLotProvenance, MaterialLotRecord, MaterialStorageHistory, StockpileId,
    apply_insert_or_merge_new_lot,
};
use super::storage_validation::{
    CommodityReferenceError, StockpileStorageError, validate_commodity_reference,
    validate_stockpile_storage,
};

/// One source-owned material parcel prepared for canonical inventory admission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MaterialIngressEntry {
    mass: Mass,
    profile: MaterialLotProfile,
    provenance: MaterialLotProvenance,
}

impl MaterialIngressEntry {
    /// Converts a newly created lot specification into an ingress parcel with exact provenance.
    #[cfg(any(test, feature = "test-gameplay"))]
    #[must_use]
    pub(crate) fn from_lot_spec(
        specification: MaterialLotSpec,
        created_at: SimulationTick,
    ) -> Self {
        Self {
            mass: specification.mass(),
            profile: MaterialLotProfile {
                commodity: specification.commodity(),
                temperature: specification.temperature(),
                composition: specification.composition().clone(),
                particle_size: specification.particle_size_distribution().cloned(),
            },
            provenance: MaterialLotProvenance {
                earliest_created_at: created_at,
                latest_created_at: created_at,
            },
        }
    }

    /// Preserves the complete material profile and lot provenance of matter transferred from
    /// another owner. Inventory storage exposure starts when custody returns to a stockpile.
    #[must_use]
    pub(crate) fn from_consumed_trace(trace: &ConsumedMaterialTrace) -> Self {
        Self {
            mass: trace.mass(),
            profile: trace.profile().clone(),
            provenance: trace.provenance(),
        }
    }

    /// Preserves matter, thermal state, composition, and provenance while an owning subsystem
    /// physically degrades a consolidated parcel into another form of the same material.
    #[must_use]
    pub(crate) fn from_reformed_consumed_trace(
        trace: &ConsumedMaterialTrace,
        target_form: FormId,
    ) -> Self {
        let mut profile = trace.profile().clone();
        profile.commodity = CommodityKey::new(profile.commodity().material(), target_form);
        Self {
            mass: trace.mass(),
            profile,
            provenance: trace.provenance(),
        }
    }
}

/// Failure while validating one complete source-owned material ingress transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MaterialIngressError {
    Empty,
    UnknownStockpile {
        stockpile: StockpileId,
    },
    UnknownMaterial {
        material: MaterialId,
    },
    UnknownForm {
        form: FormId,
    },
    UnknownCompositionMaterial {
        material: MaterialId,
    },
    ZeroMass,
    InvalidComposition {
        error: CompositionError,
    },
    CompositionMissingHost {
        host: MaterialId,
    },
    Storage(StockpileStorageError),
    InvalidProvenance,
    ProvenanceInFuture {
        latest: SimulationTick,
        current: SimulationTick,
    },
    MassOverflow {
        stockpile: StockpileId,
    },
    CapacityExceeded {
        stockpile: StockpileId,
        capacity: Mass,
        committed: Mass,
        requested: Mass,
    },
    LotIdExhausted,
    RevisionExhausted,
}

impl Display for MaterialIngressError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => formatter.write_str("material ingress must contain at least one parcel"),
            Self::UnknownStockpile { stockpile } => {
                write!(formatter, "unknown stockpile id {}", stockpile.value())
            }
            Self::UnknownMaterial { material } => {
                write!(formatter, "unknown material id {}", material.value())
            }
            Self::UnknownForm { form } => write!(formatter, "unknown form id {}", form.value()),
            Self::UnknownCompositionMaterial { material } => write!(
                formatter,
                "material ingress composition references unknown material {}",
                material.value()
            ),
            Self::ZeroMass => formatter.write_str("material ingress mass must be nonzero"),
            Self::InvalidComposition { error } => {
                write!(
                    formatter,
                    "material ingress has invalid composition: {error}"
                )
            }
            Self::CompositionMissingHost { host } => write!(
                formatter,
                "material ingress composition omits host material {}",
                host.value()
            ),
            Self::Storage(error) => {
                write!(formatter, "stockpile rejects material ingress: {error}")
            }
            Self::InvalidProvenance => formatter.write_str(
                "material ingress provenance ends before its earliest represented creation tick",
            ),
            Self::ProvenanceInFuture { latest, current } => write!(
                formatter,
                "material ingress provenance reaches tick {} after current tick {}",
                latest.value(),
                current.value()
            ),
            Self::MassOverflow { stockpile } => write!(
                formatter,
                "material ingress overflows mass accounting in stockpile {}",
                stockpile.value()
            ),
            Self::CapacityExceeded {
                stockpile,
                capacity,
                committed,
                requested,
            } => write!(
                formatter,
                "stockpile {} capacity {} mg exceeded: {} mg committed, {} mg ingress requested",
                stockpile.value(),
                capacity.milligrams(),
                committed.milligrams(),
                requested.milligrams()
            ),
            Self::LotIdExhausted => {
                formatter.write_str("material lot identifier space is exhausted")
            }
            Self::RevisionExhausted => formatter.write_str("inventory revision space is exhausted"),
        }
    }
}

impl Error for MaterialIngressError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidComposition { error } => Some(error),
            Self::Storage(error) => Some(error),
            Self::UnknownStockpile {
                stockpile: _stockpile,
            }
            | Self::MassOverflow {
                stockpile: _stockpile,
            } => None,
            Self::UnknownMaterial { material: _id }
            | Self::UnknownCompositionMaterial { material: _id }
            | Self::CompositionMissingHost { host: _id } => None,
            Self::UnknownForm { form: _form } => None,
            Self::ProvenanceInFuture {
                latest: _latest,
                current: _current,
            } => None,
            Self::CapacityExceeded {
                stockpile: _stockpile,
                capacity: _capacity,
                committed: _committed,
                requested: _requested,
            } => None,
            Self::Empty
            | Self::ZeroMass
            | Self::InvalidProvenance
            | Self::LotIdExhausted
            | Self::RevisionExhausted => None,
        }
    }
}

/// Consumed proof that a complete source-owned parcel set can enter one stockpile atomically.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ValidatedMaterialIngress {
    expected_revision: u64,
    next_revision: u64,
    destination: StockpileId,
    entries: Vec<MaterialIngressEntry>,
    lot_ids: Vec<MaterialLotId>,
    merge_policies: Vec<LotMergePolicy>,
    next_lot_id: u64,
    current_tick: SimulationTick,
}

impl ValidatedMaterialIngress {
    pub(crate) const fn expected_revision(&self) -> u64 {
        self.expected_revision
    }
}

/// Validates all material parcels entering one stockpile under one inventory revision.
pub(crate) fn validate_material_ingress(
    registries: &Registries,
    state: &InventoryState,
    destination: StockpileId,
    entries: impl IntoIterator<Item = MaterialIngressEntry>,
    current_tick: SimulationTick,
) -> Result<ValidatedMaterialIngress, MaterialIngressError> {
    let entries = entries.into_iter().collect::<Vec<_>>();
    if entries.is_empty() {
        return Err(MaterialIngressError::Empty);
    }
    let Some(destination_record) = state.get_stockpile(destination) else {
        return Err(MaterialIngressError::UnknownStockpile {
            stockpile: destination,
        });
    };

    let mut total = Mass::ZERO;
    let mut by_commodity = BTreeMap::new();
    for entry in &entries {
        if entry.mass.is_zero() {
            return Err(MaterialIngressError::ZeroMass);
        }
        let profile = &entry.profile;
        profile
            .composition()
            .validate()
            .map_err(|error| MaterialIngressError::InvalidComposition { error })?;
        if profile
            .composition()
            .parts_per_million(profile.commodity().material())
            == 0
        {
            return Err(MaterialIngressError::CompositionMissingHost {
                host: profile.commodity().material(),
            });
        }
        validate_commodity_reference(registries, profile.commodity()).map_err(
            |error| match error {
                CommodityReferenceError::UnknownMaterial { material } => {
                    MaterialIngressError::UnknownMaterial { material }
                }
                CommodityReferenceError::UnknownForm { form } => {
                    MaterialIngressError::UnknownForm { form }
                }
            },
        )?;
        for component in profile.composition().components() {
            if registries
                .materials()
                .get_material(component.material())
                .is_none()
            {
                return Err(MaterialIngressError::UnknownCompositionMaterial {
                    material: component.material(),
                });
            }
        }
        validate_stockpile_storage(
            registries,
            destination_record,
            destination,
            profile.commodity(),
            profile.composition(),
            profile.temperature(),
            profile.particle_size_distribution(),
        )
        .map_err(MaterialIngressError::Storage)?;
        if entry.provenance.latest_created_at() < entry.provenance.earliest_created_at() {
            return Err(MaterialIngressError::InvalidProvenance);
        }
        if entry.provenance.latest_created_at() > current_tick {
            return Err(MaterialIngressError::ProvenanceInFuture {
                latest: entry.provenance.latest_created_at(),
                current: current_tick,
            });
        }
        total = total
            .checked_add(entry.mass)
            .ok_or(MaterialIngressError::MassOverflow {
                stockpile: destination,
            })?;
        let existing = by_commodity
            .get(&profile.commodity())
            .copied()
            .unwrap_or(Mass::ZERO);
        by_commodity.insert(
            profile.commodity(),
            existing
                .checked_add(entry.mass)
                .ok_or(MaterialIngressError::MassOverflow {
                    stockpile: destination,
                })?,
        );
    }

    let committed = destination_record
        .stored_mass
        .checked_add(destination_record.reserved_inbound)
        .ok_or(MaterialIngressError::MassOverflow {
            stockpile: destination,
        })?;
    let after = committed
        .checked_add(total)
        .ok_or(MaterialIngressError::MassOverflow {
            stockpile: destination,
        })?;
    if after > destination_record.capacity {
        return Err(MaterialIngressError::CapacityExceeded {
            stockpile: destination,
            capacity: destination_record.capacity,
            committed,
            requested: total,
        });
    }
    for (commodity, incoming) in by_commodity {
        destination_record
            .get_mass(commodity)
            .checked_add(incoming)
            .ok_or(MaterialIngressError::MassOverflow {
                stockpile: destination,
            })?;
    }

    let merge_policies = entries
        .iter()
        .map(|entry| LotMergePolicy::for_commodity(registries, entry.profile.commodity()))
        .collect::<Vec<_>>();
    let preservation_multiplier_ppm = destination_record
        .storage_profile()
        .preservation_multiplier_ppm();
    let storage_history = MaterialStorageHistory::new(current_tick);
    let mut identity_planner = LotIdentityPlanner::new(state, std::iter::empty());
    let mut lot_ids = Vec::with_capacity(entries.len());
    for (entry, merge_policy) in entries.iter().zip(&merge_policies) {
        lot_ids.push(
            identity_planner
                .plan(
                    destination,
                    &entry.profile,
                    storage_history,
                    current_tick,
                    preservation_multiplier_ppm,
                    *merge_policy,
                )
                .ok_or(MaterialIngressError::LotIdExhausted)?,
        );
    }
    let next_lot_id = identity_planner.next_lot_id();
    let next_revision = state
        .revision()
        .checked_add(1)
        .ok_or(MaterialIngressError::RevisionExhausted)?;

    Ok(ValidatedMaterialIngress {
        expected_revision: state.revision(),
        next_revision,
        destination,
        entries,
        lot_ids,
        merge_policies,
        next_lot_id,
        current_tick,
    })
}

/// Applies a validated parcel set after its cross-owner transaction rechecks inventory revision.
pub(crate) fn apply_material_ingress(
    state: &mut InventoryState,
    ingress: ValidatedMaterialIngress,
) -> Vec<MaterialLotId> {
    let ValidatedMaterialIngress {
        expected_revision,
        next_revision,
        destination,
        entries,
        lot_ids,
        merge_policies,
        next_lot_id,
        current_tick,
    } = ingress;
    assert_eq!(
        state.revision(),
        expected_revision,
        "material ingress commit requires its validated inventory revision"
    );
    debug_assert_eq!(
        entries.len(),
        lot_ids.len(),
        "validated material ingress must bind one lot identity per parcel"
    );
    debug_assert_eq!(
        entries.len(),
        merge_policies.len(),
        "validated material ingress must bind one lot merge policy per parcel"
    );

    let preservation_multiplier_ppm = state
        .get_stockpile(destination)
        .unwrap_or_else(|| panic!("validated material ingress destination disappeared"))
        .storage_profile()
        .preservation_multiplier_ppm();

    let mut resulting_lots = Vec::with_capacity(entries.len());
    for ((entry, lot_id), merge_policy) in entries.into_iter().zip(lot_ids).zip(merge_policies) {
        let resulting = apply_insert_or_merge_new_lot(
            state,
            MaterialLotRecord {
                id: lot_id,
                stockpile: destination,
                mass: entry.mass,
                profile: entry.profile,
                provenance: entry.provenance,
                storage_history: MaterialStorageHistory::new(current_tick),
            },
            merge_policy,
            current_tick,
            preservation_multiplier_ppm,
        );
        resulting_lots.push(resulting);
    }
    state.apply_lot_cursor_and_revision(next_lot_id, next_revision);
    resulting_lots
}

#[cfg(test)]
#[path = "ingress_tests.rs"]
mod tests;
