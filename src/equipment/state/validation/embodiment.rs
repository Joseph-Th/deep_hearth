//! Validates equipment-owned material embodiment and plausible post-construction history.

use std::collections::BTreeMap;

use crate::core::quantity::Mass;
use crate::core::time::SimulationTick;
use crate::inventory::ConsumedMaterialTrace;
use crate::material::{
    CommodityKey, MaterialAssemblyProfile, MaterialRegistry, validate_material_particle_size_state,
    validate_material_phase_state,
};

use super::super::super::definitions::{
    EquipmentDefinition, EquipmentDefinitionId, EquipmentRegistry,
};
use super::super::EquipmentRecord;
use super::EquipmentValidationError;

pub(super) fn validate_equipment_material(
    definitions: &EquipmentRegistry,
    materials: &MaterialRegistry,
    record: &EquipmentRecord,
    definition: &EquipmentDefinition,
    current_tick: SimulationTick,
) -> Result<(), EquipmentValidationError> {
    if record.embodied_mass != definition.mass() {
        return Err(EquipmentValidationError::EmbodiedMassMismatch {
            equipment: record.id,
            stored: record.embodied_mass,
            authored: definition.mass(),
        });
    }
    validate_embodied_material(
        definitions,
        materials,
        record,
        definition.assembly_profile(),
        current_tick,
    )
}

fn validate_embodied_material(
    definitions: &EquipmentRegistry,
    materials: &MaterialRegistry,
    record: &EquipmentRecord,
    assembly: Option<&MaterialAssemblyProfile>,
    current_tick: SimulationTick,
) -> Result<(), EquipmentValidationError> {
    let Some(assembly) = assembly else {
        if !record.embodied_material.is_empty() {
            return Err(EquipmentValidationError::UnexpectedAssemblyMaterial {
                equipment: record.id,
            });
        }
        return Ok(());
    };
    if record.embodied_material.is_empty() {
        return Err(EquipmentValidationError::MissingAssemblyMaterial {
            equipment: record.id,
        });
    }

    let mut traced_mass = Mass::ZERO;
    let mut stored_by_commodity = BTreeMap::new();
    let mut post_construction_allowance =
        post_construction_material_allowance(definitions, record.definition);
    for trace in &record.embodied_material {
        let commodity = validate_embodied_trace(materials, record, trace, current_tick)?;
        consume_post_construction_allowance(
            record,
            trace,
            commodity,
            &mut post_construction_allowance,
        )?;
        traced_mass = traced_mass.checked_add(trace.mass()).ok_or(
            EquipmentValidationError::EmbodiedTraceMassOverflow {
                equipment: record.id,
            },
        )?;
        let current = stored_by_commodity
            .get(&commodity)
            .copied()
            .unwrap_or(Mass::ZERO);
        let next = current.checked_add(trace.mass()).ok_or(
            EquipmentValidationError::EmbodiedTraceMassOverflow {
                equipment: record.id,
            },
        )?;
        stored_by_commodity.insert(commodity, next);
    }
    validate_embodied_totals(record, assembly, traced_mass, stored_by_commodity)
}

fn consume_post_construction_allowance(
    record: &EquipmentRecord,
    trace: &ConsumedMaterialTrace,
    commodity: CommodityKey,
    allowance: &mut BTreeMap<CommodityKey, Mass>,
) -> Result<(), EquipmentValidationError> {
    if trace.provenance().latest_created_at() <= record.created_at {
        return Ok(());
    }
    let remaining = allowance.get(&commodity).copied().unwrap_or(Mass::ZERO);
    let Some(next_remaining) = remaining.checked_sub(trace.mass()) else {
        return Err(
            EquipmentValidationError::EmbodiedProvenanceAfterConstruction {
                equipment: record.id,
                latest_created_at: trace.provenance().latest_created_at(),
                created_at: record.created_at,
            },
        );
    };
    allowance.insert(commodity, next_remaining);
    Ok(())
}

fn post_construction_material_allowance(
    definitions: &EquipmentRegistry,
    definition: EquipmentDefinitionId,
) -> BTreeMap<CommodityKey, Mass> {
    let mut upgrade_allowance = BTreeMap::<CommodityKey, Mass>::new();
    let mut maintenance_allowance = BTreeMap::<CommodityKey, Mass>::new();
    let mut current = definitions.get_equipment(definition).unwrap_or_else(|| {
        panic!(
            "validated equipment definition {} disappeared while checking embodied history",
            definition.value()
        )
    });

    loop {
        collect_maintenance_allowance(current, &mut maintenance_allowance);
        let Some(upgrade) = current.upgrade_profile() else {
            break;
        };
        for input in upgrade.additions().inputs() {
            checked_add_allowance(&mut upgrade_allowance, input.commodity(), input.mass());
        }
        current = definitions.get_equipment(upgrade.from()).unwrap_or_else(|| {
            panic!(
                "validated equipment upgrade base {} disappeared while checking embodied history",
                upgrade.from().value()
            )
        });
    }

    for (commodity, maintenance_mass) in maintenance_allowance {
        checked_add_allowance(&mut upgrade_allowance, commodity, maintenance_mass);
    }
    cap_allowance_to_current_assembly(definitions, definition, &mut upgrade_allowance);
    upgrade_allowance
}

