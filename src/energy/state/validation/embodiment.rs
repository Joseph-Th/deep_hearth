//! Validates material embodiment and plausible additive-upgrade history for energy stores.

use std::collections::BTreeMap;

use crate::core::quantity::Mass;
use crate::core::time::SimulationTick;
use crate::inventory::{ConsumedMaterialTrace, PureMaterialTraceValidationError};
use crate::material::{CommodityKey, MaterialAssemblyProfile, MaterialRegistry};

use super::super::super::definitions::{EnergyRegistry, EnergyStoreDefinitionId};
use super::super::EnergyStoreRecord;
use super::EnergyValidationError;

pub(super) fn validate_embodied_material(
    registry: &EnergyRegistry,
    materials: &MaterialRegistry,
    record: &EnergyStoreRecord,
    assembly: Option<&MaterialAssemblyProfile>,
    current: SimulationTick,
) -> Result<(), EnergyValidationError> {
    let Some(assembly) = assembly else {
        if !record.embodied_material.is_empty() {
            return Err(EnergyValidationError::UnexpectedAssemblyMaterial { store: record.id });
        }
        return Ok(());
    };

    if record.embodied_material.is_empty() {
        return Err(EnergyValidationError::MissingAssemblyMaterial { store: record.id });
    }

    let mut traced_mass = Mass::ZERO;
    let mut stored_by_commodity = BTreeMap::new();
    let mut post_construction_allowance =
        post_construction_addition_allowance(registry, record.definition);
    for trace in &record.embodied_material {
        let commodity = validate_embodied_trace(materials, record, trace, current)?;
        if trace.provenance().latest_created_at() > record.created_at {
            let remaining = post_construction_allowance
                .get(&commodity)
                .copied()
                .unwrap_or(Mass::ZERO);
            let Some(next_remaining) = remaining.checked_sub(trace.mass()) else {
                return Err(EnergyValidationError::EmbodiedProvenanceAfterConstruction {
                    store: record.id,
                    latest_created_at: trace.provenance().latest_created_at(),
                    created_at: record.created_at,
                });
            };
            post_construction_allowance.insert(commodity, next_remaining);
        }
        traced_mass = traced_mass
            .checked_add(trace.mass())
            .ok_or(EnergyValidationError::EmbodiedTraceMassOverflow { store: record.id })?;
        let stored = stored_by_commodity
            .get(&commodity)
            .copied()
            .unwrap_or(Mass::ZERO);
        let next = stored
            .checked_add(trace.mass())
            .ok_or(EnergyValidationError::EmbodiedTraceMassOverflow { store: record.id })?;
        stored_by_commodity.insert(commodity, next);
    }

    validate_embodied_totals(record, assembly, traced_mass, stored_by_commodity)
}

fn post_construction_addition_allowance(
    registry: &EnergyRegistry,
    definition: EnergyStoreDefinitionId,
) -> BTreeMap<CommodityKey, Mass> {
    let mut allowance = BTreeMap::new();
    let mut current = registry.get_store(definition).unwrap_or_else(|| {
        panic!(
            "validated energy store definition {} disappeared while checking embodied history",
            definition.value()
        )
    });
    while let Some(upgrade) = current.upgrade_profile() {
        for input in upgrade.additions().inputs() {
            let previous = allowance
                .get(&input.commodity())
                .copied()
                .unwrap_or(Mass::ZERO);
            let next = previous.checked_add(input.mass()).unwrap_or_else(|| {
                panic!(
                    "validated energy-store upgrade ancestry overflows post-construction allowance for commodity {}",
                    input.commodity().value()
                )
            });
            allowance.insert(input.commodity(), next);
        }
        current = registry.get_store(upgrade.from()).unwrap_or_else(|| {
            panic!(
                "validated energy-store upgrade base {} disappeared while checking embodied history",
                upgrade.from().value()
            )
        });
    }
    allowance
}

fn validate_embodied_trace(
    materials: &MaterialRegistry,
    record: &EnergyStoreRecord,
    trace: &ConsumedMaterialTrace,
    current: SimulationTick,
) -> Result<CommodityKey, EnergyValidationError> {
    trace
        .validate_pure_material_state(materials, current)
        .map_err(|error| match error {
            PureMaterialTraceValidationError::ZeroMass => {
                EnergyValidationError::ZeroEmbodiedTrace { store: record.id }
            }
            PureMaterialTraceValidationError::UnknownCommodity { commodity } => {
                EnergyValidationError::UnknownEmbodiedCommodity {
                    store: record.id,
                    commodity,
                }
            }
            PureMaterialTraceValidationError::ImpureMaterial { commodity } => {
                EnergyValidationError::ImpureEmbodiedMaterial {
                    store: record.id,
                    commodity,
                }
            }
            PureMaterialTraceValidationError::InvalidPhaseState(error) => {
                EnergyValidationError::InvalidEmbodiedPhaseState {
                    store: record.id,
                    error,
                }
            }
            PureMaterialTraceValidationError::InvalidParticleSizeState(error) => {
                EnergyValidationError::InvalidEmbodiedParticleSizeState {
                    store: record.id,
                    error,
                }
            }
            PureMaterialTraceValidationError::ProvenanceInFuture {
                latest_created_at,
                current,
            } => EnergyValidationError::EmbodiedProvenanceInFuture {
                store: record.id,
                latest_created_at,
                current,
            },
        })
}

fn validate_embodied_totals(
    record: &EnergyStoreRecord,
    assembly: &MaterialAssemblyProfile,
    traced_mass: Mass,
    stored_by_commodity: BTreeMap<CommodityKey, Mass>,
) -> Result<(), EnergyValidationError> {
    if traced_mass != assembly.input_mass() {
        return Err(EnergyValidationError::EmbodiedMassMismatch {
            store: record.id,
            traced: traced_mass,
            authored: assembly.input_mass(),
        });
    }
    if let Some((commodity, stored, authored)) = assembly.first_mass_mismatch(&stored_by_commodity)
    {
        return Err(EnergyValidationError::AssemblyMaterialMismatch {
            store: record.id,
            commodity,
            stored,
            authored,
        });
    }
    Ok(())
}
