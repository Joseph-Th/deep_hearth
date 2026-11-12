//! Visible, renderer-neutral readout only. Never inspect undiscovered world resources.

use std::fmt::Write as _;

use deep_hearth::kobolds::{
    available_nearby_kobold_footfalls, available_nearby_kobold_scouts,
    available_nearby_kobold_tracks,
};
use deep_hearth::logistics::project_player_inventory;
use deep_hearth::spatial::ViewDirection;
use deep_hearth::surface::{
    AdjacentSurfaceResource, PlayerSurfaceGatheringOption, player_surface_gathering_options,
};
use deep_hearth::survival::{
    Vitality, assess_player_resting_provision_runway, assess_player_unheated_comfort_runway,
};
use deep_hearth::world::{
    assess_player_forage_scents, assess_player_weather, available_visible_generated_water,
    raycast_player_block,
};
use deep_hearth::{AppState, Registries};

fn name<'a>(registries: &'a Registries, commodity: deep_hearth::material::CommodityKey) -> &'a str {
    registries
        .materials()
        .commodity_name(commodity)
        .unwrap_or("unidentified material")
}

fn pos(position: deep_hearth::spatial::VoxelCoord) -> String {
    format!("({},{},{})", position.x(), position.y(), position.z())
}

/// One player-visible environment signature, sampled only at consequential slow boundaries.
/// Do not inspect private geology, colony schedules, or all generated blocks to stop a wait.
pub fn local_cues(registries: &Registries, state: &AppState) -> String {
    format!(
        "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
        AdjacentSurfaceResource::visible_in_generated_world(registries, state).collect::<Vec<_>>(),
        available_visible_generated_water(registries, state),
        available_nearby_kobold_scouts(registries, state).collect::<Vec<_>>(),
        available_nearby_kobold_footfalls(registries, state),
        available_nearby_kobold_tracks(registries, state).collect::<Vec<_>>(),
        assess_player_forage_scents(registries, state),
        assess_player_weather(registries, state).map(|weather| (
            weather.season(),
            weather.phase(),
            weather.wind(),
            weather.wind_direction(),
        )),
    )
}

/// Threshold transitions, not tiny per-tick resource changes, interrupt long waits.
pub fn urgency(registries: &Registries, state: &AppState) -> (bool, bool, bool, bool) {
    let Some(player) = state.survival().player() else {
        return (true, true, true, true);
    };
    let physiology = registries.survival().physiology();
    (
        player.metabolic_energy() < physiology.hungry_below(),
        player.hydration() < physiology.thirsty_below(),
        player.vitality() < Vitality::MAXIMUM,
        player.thermal_comfort_ppm() < 500_000,
    )
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
    if let Some(survival) = state.survival().player() {
        let physiology = registries.survival().physiology();
        let energy = survival.metabolic_energy().nanojoules() * 100
            / physiology.maximum_metabolic_energy().nanojoules();
        let water = u128::from(survival.hydration().microliters()) * 100
            / u128::from(physiology.maximum_hydration().microliters());
        let _ = writeln!(
            output,
            "Condition: vitality {}%, food reserve {}%, hydration {}%, warmth {}%",
            survival.vitality().parts_per_million() / 10_000,
            energy,
            water,
            survival.thermal_comfort_ppm() / 10_000,
        );
        if survival.vitality() == Vitality::ZERO {
            output.push_str("You are dead. Direct actions require respawn, which is not wired to this client.\n");
        } else {
            let (hungry, thirsty, injured, cold) = urgency(registries, state);
            if hungry || thirsty || injured || cold {
                let _ = writeln!(
                    output,
                    "Warnings: hunger={hungry}, thirst={thirsty}, injury={injured}, cold={cold}",
                );
            }
        }
    }
    if let Some(weather) = assess_player_weather(registries, state) {
        let _ = writeln!(
            output,
            "Weather: {:?} {:?}, {} mK, {:?} wind (toward {:?})",
            weather.season(),
            weather.phase(),
            weather.ambient_temperature().millikelvin(),
            weather.wind(),
            weather.wind_direction(),
        );
    }
    if let Some(work) = state.player_work().active() {
        let _ = writeln!(output, "Ongoing work: {work:?}");
    }
    output
}

