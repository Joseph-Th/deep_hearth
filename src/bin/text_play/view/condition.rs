//! Actor condition, motion, local warmth and work status.

use std::fmt::Write as _;

use deep_hearth::energy::project_player_nearby_warmth_sources;
use deep_hearth::logistics::{
    PlayerActiveSelection, player_body_has_fixed_floor, player_body_is_grounded,
};
use deep_hearth::spatial::{HorizontalVelocity, VerticalVelocity};
use deep_hearth::survival::{
    HungerState, HydrationState, SurvivalAssessment, ThermalComfortState, Vitality, assess_survival,
};
use deep_hearth::world::{HabitatKind, assess_player_habitat, assess_player_weather};
use deep_hearth::{AppState, Registries};

use super::{actor_inventory::hand_status, pos, runway_label, sensory};
use crate::text_play::{activity, fire};

fn meters_from_nanometers(nanometers: i64) -> String {
    let absolute = i128::from(nanometers).abs();
    format!(
        "{}{}.{:03}",
        if nanometers < 0 { "-" } else { "" },
        absolute / 1_000_000_000,
        (absolute % 1_000_000_000) / 1_000_000,
    )
}

/// Never render a living player as zero vitality. Preserve ordinary
/// whole-percent labels outside the final one percent.
fn vitality_label(ppm: u32) -> String {
    match ppm {
        0 => "0".to_owned(),
        1..=99 => "<0.01".to_owned(),
        100..=9_999 => format!("0.{:02}", ppm / 100),
        _ => (ppm / 10_000).to_string(),
    }
}

fn reserve_label(percent: u8, exhausted: bool) -> String {
    if percent == 0 && !exhausted {
        "<1".to_owned()
    } else {
        percent.to_string()
    }
}

fn survival_status(registries: &Registries, state: &AppState) -> String {
    let mut output = String::new();
    if let Some(survival) = assess_survival(registries, state) {
        let (food_percent, water_percent) = survival.reserve_percentages(registries);
        let _ = writeln!(
            output,
            "Condition: vitality {}%, food reserve {}%, hydration {}%, warmth {}%",
            vitality_label(survival.vitality().parts_per_million()),
            reserve_label(food_percent, survival.metabolic_energy().is_zero()),
            reserve_label(water_percent, survival.hydration().is_zero()),
            reserve_label(
                (survival.thermal_comfort_ppm() / 10_000) as u8,
                survival.thermal_comfort_ppm() == 0,
            ),
        );
        if survival.vitality() == Vitality::ZERO {
            output.push_str(
                "You are dead. Use respawn to return; your old pack remains at the death cache.\n",
            );
        } else {
            let hungry = survival.hunger() != HungerState::Fed;
            let thirsty = survival.hydration_state() != HydrationState::Hydrated;
            let injured = survival.vitality() < Vitality::MAXIMUM;
            let cold = survival.thermal_comfort_state() != ThermalComfortState::Comfortable;
            let warnings = [
                (hungry, "hunger"),
                (thirsty, "thirst"),
                (injured, "injury"),
                (cold, "cold exposure"),
            ]
            .into_iter()
            .filter_map(|(active, label)| active.then_some(label))
            .collect::<Vec<_>>();
            if !warnings.is_empty() {
                let _ = writeln!(output, "Warnings: {}", warnings.join(", "));
            }
            if survival.hunger() == HungerState::Starving {
                output.push_str("Starvation: metabolic reserve exhausted. Uncovered metabolic costs cause vitality loss. Eat carried food or gather a small fresh edible portion at risk to vitality; other paid work may be impossible.\n");
            }
            if survival.hydration_state() == HydrationState::Dehydrated {
                output.push_str("Dehydration: water reserve exhausted. Uncovered hydration costs cause vitality loss. Drink local water or forage fresh edible food at risk to vitality.\n");
            }
            if injured {
                injury_recovery_report(registries, survival, &mut output);
            }
        }
    }
    output
}

