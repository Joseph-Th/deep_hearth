//! Validates material embodiment and plausible additive-upgrade history for energy stores.

use std::collections::BTreeMap;

use crate::core::quantity::Mass;
use crate::core::time::SimulationTick;
use crate::inventory::ConsumedMaterialTrace;
use crate::material::{
    CommodityKey, MaterialAssemblyProfile, MaterialRegistry, validate_material_particle_size_state,
    validate_material_phase_state,
};

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
    if trace.mass().is_zero() {
        return Err(EnergyValidationError::ZeroEmbodiedTrace { store: record.id });
    }
    let commodity = trace.profile().commodity();
    if !materials.has_commodity(commodity) {
        return Err(EnergyValidationError::UnknownEmbodiedCommodity {
            store: record.id,
            commodity,
        });
    }
    if trace.profile().composition().pure_material() != Some(commodity.material()) {
        return Err(EnergyValidationError::ImpureEmbodiedMaterial {
            store: record.id,
            commodity,
        });
    }
    validate_material_phase_state(
        materials,
        commodity,
        trace.profile().composition(),
        trace.profile().temperature(),
    )
    .map_err(|error| EnergyValidationError::InvalidEmbodiedPhaseState {
        store: record.id,
        error,
    })?;
    validate_material_particle_size_state(
        materials,
        commodity,
        trace.profile().particle_size_distribution(),
    )
    .map_err(
        |error| EnergyValidationError::InvalidEmbodiedParticleSizeState {
            store: record.id,
            error,
        },
    )?;

    let provenance = trace.provenance();
    if provenance.latest_created_at() < provenance.earliest_created_at() {
        return Err(EnergyValidationError::InvalidEmbodiedProvenanceRange { store: record.id });
    }
    if provenance.latest_created_at() > current {
        return Err(EnergyValidationError::EmbodiedProvenanceInFuture {
            store: record.id,
            latest_created_at: provenance.latest_created_at(),
            current,
        });
    }
    Ok(commodity)
}

fn validate_embodied_totals(
    record: &EnergyStoreRecord,
    assembly: &MaterialAssemblyProfile,
    traced_mass: Mass,
    mut stored_by_commodity: BTreeMap<CommodityKey, Mass>,
) -> Result<(), EnergyValidationError> {
    if traced_mass != assembly.input_mass() {
        return Err(EnergyValidationError::EmbodiedMassMismatch {
            store: record.id,
            traced: traced_mass,
            authored: assembly.input_mass(),
        });
    }
    for input in assembly.inputs() {
        let stored = stored_by_commodity
            .remove(&input.commodity())
            .unwrap_or(Mass::ZERO);
        if stored != input.mass() {
            return Err(EnergyValidationError::AssemblyMaterialMismatch {
                store: record.id,
                commodity: input.commodity(),
                stored,
                authored: input.mass(),
            });
        }
    }
    if let Some((commodity, stored)) = stored_by_commodity.into_iter().next() {
        return Err(EnergyValidationError::AssemblyMaterialMismatch {
            store: record.id,
            commodity,
            stored,
            authored: Mass::ZERO,
        });
    }
    Ok(())
}
