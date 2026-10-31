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

pub(crate) fn apply_natural_water_cycle(
    registries: &Registries,
    state: &mut AppState,
    at: SimulationTick,
) {
    if !at.value().is_multiple_of(WEATHER_FRONT_TICKS) {
        return;
    }
    let Some(next_revision) = state.fluid().revision().checked_add(1) else {
        return;
    };
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
                    let amount = surface.min(WATER_PER_FRONT).min(space);
                    if amount.is_zero() {
                        return None;
                    }
                    let visible = surface.checked_sub(amount)?;
                    let soaked = catchment.soaked.checked_add(amount)?;
                    let incoming_temperature = record.contents()?.temperature();
                    let soaked_temperature = match catchment.soaked_temperature {
                        Some(existing) => exact_mixed_temperature(
                            (catchment.soaked, existing),
                            (amount, incoming_temperature),
                            soaked,
                        )
                        .ok()?,
                        None => incoming_temperature,
                    };
                    let contents = (!visible.is_zero()).then(|| FluidContents {
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
                    let amount = catchment.soaked.min(WATER_PER_FRONT).min(space);
                    if amount.is_zero() {
                        return None;
                    }
                    let visible = surface.checked_add(amount)?;
                    let soaked = catchment.soaked.checked_sub(amount)?;
                    let groundwater_temperature = catchment.soaked_temperature?;
                    let temperature = match record.contents() {
                        Some(water) => exact_mixed_temperature(
                            (surface, water.temperature()),
                            (amount, groundwater_temperature),
                            visible,
                        )
                        .ok()?,
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
    if !changes.is_empty() {
        state
            .fluid_state_mut()
            .apply_spring_soil_exchange(changes, next_revision);
    }
}

#[cfg(test)]
#[path = "spring_cycle_tests.rs"]
mod tests;