fn collect_maintenance_allowance(
    definition: &EquipmentDefinition,
    allowance: &mut BTreeMap<CommodityKey, Mass>,
) {
    let Some(maintenance) = definition
        .maintenance_profile()
        .filter(|profile| profile.is_component_replacement())
    else {
        return;
    };
    let commodity = maintenance.replacement();
    let previous = allowance.get(&commodity).copied().unwrap_or(Mass::ZERO);
    allowance.insert(
        commodity,
        previous.max(maintenance.full_service_replacement_mass()),
    );
}

fn checked_add_allowance(
    allowance: &mut BTreeMap<CommodityKey, Mass>,
    commodity: CommodityKey,
    mass: Mass,
) {
    let previous = allowance.get(&commodity).copied().unwrap_or(Mass::ZERO);
    let next = previous.checked_add(mass).unwrap_or_else(|| {
        panic!(
            "validated equipment history allowance overflows for commodity {}",
            commodity.value()
        )
    });
    allowance.insert(commodity, next);
}

fn cap_allowance_to_current_assembly(
    definitions: &EquipmentRegistry,
    definition: EquipmentDefinitionId,
    allowance: &mut BTreeMap<CommodityKey, Mass>,
) {
    let current = definitions.get_equipment(definition).unwrap_or_else(|| {
        unreachable!("validated equipment definition remained available during history check")
    });
    let Some(assembly) = current.assembly_profile() else {
        return;
    };
    for input in assembly.inputs() {
        if let Some(existing) = allowance.get_mut(&input.commodity()) {
            *existing = (*existing).min(input.mass());
        }
    }
}

fn validate_embodied_trace(
    materials: &MaterialRegistry,
    record: &EquipmentRecord,
    trace: &ConsumedMaterialTrace,
    current_tick: SimulationTick,
) -> Result<CommodityKey, EquipmentValidationError> {
    if trace.mass().is_zero() {
        return Err(EquipmentValidationError::ZeroEmbodiedTrace {
            equipment: record.id,
        });
    }
    let commodity = trace.profile().commodity();
    if !materials.has_commodity(commodity) {
        return Err(EquipmentValidationError::UnknownEmbodiedCommodity {
            equipment: record.id,
            commodity,
        });
    }
    if trace.profile().composition().pure_material() != Some(commodity.material()) {
        return Err(EquipmentValidationError::ImpureEmbodiedMaterial {
            equipment: record.id,
            commodity,
        });
    }
    validate_material_phase_state(
        materials,
        commodity,
        trace.profile().composition(),
        trace.profile().temperature(),
    )
    .map_err(
        |error| EquipmentValidationError::InvalidEmbodiedPhaseState {
            equipment: record.id,
            error,
        },
    )?;
    validate_material_particle_size_state(
        materials,
        commodity,
        trace.profile().particle_size_distribution(),
    )
    .map_err(
        |error| EquipmentValidationError::InvalidEmbodiedParticleSizeState {
            equipment: record.id,
            error,
        },
    )?;
    let provenance = trace.provenance();
    if provenance.latest_created_at() < provenance.earliest_created_at() {
        return Err(EquipmentValidationError::InvalidEmbodiedProvenanceRange {
            equipment: record.id,
        });
    }
    if provenance.latest_created_at() > current_tick {
        return Err(EquipmentValidationError::EmbodiedProvenanceInFuture {
            equipment: record.id,
            latest_created_at: provenance.latest_created_at(),
            current: current_tick,
        });
    }
    Ok(commodity)
}

fn validate_embodied_totals(
    record: &EquipmentRecord,
    assembly: &MaterialAssemblyProfile,
    traced_mass: Mass,
    mut stored_by_commodity: BTreeMap<CommodityKey, Mass>,
) -> Result<(), EquipmentValidationError> {
    if traced_mass != record.embodied_mass {
        return Err(EquipmentValidationError::EmbodiedTraceMassMismatch {
            equipment: record.id,
            stored: record.embodied_mass,
            traced: traced_mass,
        });
    }
    for input in assembly.inputs() {
        let stored = stored_by_commodity
            .remove(&input.commodity())
            .unwrap_or(Mass::ZERO);
        if stored != input.mass() {
            return Err(EquipmentValidationError::AssemblyMaterialMismatch {
                equipment: record.id,
                commodity: input.commodity(),
                stored,
                authored: input.mass(),
            });
        }
    }
    if let Some((commodity, stored)) = stored_by_commodity.into_iter().next() {
        return Err(EquipmentValidationError::AssemblyMaterialMismatch {
            equipment: record.id,
            commodity,
            stored,
            authored: Mass::ZERO,
        });
    }
    Ok(())
}
