//! Exact-local water and gathering catalogs, with survival-owned resource estimates.

use std::fmt::Write as _;

use deep_hearth::core::quantity::Volume;
use deep_hearth::surface::{
    PlayerSurfaceGatheringOption, SurfaceGatheringError,
    assess_player_surface_gathering_option_admission, available_player_world_drops,
};
use deep_hearth::survival::{LocalDrinkSource, available_local_drink_sources};
use deep_hearth::world::assess_player_local_natural_spring;
use deep_hearth::{AppState, Registries};

use super::{celsius, freshness_label, grams, milliliters, name, runway_label};
use crate::text_play::{activity, errors, fire, gathering::DefaultGatheringBatch};

/// Numbered exact-local drinking choices, never remote or unsafe water.
/// Choosing one still revalidates the actual finite source on the input tick.
pub fn drink_options(registries: &Registries, state: &AppState) -> String {
    let sources = available_local_drink_sources(registries, state).collect::<Vec<_>>();
    let mut output = drink_sources_report(&sources)
        .unwrap_or_else(|| "No drinkable source at your current location.\n".to_owned());
    output.push_str(&natural_spring_report(registries, state));
    output
}

pub(super) fn drink_sources_report(sources: &[LocalDrinkSource]) -> Option<String> {
    if sources.is_empty() {
        return None;
    }
    let mut output = String::from("Drink here (drink N):\n");
    for (index, source) in sources.iter().enumerate() {
        let celsius = celsius(source.temperature());
        let _ = writeln!(
            output,
            "  {}. {} mL available | {} C",
            index + 1,
            milliliters(source.available_volume()),
            celsius,
        );
    }
    Some(output)
}

/// Only the co-located natural pool is assessed. This is a conditional
/// planning aid, not a promise of future rainfall or automatic absorption.
pub(super) fn natural_spring_report(registries: &Registries, state: &AppState) -> String {
    let Some(surface) = assess_player_local_natural_spring(state) else {
        return String::new();
    };
    spring_supply_label(
        surface,
        registries.survival().physiology().hydration_loss_per_tick(),
    )
}

fn spring_supply_label(surface: Volume, resting_hydration_loss_per_tick: Volume) -> String {
    if surface.is_zero() {
        return "Dry natural spring basin here: no surface water to drink. Moisture retained in its soil may return with weather, but a completely spent spring cannot replenish itself. Seek another source.\n".to_owned();
    }
    let rate = resting_hydration_loss_per_tick.microliters();
    if rate == 0 {
        return format!(
            "Natural spring here: {} mL of finite surface water.\n",
            milliliters(surface)
        );
    }
    format!(
        "Natural spring supply: {} mL, equivalent to {} of hydration loss at rest. This is finite water, not a daily yield; work uses more and soil-weather exchange can change the visible amount.\n",
        milliliters(surface),
        runway_label(Some(surface.microliters() / rate)),
    )
}

/// Exact-local custody remains inspectable even when darkness conceals the sightline.
pub(super) fn loose_drop_report(
    registries: &Registries,
    state: &AppState,
    output: &mut String,
) -> bool {
    let mut found = false;
    for drop in available_player_world_drops(registries, state) {
        if !found {
            output.push_str("Loose drops at your position:\n");
            found = true;
        }
        let pickup = if drop.requires_deliberate_pickup() {
            "load with gather"
        } else if drop.automatic_pickup_preflight_ready() {
            "automatic pickup currently eligible"
        } else if !drop.automatic_pickup_ready() {
            "self-pickup delay active"
        } else {
            "automatic pickup currently blocked"
        };
        let _ = writeln!(
            output,
            "  {} (loose drop) here at {} | {} g | {} C | {pickup}",
            name(registries, drop.commodity()),
            super::pos(drop.position()),
            grams(drop.mass()),
            celsius(drop.temperature()),
        );
    }
    found
}

pub fn gather_options(
    registries: &Registries,
    state: &AppState,
    options: &[PlayerSurfaceGatheringOption],
) -> String {
    let mut output = String::new();
    if options.is_empty() {
        return "No gatherable source at your current location.\n".to_owned();
    }
    output.push_str("Gather here (gather N, optional grams to 0.001 g):\n");
    for (index, option) in options.iter().copied().enumerate() {
        let batch = DefaultGatheringBatch::assess(registries, state, option);
        let mass = batch.request_mass();
        let duration = option
            .duration_for_mass(mass)
            .map(|span| activity::gathering_duration(span.value()))
            .unwrap_or_else(|| "unavailable".to_owned());
        let _ = writeln!(
            output,
            "  {}. {} | available {} g | batch limit {} g | default {} g | work {} for default{}",
            index + 1,
            name(registries, option.commodity()),
            grams(option.source_mass()),
            grams(option.maximum_mass_at(state)),
            grams(mass),
            duration,
            if option.hand_handling_blocker().is_some() {
                " | too hot for bare hands"
            } else if mass.is_zero() {
                " | no carrying space"
            } else {
                ""
            }
        );
        if let Ok(Some(food)) = option.food_value(registries, state) {
            let freshness = freshness_label(food.current_freshness());
            let _ = writeln!(
                output,
                "     Food: {:?}, {freshness}; {} gross dietary energy per {} g",
                food.category(),
                fire::energy_label(food.dietary_energy()),
                grams(food.batch_mass()),
            );
        }
        if mass < batch.suggested {
            let _ = writeln!(
                output,
                "     Default reduced from {} g to fit current work reserves (revalidated on use).",
                grams(batch.suggested)
            );
        } else if !mass.is_zero() {
            let (admission, no_affordable_portion) = match batch.affordable {
                Ok(affordable) if affordable.is_zero() => (
                    assess_player_surface_gathering_option_admission(
                        registries, state, option, mass,
                    ),
                    true,
                ),
                Ok(_) => (Ok(()), false),
                Err(error) => (Err(error), false),
            };
            if let Err(SurfaceGatheringError::PlayerWork(error)) = admission {
                let _ = writeln!(
                    output,
                    "     Default batch currently blocked: {}",
                    errors::work_blocker(&error),
                );
                if no_affordable_portion {
                    output.push_str(
                        "     No positive portion is affordable with current reserves.\n",
                    );
                }
            }
        }
    }
    output
}

#[cfg(test)]
#[path = "provisions_tests.rs"]
mod tests;