fn injury_recovery_report(
    registries: &Registries,
    survival: SurvivalAssessment,
    output: &mut String,
) {
    let ppm = survival.vitality().parts_per_million();
    if ppm < 100 {
        output.push_str("Vitality: <0.01%\n");
    } else {
        let _ = writeln!(
            output,
            "Vitality: {}.{:02}%",
            ppm / 10_000,
            (ppm % 10_000) / 100
        );
    }
    if survival.hunger() != HungerState::Fed
        || survival.hydration_state() != HydrationState::Hydrated
        || survival.thermal_comfort_state() != ThermalComfortState::Comfortable
    {
        output.push_str("Injury recovery paused: maintain food, water and warmth above their warning thresholds.\n");
    } else if let Some(duration) = survival.recovery_duration_at_current_diet(registries) {
        let _ = writeln!(
            output,
            "Injury recovery: {} to full vitality at unchanged diet quality, if fed, hydrated and warm without further injury.",
            runway_label(Some(duration.value())),
        );
    } else {
        output.push_str("Injury recovery paused: replenish dietary reserves with varied food.\n");
    }
}

pub fn status(registries: &Registries, state: &AppState) -> String {
    let mut output = String::new();
    let Some(player) = state.logistics().player() else {
        return "No player admitted.\n".to_owned();
    };
    let _ = writeln!(
        output,
        "At {} | control {} | simulation {}",
        pos(player.position()),
        state.control_tick().value(),
        state.tick().value(),
    );
    let _ = writeln!(
        output,
        "Body: {:?} | foot elevation {} m",
        player.body().stance(),
        meters_from_nanometers(player.body().foot_position().y_nanometers()),
    );
    let foot = player.body().foot_position();
    let _ = writeln!(
        output,
        "Within cell: x {} m, z {} m (centre 0.500, 0.500)",
        meters_from_nanometers(foot.x_nanometers().rem_euclid(1_000_000_000)),
        meters_from_nanometers(foot.z_nanometers().rem_euclid(1_000_000_000))
    );
    output.push_str(&survival_status(registries, state));
    let active = state.logistics().active_selection();
    let (hand, material_carried) = hand_status(registries, state, active);
    let _ = writeln!(
        output,
        "Hand: {} (slot {})",
        hand,
        state.logistics().active_quick_access_slot().index() + 1,
    );
    if let Some(PlayerActiveSelection::Material(commodity)) = active
        && registries.survival().get_food(commodity).is_some()
        && material_carried
    {
        output.push_str(
            "Selected food: use eats it; eat N chooses a physical stack; drink N selects local water without changing the hand.\n",
        );
    }
    if !player_body_is_grounded(registries, state) {
        output.push_str("Airborne: gravity is active; wait until landing before hand work.\n");
    } else if !player_body_has_fixed_floor(registries, state) {
        output.push_str("Unstable footing: move onto fixed ground before hand work.\n");
    } else if player.body().horizontal_velocity() != HorizontalVelocity::ZERO
        || player.body().vertical_velocity() != VerticalVelocity::ZERO
    {
        output.push_str("Still moving: wait 10 to let momentum settle before hand work.\n");
    }
    if let Some(weather) = assess_player_weather(registries, state) {
        let _ = writeln!(output, "Weather: {}", sensory::weather(weather));
    }
    if let Some(habitat) = assess_player_habitat(state) {
        let name = match habitat {
            HabitatKind::DampGrove => "damp grove",
            HabitatKind::MixedWoodland => "mixed woodland",
            HabitatKind::DryScrub => "dry scrub",
        };
        let _ = writeln!(output, "Habitat: {name}");
    }
    for source in project_player_nearby_warmth_sources(registries, state) {
        let definition = registries
            .energy()
            .get_store(source.definition())
            .unwrap_or_else(|| {
                unreachable!("energy owner projected a missing warmth source definition")
            });
        if source.position() == player.position() {
            let _ = writeln!(
                output,
                "Local warmth: {} | {} stored | {}",
                definition.name(),
                fire::energy_label(source.stored()),
                fire::burn_runway_label(registries, source.definition(), source.stored()),
            );
        } else {
            let _ = writeln!(
                output,
                "Adjacent warmth: {} at {} | {} stored | {}",
                definition.name(),
                pos(source.position()),
                fire::energy_label(source.stored()),
                fire::burn_runway_label(registries, source.definition(), source.stored()),
            );
        }
    }
    if let Some(work) = state.player_work().active() {
        let _ = writeln!(
            output,
            "Work: {}",
            activity::remaining_duration(registries, state, work),
        );
    } else if state.logistics().has_pending_direct_action() {
        output.push_str("Queued interaction: 'wait finish' admits this action at the next simulation boundary and continues through scheduled work unless interrupted; 'wait work' stops at admission.\n");
    }
    output.push_str(&activity::local_production_report(registries, state));
    output
}

#[cfg(test)]
#[path = "condition_tests.rs"]
mod tests;
