//! Conserved exchange of standing water and finite shallow catchment moisture.

use crate::content::FLUID_WATER;
use crate::core::quantity::{Temperature, Volume};
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::registry::Registries;
use crate::world::{SpringMoistureFlux, WEATHER_FRONT_TICKS, spring_moisture_flux};

use super::state::FluidContents;

const WATER_PER_FRONT: Volume = Volume::from_microliters(250_000);
const GROUNDWATER_TEMPERATURE: Temperature = Temperature::from_millikelvin(293_150);

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
                    let contents = (!visible.is_zero()).then(|| FluidContents {
                        fluid: FLUID_WATER,
                        volume: visible,
                        temperature: record.contents().expect("nonempty source").temperature(),
                    });
                    Some((id, contents, soaked))
                }
                SpringMoistureFlux::Returning => {
                    let space = record.capacity().checked_sub(surface)?;
                    let amount = catchment.soaked.min(WATER_PER_FRONT).min(space);
                    if amount.is_zero() {
                        return None;
                    }
                    let visible = surface.checked_add(amount)?;
                    let soaked = catchment.soaked.checked_sub(amount)?;
                    let temperature = record
                        .contents()
                        .map_or(GROUNDWATER_TEMPERATURE, |water| water.temperature());
                    Some((
                        id,
                        Some(FluidContents {
                            fluid: FLUID_WATER,
                            volume: visible,
                            temperature,
                        }),
                        soaked,
                    ))
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
