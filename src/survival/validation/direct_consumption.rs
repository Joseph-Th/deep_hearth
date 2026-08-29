//! Trusted-load validation for pending direct-consumption custody and its physical traces.

use std::collections::BTreeMap;

use crate::core::quantity::{AggregateMass, AggregateVolume};
use crate::core::time::{SimulationTick, TickSpan};
use crate::fluid::FluidRegistry;
use crate::inventory::ConsumedMaterialTrace;
use crate::material::{
    MaterialId, MaterialRegistry, validate_material_particle_size_state,
    validate_material_phase_state,
};

use super::SurvivalValidationError;
use crate::survival::state::{PendingDirectConsumption, PendingDrinking, PendingEating};
use crate::survival::{SurvivalRegistry, SurvivalState};

fn validate_pending_schedule(
    pending: &PendingDirectConsumption,
    current: SimulationTick,
) -> Result<TickSpan, SurvivalValidationError> {
    let started_at = pending.started_at();
    let completes_at = pending.completes_at();
    if started_at > current || completes_at <= current || completes_at <= started_at {
        return Err(SurvivalValidationError::PendingConsumptionScheduleInvalid);
    }
    Ok(TickSpan::new(completes_at.value() - started_at.value()))
}

fn validate_pending_meal_envelope(
    registry: &SurvivalRegistry,
    pending: &PendingEating,
    duration: TickSpan,
) -> Result<(), SurvivalValidationError> {
    let total_mass = pending
        .total_mass()
        .ok_or(SurvivalValidationError::PendingEatingMassOverflow)?;
    if total_mass.is_zero() {
        return Err(SurvivalValidationError::PendingEatingEmpty);
    }
    let direct = registry.physiology().direct_consumption();
    if total_mass > direct.maximum_meal_mass() {
        return Err(SurvivalValidationError::PendingEatingMassExceedsIntakeLimit);
    }
    if direct.meal_duration(total_mass) != Some(duration) {
        return Err(SurvivalValidationError::PendingConsumptionScheduleInvalid);
    }
    Ok(())
}

fn validate_pending_food_trace(
    registry: &SurvivalRegistry,
    materials: &MaterialRegistry,
    pending: &PendingEating,
    trace: &ConsumedMaterialTrace,
) -> Result<MaterialId, SurvivalValidationError> {
    if trace.mass().is_zero() {
        return Err(SurvivalValidationError::PendingEatingTraceInvalid);
    }
    let profile = trace.profile();
    let commodity = profile.commodity();
    let food = registry
        .get_food(commodity)
        .ok_or(SurvivalValidationError::PendingEatingTraceInvalid)?;
    if !materials.has_commodity(commodity)
        || profile.composition().pure_material() != Some(commodity.material())
    {
        return Err(SurvivalValidationError::PendingEatingTraceInvalid);
    }
    if validate_material_phase_state(
        materials,
        commodity,
        profile.composition(),
        profile.temperature(),
    )
    .is_err()
        || validate_material_particle_size_state(
            materials,
            commodity,
            profile.particle_size_distribution(),
        )
        .is_err()
        || !food
            .consumption_temperature()
            .contains(profile.temperature())
    {
        return Err(SurvivalValidationError::PendingEatingTraceInvalid);
    }
    let provenance = trace.provenance();
    if provenance.earliest_created_at() > provenance.latest_created_at()
        || provenance.latest_created_at() > pending.started_at()
    {
        return Err(SurvivalValidationError::PendingEatingTraceInvalid);
    }
    Ok(commodity.material())
}

fn add_pending_material_mass(
    pending_by_material: &mut BTreeMap<MaterialId, AggregateMass>,
    material: MaterialId,
    trace: &ConsumedMaterialTrace,
) -> Result<(), SurvivalValidationError> {
    let current = pending_by_material
        .get(&material)
        .copied()
        .unwrap_or(AggregateMass::ZERO);
    let next = current
        .checked_add(AggregateMass::from_mass(trace.mass()))
        .ok_or(SurvivalValidationError::PendingEatingMassOverflow)?;
    pending_by_material.insert(material, next);
    Ok(())
}

fn validate_pending_material_accounting(
    state: &SurvivalState,
    pending_by_material: BTreeMap<MaterialId, AggregateMass>,
) -> Result<(), SurvivalValidationError> {
    for (material, pending_mass) in pending_by_material {
        if state.consumed_mass(material) < pending_mass {
            return Err(SurvivalValidationError::PendingEatingAccountingMismatch { material });
        }
    }
    Ok(())
}

fn validate_pending_eating(
    registry: &SurvivalRegistry,
    materials: &MaterialRegistry,
    state: &SurvivalState,
    pending: &PendingEating,
    duration: TickSpan,
) -> Result<(), SurvivalValidationError> {
    if pending.consumed().is_empty() {
        return Err(SurvivalValidationError::PendingEatingEmpty);
    }
    validate_pending_meal_envelope(registry, pending, duration)?;

    let mut pending_by_material = BTreeMap::<MaterialId, AggregateMass>::new();
    for trace in pending.consumed() {
        let material = validate_pending_food_trace(registry, materials, pending, trace)?;
        add_pending_material_mass(&mut pending_by_material, material, trace)?;
    }
    validate_pending_material_accounting(state, pending_by_material)
}

fn validate_pending_drinking(
    registry: &SurvivalRegistry,
    fluids: &FluidRegistry,
    state: &SurvivalState,
    pending: PendingDrinking,
    duration: TickSpan,
) -> Result<(), SurvivalValidationError> {
    let direct = registry.physiology().direct_consumption();
    if pending.volume().is_zero()
        || pending.volume() > direct.maximum_drink_volume()
        || direct.drink_duration(pending.volume()) != Some(duration)
    {
        return Err(SurvivalValidationError::PendingDrinkingVolumeInvalid);
    }
    let fluid = pending.fluid();
    if fluids.get_fluid(fluid).is_none() {
        return Err(SurvivalValidationError::PendingDrinkingUnknownFluid { fluid });
    }
    let drink = registry
        .get_drink(fluid)
        .ok_or(SurvivalValidationError::PendingDrinkingNotDrinkable { fluid })?;
    if !drink
        .consumption_temperature()
        .contains(pending.temperature())
    {
        return Err(SurvivalValidationError::PendingDrinkingTemperatureInvalid);
    }
    if state.consumed_fluid_volume(fluid) < AggregateVolume::from_volume(pending.volume()) {
        return Err(SurvivalValidationError::PendingDrinkingAccountingMismatch { fluid });
    }
    Ok(())
}

pub(super) fn validate_pending_consumption(
    registry: &SurvivalRegistry,
    materials: &MaterialRegistry,
    fluids: &FluidRegistry,
    state: &SurvivalState,
    current: SimulationTick,
) -> Result<(), SurvivalValidationError> {
    let Some(pending) = state.pending_direct_consumption() else {
        return Ok(());
    };
    if state.player().is_none() {
        return Err(SurvivalValidationError::PendingConsumptionWithoutPlayer);
    }
    let duration = validate_pending_schedule(pending, current)?;
    match pending {
        PendingDirectConsumption::Eating(pending) => {
            validate_pending_eating(registry, materials, state, pending, duration)
        }
        PendingDirectConsumption::Drinking(pending) => {
            validate_pending_drinking(registry, fluids, state, *pending, duration)
        }
    }
}