pub fn look(registries: &Registries, state: &AppState, aim: Option<ViewDirection>) -> String {
    let mut output = status(registries, state);
    let nearby =
        AdjacentSurfaceResource::visible_in_generated_world(registries, state).collect::<Vec<_>>();
    if nearby.is_empty() {
        output.push_str("No loose resources in immediate sight.\n");
    } else {
        output.push_str("Visible loose resources (exact quantities require standing at them):\n");
        for source in nearby {
            let _ = writeln!(
                output,
                "  {} at {}",
                name(registries, source.commodity()),
                pos(source.position()),
            );
        }
    }
    for water in available_visible_generated_water(registries, state) {
        let _ = writeln!(output, "Visible spring water at {}", pos(water.position()));
    }
    for sighting in available_nearby_kobold_scouts(registries, state) {
        let _ = writeln!(
            output,
            "Visible {:?} at {}, stature {:?}, wary {}, defending {}",
            sighting.role(),
            pos(sighting.position()),
            sighting.stature(),
            sighting.is_wary(),
            sighting.is_defending(),
        );
    }
    for scent in assess_player_forage_scents(registries, state) {
        let _ = writeln!(
            output,
            "Scent: {:?} {:?} (no exact location)",
            scent.intensity(),
            scent.kind(),
        );
    }
    for footfall in available_nearby_kobold_footfalls(registries, state) {
        let _ = writeln!(
            output,
            "Heard: {:?} {:?}",
            footfall.kind(),
            footfall.bearing()
        );
    }
    for track in available_nearby_kobold_tracks(registries, state) {
        let _ = writeln!(output, "Track: {track:?}");
    }
    if let Some(aim) = aim {
        match raycast_player_block(registries, state, aim) {
            Ok(Some(target)) => {
                let block_name = registries
                    .world()
                    .block_for_state(target.state())
                    .map_or("unknown block", |block| block.name());
                let _ = writeln!(
                    output,
                    "Pointed at {block_name} at {}, face {:?}",
                    pos(target.voxel()),
                    target.face()
                );
            }
            Ok(None) => output.push_str("No targetable block within reach in that direction.\n"),
            Err(error) => {
                let _ = writeln!(output, "Pointing unavailable: {error}");
            }
        }
    }
    if let Ok(options) = player_surface_gathering_options(registries, state)
        && !options.is_empty()
    {
        output.push_str(&gather_options(registries, state, &options));
    }
    if let Some(runway) = assess_player_resting_provision_runway(registries, state) {
        let _ = writeln!(
            output,
            "At rest, hunger warning in {:?} slow ticks, thirst warning in {:?} slow ticks",
            runway.hungry_in().map(|time| time.value()),
            runway.thirsty_in().map(|time| time.value())
        );
    }
    if let Some(runway) = assess_player_unheated_comfort_runway(registries, state) {
        let _ = writeln!(
            output,
            "Without heat, current warmth runway: {:?} slow ticks; cover: {:?}",
            runway.current().map(|span| span.value()),
            runway.overhead_cover()
        );
    }
    output
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
    output.push_str("Gather here (gather N, optional whole grams):\n");
    for (index, option) in options.iter().copied().enumerate() {
        let mass = option
            .source_mass()
            .min(option.maximum_batch_mass())
            .min(option.direct_batch_mass());
        let duration = option.duration_for_mass(mass).map(|span| span.value());
        let _ = writeln!(
            output,
            "  {}. {} | available {} g | default {} g | work {:?} slow ticks{}",
            index + 1,
            name(registries, option.commodity()),
            option.source_mass().milligrams() / 1_000,
            mass.milligrams() / 1_000,
            duration,
            match option.hand_handling_blocker() {
                Some(_) => " | too hot for bare hands",
                None => "",
            }
        );
        if let Ok(Some(food)) = option.food_value(registries, state) {
            let _ = writeln!(
                output,
                "     Food: {:?}, freshness {:?}",
                food.category(),
                food.current_freshness(),
            );
        }
    }
    output
}

pub fn inventory(registries: &Registries, state: &AppState) -> Result<String, String> {
    let projection =
        project_player_inventory(registries, state).map_err(|error| error.to_string())?;
    let mut output = String::new();
    let carried = projection.carried();
    let _ = writeln!(
        output,
        "Carried matter: {} g / {} g",
        carried.stored().milligrams() / 1_000,
        carried.capacity().milligrams() / 1_000,
    );
    if carried.stacks().is_empty() {
        output.push_str("  (empty)\n");
    }
    for (index, stack) in carried.stacks().iter().enumerate() {
        let _ = writeln!(
            output,
            "  {}. {} | {} g | {} mK",
            index + 1,
            name(registries, stack.commodity()),
            stack.mass().milligrams() / 1_000,
            stack.temperature().millikelvin(),
        );
    }
    for equipment in projection.equipment() {
        let _ = writeln!(
            output,
            "Carried equipment: definition {} | equipment {} | {} g",
            equipment.definition().value(),
            equipment.equipment().value(),
            equipment.mass().milligrams() / 1_000,
        );
    }
    let _ = writeln!(
        output,
        "Active quick slot: {}",
        projection.active_quick_access_slot().index() + 1,
    );
    for (index, selection) in projection.quick_access_slots().iter().enumerate() {
        let _ = writeln!(output, "  slot {}: {selection:?}", index + 1);
    }
    for local in projection.local_containers() {
        let _ = writeln!(
            output,
            "Local storage: {} g / {} g",
            local.stored().milligrams() / 1_000,
            local.capacity().milligrams() / 1_000,
        );
    }
    Ok(output)
}
