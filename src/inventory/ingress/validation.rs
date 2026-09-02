//! Parcel semantics and capacity accounting for canonical material ingress.

use std::collections::BTreeMap;

use crate::core::quantity::Mass;
use crate::core::time::SimulationTick;
use crate::material::CommodityKey;
use crate::registry::Registries;

use super::{MaterialIngressEntry, MaterialIngressError};
use crate::inventory::state::{StockpileId, StockpileRecord};
use crate::inventory::storage_validation::{
    CommodityReferenceError, StockpileStorageError, validate_commodity_reference,
    validate_stockpile_storage,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct IngressMassSummary {
    pub(super) total: Mass,
    pub(super) by_commodity: BTreeMap<CommodityKey, Mass>,
}

fn map_commodity_reference_error(error: CommodityReferenceError) -> MaterialIngressError {
    match error {
        CommodityReferenceError::UnknownMaterial { material } => {
            MaterialIngressError::UnknownMaterial { material }
        }
        CommodityReferenceError::UnknownForm { form } => MaterialIngressError::UnknownForm { form },
        CommodityReferenceError::UnsupportedCommodity { commodity } => {
            MaterialIngressError::Storage(StockpileStorageError::UnsupportedCommodity { commodity })
        }
    }
}

fn validate_ingress_entry(
    registries: &Registries,
    destination_record: &StockpileRecord,
    destination: StockpileId,
    entry: &MaterialIngressEntry,
    current_tick: SimulationTick,
) -> Result<(), MaterialIngressError> {
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
    validate_commodity_reference(registries, profile.commodity())
        .map_err(map_commodity_reference_error)?;
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
    Ok(())
}

pub(super) fn summarize_ingress_mass(
    registries: &Registries,
    destination_record: &StockpileRecord,
    destination: StockpileId,
    entries: &[MaterialIngressEntry],
    current_tick: SimulationTick,
) -> Result<IngressMassSummary, MaterialIngressError> {
    let mut total = Mass::ZERO;
    let mut by_commodity = BTreeMap::new();
    for entry in entries {
        validate_ingress_entry(
            registries,
            destination_record,
            destination,
            entry,
            current_tick,
        )?;
        total = total
            .checked_add(entry.mass)
            .ok_or(MaterialIngressError::MassOverflow {
                stockpile: destination,
            })?;
        let existing = by_commodity
            .get(&entry.profile.commodity())
            .copied()
            .unwrap_or(Mass::ZERO);
        by_commodity.insert(
            entry.profile.commodity(),
            existing
                .checked_add(entry.mass)
                .ok_or(MaterialIngressError::MassOverflow {
                    stockpile: destination,
                })?,
        );
    }
    Ok(IngressMassSummary {
        total,
        by_commodity,
    })
}

pub(super) fn validate_ingress_capacity(
    destination_record: &StockpileRecord,
    destination: StockpileId,
    summary: &IngressMassSummary,
) -> Result<(), MaterialIngressError> {
    validate_ingress_capacity_with_reserved_credit(
        destination_record,
        destination,
        summary,
        Mass::ZERO,
    )
}

pub(super) fn validate_ingress_capacity_with_reserved_credit(
    destination_record: &StockpileRecord,
    destination: StockpileId,
    summary: &IngressMassSummary,
    reserved_credit: Mass,
) -> Result<(), MaterialIngressError> {
    if reserved_credit > destination_record.reserved_inbound() || reserved_credit > summary.total {
        return Err(MaterialIngressError::CapacityExceeded {
            stockpile: destination,
            capacity: destination_record.capacity(),
            committed: destination_record
                .stored_mass()
                .checked_add(destination_record.reserved_inbound())
                .ok_or(MaterialIngressError::MassOverflow {
                    stockpile: destination,
                })?,
            requested: summary.total,
        });
    }
    let effective_reserved = destination_record
        .reserved_inbound()
        .checked_sub(reserved_credit)
        .ok_or(MaterialIngressError::MassOverflow {
            stockpile: destination,
        })?;
    let committed_before_incoming = destination_record
        .stored_mass()
        .checked_add(effective_reserved)
        .ok_or(MaterialIngressError::MassOverflow {
            stockpile: destination,
        })?;
    let after_incoming = committed_before_incoming.checked_add(summary.total).ok_or(
        MaterialIngressError::MassOverflow {
            stockpile: destination,
        },
    )?;
    if after_incoming > destination_record.capacity() {
        return Err(MaterialIngressError::CapacityExceeded {
            stockpile: destination,
            capacity: destination_record.capacity(),
            committed: committed_before_incoming,
            requested: summary.total,
        });
    }
    for (commodity, incoming) in &summary.by_commodity {
        destination_record
            .get_mass(*commodity)
            .checked_add(*incoming)
            .ok_or(MaterialIngressError::MassOverflow {
                stockpile: destination,
            })?;
    }
    Ok(())
}
