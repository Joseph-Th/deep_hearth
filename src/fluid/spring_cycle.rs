//! Conserved exchange of standing water and finite shallow catchment moisture.

use crate::content::FLUID_WATER;
use crate::core::quantity::Volume;
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::registry::Registries;
use crate::world::{SpringMoistureFlux, WEATHER_FRONT_TICKS, spring_moisture_flux};

use super::physics::exact_mixed_temperature;
use super::state::{FluidContents, SpringSoilExchange};

const WATER_PER_FRONT: Volume = Volume::from_microliters(250_000);

/// Choose the largest available exchange that preserves exact millikelvin heat.
///
/// Mixing an existing volume V at a different temperature with x incoming
/// microliters is representable precisely when (V + x) divides
/// V * |existing_temperature - incoming_temperature|. The preferred batch
/// need not satisfy that condition even when a smaller batch does. Search
/// the narrower of the possible temperature deltas and the bounded exchange
/// volumes instead of abandoning an otherwise possible weather-front transfer.
fn largest_exact_mixing_exchange(
    existing: (Volume, crate::core::quantity::Temperature),
    incoming_temperature: crate::core::quantity::Temperature,
    maximum: Volume,
) -> Option<Volume> {
    let available = u128::from(maximum.microliters());
    if available == 0 {
        return None;
    }
    let original = u128::from(existing.0.microliters());
    let delta = u128::from(
        existing
            .1
            .millikelvin()
            .abs_diff(incoming_temperature.millikelvin()),
    );
    if original == 0 || delta == 0 {
        return Some(maximum);
    }
    let numerator = original * delta;
    let first_quotient = numerator.div_ceil(original + available);
    let last_quotient = numerator / (original + 1);
    if first_quotient > last_quotient {
        return None;
    }
    let amount = if last_quotient - first_quotient < available {
        (first_quotient..=last_quotient)
            .find(|&quotient| numerator.is_multiple_of(quotient))
            .map(|quotient| numerator / quotient - original)
    } else {
        (1..=available)
            .rev()
            .find(|&amount| numerator.is_multiple_of(original + amount))
    }?;
    Some(Volume::from_microliters(
        u64::try_from(amount).expect("exact exchange is bounded by preferred volume"),
    ))
}

/// Checked finite-water exchange resolved against the final same-tick roof and world state.
pub(crate) struct NaturalWaterCyclePlan {
    changes: Vec<SpringSoilExchange>,
    next_revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NaturalWaterCycleError {
    RevisionExhausted,
}

pub(crate) fn decide_natural_water_cycle(
    registries: &Registries,
    state: &AppState,
    at: SimulationTick,
) -> Result<Option<NaturalWaterCyclePlan>, NaturalWaterCycleError> {
    if !at.value().is_multiple_of(WEATHER_FRONT_TICKS) {
        return Ok(None);
    }
    let changes = state
        .fluid()
        .spring_soil()
        .filter_map(|(id, catchment)| {
            if state.logistics().fluid_store_position(id) != Some(catchment.position) {
                return None;
            }
            let record = state.fluid().get_store(id)?;
            if record.supported_by().is_some()
                || record
                    .contents()
                    .is_some_and(|water| water.fluid() != FLUID_WATER)
            {
                return None;
            }
            let direction = spring_moisture_flux(registries, state, catchment.position, at);
            let surface = record.stored_volume();
            match direction {
                SpringMoistureFlux::Drying => {
                    let space = record.capacity().checked_sub(catchment.soaked)?;
                    let maximum = surface.min(WATER_PER_FRONT).min(space);
                    let incoming_temperature = record.contents()?.temperature();
                    let existing_temperature = if catchment.soaked.is_zero() {
                        incoming_temperature
                    } else {
                        catchment.soaked_temperature?
                    };
                    let amount = largest_exact_mixing_exchange(
                        (catchment.soaked, existing_temperature),
                        incoming_temperature,
                        maximum,
                    )?;
                    let visible = surface.checked_sub(amount)?;
                    let soaked = catchment.soaked.checked_add(amount)?;
                    let soaked_temperature = match catchment.soaked_temperature {
                        Some(existing) => exact_mixed_temperature(
                            (catchment.soaked, existing),
                            (amount, incoming_temperature),
                            soaked,
                        )
                        .expect("selected exchange admits exact soil temperature"),
                        None => incoming_temperature,
                    };
                    let contents = (!visible.is_zero()).then_some(FluidContents {
                        fluid: FLUID_WATER,
                        volume: visible,
                        temperature: incoming_temperature,
                    });
                    Some(SpringSoilExchange {
                        store: id,
                        surface: contents,
                        soaked,
                        soaked_temperature: Some(soaked_temperature),
                    })
                }
                SpringMoistureFlux::Returning => {
                    let space = record.capacity().checked_sub(surface)?;
                    let maximum = catchment.soaked.min(WATER_PER_FRONT).min(space);
                    let groundwater_temperature = catchment.soaked_temperature?;
                    let existing_temperature = record
                        .contents()
                        .map_or(groundwater_temperature, FluidContents::temperature);
                    let amount = largest_exact_mixing_exchange(
                        (surface, existing_temperature),
                        groundwater_temperature,
                        maximum,
                    )?;
                    let visible = surface.checked_add(amount)?;
                    let soaked = catchment.soaked.checked_sub(amount)?;
                    let temperature = match record.contents() {
                        Some(water) => exact_mixed_temperature(
                            (surface, water.temperature()),
                            (amount, groundwater_temperature),
                            visible,
                        )
                        .expect("selected exchange admits exact surface temperature"),
                        None => groundwater_temperature,
                    };
                    Some(SpringSoilExchange {
                        store: id,
                        surface: Some(FluidContents {
                            fluid: FLUID_WATER,
                            volume: visible,
                            temperature,
                        }),
                        soaked,
                        soaked_temperature: (!soaked.is_zero()).then_some(groundwater_temperature),
                    })
                }
                SpringMoistureFlux::Balanced => None,
            }
        })
        .collect::<Vec<_>>();
    if changes.is_empty() {
        return Ok(None);
    }
    let next_revision = state
        .fluid()
        .revision()
        .checked_add(1)
        .ok_or(NaturalWaterCycleError::RevisionExhausted)?;
    Ok(Some(NaturalWaterCyclePlan {
        changes,
        next_revision,
    }))
}

pub(crate) fn apply_natural_water_cycle(state: &mut AppState, plan: Option<NaturalWaterCyclePlan>) {
    if let Some(plan) = plan {
        state
            .fluid_state_mut()
            .apply_spring_soil_exchange(plan.changes, plan.next_revision);
    }
}

#[cfg(test)]
#[path = "spring_cycle_tests.rs"]
mod tests;
