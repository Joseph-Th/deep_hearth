//! Text-play intent routing. All consequences come from public production operations.

mod activity;
mod command;
mod crafting;
mod equipment;
mod errors;
mod fire;
mod geology;
mod ore;
mod quantity;
mod save_file;
mod stepping;
mod storage;
mod view;

#[cfg(test)]
mod geology_tests;
#[cfg(test)]
mod tests;

use std::fmt::Write as _;

use deep_hearth::content::BLOCK_STATE_STONE;
use deep_hearth::labor::PlayerWork;
use deep_hearth::logistics::{
    PlayerActiveSelection, project_player_inventory, validate_set_active_selection,
};
use deep_hearth::persistence::{LoadedSaveEnvelope, SaveEnvelope, validate_persistable_state};
use deep_hearth::player::{validate_admit_player, validate_respawn_player};
use deep_hearth::simulation::{PlayerDirectControlInput, SecondaryControlIntent};
use deep_hearth::spatial::VoxelCoord;
use deep_hearth::surface::player_surface_gathering_options;
use deep_hearth::world::{
    BlockStateId, ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, WorldGenerationSeed,
    validate_generate_chunk, validate_generate_player_neighborhood,
};
use deep_hearth::world_generation::validate_generate_initial_wilderness;
use deep_hearth::{AppState, Registries, build_registries};

use command::parse_index;
use quantity::parse_decimal_grams;
use save_file::{publish_new_save, slot_path};

pub struct Session {
    registries: Registries,
    state: AppState,
    seed: Option<u64>,
}

impl Session {
    pub fn new(seed: u64) -> Result<Self, String> {
        let registries = build_registries();
        let mut state = AppState::new();
        validate_generate_chunk(
            &registries,
            &state,
            WorldGenerationSeed::new(seed),
            ChunkCoord::new(0, 0),
            BLOCK_STATE_STONE,
        )
        .map_err(|error| format!("opening terrain generation failed: {error}"))?
        .commit(&mut state)
        .map_err(|error| format!("opening terrain commit failed: {error}"))?;

        // Candidate selection is startup setup, not a player observation. The opening
        // generator and player admission each validate the same real terrain clearance.
        // Start near the chunk center to leave room for the authored finite trail.
        let candidates = std::iter::once((8_i64, 8_i64))
            .chain((0..16_i64).flat_map(|x| (0..16_i64).map(move |z| (x, z))));
        let mut selected = None;
        'position: for (x, z) in candidates {
            for y in (WORLD_MIN_Y + 1..WORLD_MAX_Y_EXCLUSIVE - 2).rev() {
                let foot = VoxelCoord::new(x, y, z);
                if state.world().block_at(foot) != Some(BlockStateId::AIR)
                    || state.world().block_at(VoxelCoord::new(x, y + 1, z))
                        != Some(BlockStateId::AIR)
                    || state.world().block_at(VoxelCoord::new(x, y - 1, z))
                        != Some(BLOCK_STATE_STONE)
                {
                    continue;
                }
                if validate_admit_player(&registries, &state, foot).is_err() {
                    continue;
                }
                if let Ok(opening) = validate_generate_initial_wilderness(
                    &registries,
                    &state,
                    WorldGenerationSeed::new(seed),
                    foot,
                ) {
                    selected = Some((foot, opening));
                    break 'position;
                }
            }
        }
        let (spawn, opening) = selected
            .ok_or("generated world contains no legal wilderness trail and starting cell")?;
        opening
            .commit(&mut state)
            .map_err(|error| format!("opening wilderness commit failed: {error}"))?;
        validate_admit_player(&registries, &state, spawn)
            .map_err(|error| format!("opening player admission failed: {error}"))?
            .commit(&mut state)
            .map_err(|error| format!("opening player admission commit failed: {error}"))?;
        Self::expand_neighborhood(&registries, &mut state)?;
        Ok(Self {
            registries,
            state,
            seed: Some(seed),
        })
    }

    fn expand_neighborhood(registries: &Registries, state: &mut AppState) -> Result<(), String> {
        if let Some(demand) = validate_generate_player_neighborhood(registries, state)
            .map_err(|error| format!("world expansion failed: {error}"))?
        {
            demand
                .commit(state)
                .map_err(|error| format!("world expansion commit failed: {error}"))?;
        }
        Ok(())
    }

    pub fn load(slot: &str) -> Result<Self, String> {
        let path = slot_path(slot)?;
        let bytes = std::fs::read(&path)
            .map_err(|error| format!("cannot load {}: {error}", path.display()))?;
        let envelope: LoadedSaveEnvelope = serde_json::from_slice(&bytes)
            .map_err(|error| format!("save decoding failed: {error}"))?;
        let registries = build_registries();
        let state = envelope
            .into_state(&registries)
            .map_err(|error| format!("save rejected: {error}"))?;
        if state.logistics().player().is_none() {
            return Err("save has no playable character".to_owned());
        }
        Ok(Self {
            registries,
            state,
            seed: None,
        })
    }

    fn save(&self, slot: &str) -> Result<(), String> {
        let destination = slot_path(slot)?;
        validate_persistable_state(&self.registries, &self.state)
            .map_err(|error| format!("cannot save invalid state: {error}"))?;
        let bytes = serde_json::to_vec(&SaveEnvelope::new(&self.registries, &self.state))
            .map_err(|error| format!("save encoding failed: {error}"))?;
        publish_new_save(&destination, &bytes)
    }

    pub fn opening(&self) -> String {
        let mut output = String::new();
        match self.seed {
            Some(seed) => {
                let _ = writeln!(output, "Deep Hearth wilderness | seed {seed}");
            }
            None => output.push_str("Deep Hearth wilderness | loaded save\n"),
        }
        output.push_str(&view::look(&self.registries, &self.state, None));
        output
    }

    fn respawn(&mut self) -> Result<String, String> {
        let outcome = validate_respawn_player(&self.registries, &self.state)
            .map_err(|error| error.to_string())?
            .commit(&mut self.state)
            .map_err(|error| error.to_string())?;
        let spawn = outcome.position();
        let cache = outcome.death_cache_position();
        let mut output = format!(
            "Respawned at ({},{},{}){}; previous possessions remain in a death cache at ({},{},{}).\n",
            spawn.x(),
            spawn.y(),
            spawn.z(),
            if outcome.used_spawn_anchor() {
                " (original spawn)"
            } else {
                " (safe fallback)"
            },
            cache.x(),
            cache.y(),
            cache.z(),
        );
        if !outcome.dropped_equipment().is_empty() {
            let _ = writeln!(
                output,
                "{} carried tools were dropped with the pack.",
                outcome.dropped_equipment().len()
            );
        }
        output.push_str(&view::look(&self.registries, &self.state, None));
        Ok(output)
    }

    fn set_selection(
        &mut self,
        selection: Option<PlayerActiveSelection>,
    ) -> Result<String, String> {
        validate_set_active_selection(&self.registries, &self.state, selection)
            .map_err(|error| error.to_string())?
            .commit(&mut self.state)
            .map_err(|error| error.to_string())?;
        Ok(format!(
            "Active hand: {}.\n",
            view::hand_label(&self.registries, &self.state, selection)
        ))
    }

    fn select(&mut self, text: &str) -> Result<String, String> {
        let inventory = project_player_inventory(&self.registries, &self.state)
            .map_err(|error| error.to_string())?;
        let index = parse_index(text, inventory.carried().stacks().len())?;
        let commodity = inventory.carried().stacks()[index - 1].commodity();
        self.set_selection(Some(PlayerActiveSelection::material(commodity)))
    }

    fn gather(&mut self, args: &[&str]) -> Result<String, String> {
        let options = player_surface_gathering_options(&self.registries, &self.state)
            .map_err(|error| error.to_string())?;
        if args.is_empty() {
            return Ok(view::gather_options(
                &self.registries,
                &self.state,
                &options,
            ));
        }
        let index = parse_index(args[0], options.len())?;
        let option = options[index - 1];
        let mass = if let Some(grams) = args.get(1) {
            parse_decimal_grams(grams, "gather quantity")?
        } else {
            view::default_gather_mass(&self.state, option)
        };
        if args.len() == 1 && mass.is_zero() {
            return Err("no carrying space for the suggested gathering batch".to_owned());
        }
        if mass.is_zero() || mass > option.source_mass() || mass > option.maximum_batch_mass() {
            return Err(
                "gather quantity must be positive and within the displayed source/method limit"
                    .to_owned(),
            );
        }
        self.advance_one(
            PlayerDirectControlInput::NEUTRAL
                .with_secondary(SecondaryControlIntent::gather_surface_option(option, mass)),
        )
    }

    /// Repeat exact-local hand gathering without skipping any authoritative work
    /// or retargeting an indexed choice when a source is exhausted. A visible
    /// interruption leaves the active job for the player to resume explicitly.
    fn gather_repeated(&mut self, args: &[&str]) -> Result<String, String> {
        let (index_text, mass_text, repetitions_text) = match args {
            [index, "repeat", count] => (*index, None, *count),
            [index, mass, "repeat", count] => (*index, Some(*mass), *count),
            _ => return Err("use gather N [grams] repeat COUNT".to_owned()),
        };
        let count = repetitions_text
            .parse::<usize>()
            .map_err(|_| "repeat count must be a whole number")?;
        if !(1..=32).contains(&count) {
            return Err("repeat count must be between 1 and 32".to_owned());
        }
        let requested_mass = mass_text
            .map(|grams| parse_decimal_grams(grams, "gather quantity"))
            .transpose()?;
        if requested_mass.is_some_and(|mass| mass.is_zero()) {
            return Err("gather quantity must be positive".to_owned());
        }
        if self.state.player_work().active().is_some() {
            return Err("finish the current work before starting repeated gathering".to_owned());
        }
        let options = player_surface_gathering_options(&self.registries, &self.state)
            .map_err(|error| error.to_string())?;
        let index = parse_index(index_text, options.len())?;
        let original = options[index - 1];
        let mut output = String::new();
        let mut completed = 0;
        for _ in 0..count {
            let current = player_surface_gathering_options(&self.registries, &self.state)
                .map_err(|error| error.to_string())?;
            let mut same_source = current.into_iter().filter(|option| {
                option.resource() == original.resource()
                    && option.commodity() == original.commodity()
                    && option.maximum_batch_mass() == original.maximum_batch_mass()
                    && option.direct_batch_mass() == original.direct_batch_mass()
                    && option.exertion() == original.exertion()
            });
            let Some(option) = same_source.next() else {
                let _ = writeln!(
                    output,
                    "Gather repeat: source depleted after {completed} batches."
                );
                break;
            };
            if same_source.next().is_some() {
                let _ = writeln!(
                    output,
                    "Gather repeat: method became ambiguous after {completed} batches."
                );
                break;
            }
            let mass =
                requested_mass.unwrap_or_else(|| view::default_gather_mass(&self.state, option));
            if mass.is_zero()
                || mass > option.source_mass()
                || mass > option.maximum_batch_mass()
                || mass > option.direct_batch_mass()
                || mass > view::default_gather_mass(&self.state, option)
            {
                if completed == 0 {
                    return Err(
                        "gather quantity exceeds the available source, method, or carrying space"
                            .to_owned(),
                    );
                }
                let _ = writeln!(
                    output,
                    "Gather repeat: insufficient remaining source or carrying space after {completed} batches."
                );
                break;
            }
            let _ = self.step(
                PlayerDirectControlInput::NEUTRAL
                    .with_secondary(SecondaryControlIntent::gather_surface_option(option, mass)),
                &mut output,
            )?;
            if !matches!(
                self.state.player_work().active(),
                Some(PlayerWork::SurfaceGathering { .. })
            ) {
                let _ = writeln!(
                    output,
                    "Gather repeat: admission rejected after {completed} completed batches."
                );
                break;
            }
            let wait = activity::remaining_work_control_ticks(&self.state)
                .ok_or("gathering started without a scheduled completion")?
                .min(7_200);
            output.push_str(&self.advance_many_compact(
                PlayerDirectControlInput::NEUTRAL,
                wait,
                true,
            )?);
            if self.state.player_work().active().is_some() {
                let _ = writeln!(
                    output,
                    "Gather repeat: interrupted after {completed} completed batches; finish the active work before repeating."
                );
                break;
            }
            completed += 1;
        }
        let _ = writeln!(output, "Gather repeat: {completed} batches completed.");
        output.push_str(&view::look(&self.registries, &self.state, None));
        Ok(output)
    }

    fn eat(&mut self, index: &str) -> Result<String, String> {
        let inventory = project_player_inventory(&self.registries, &self.state)
            .map_err(|error| error.to_string())?;
        let stacks = inventory.carried().stacks();
        let index = parse_index(index, stacks.len())?;
        let choice = stacks[index - 1].choice();
        self.advance_one(
            PlayerDirectControlInput::NEUTRAL
                .with_secondary(SecondaryControlIntent::eat_stack(choice)),
        )
    }

    fn drink(&mut self, index: &str) -> Result<String, String> {
        let sources =
            deep_hearth::survival::available_local_drink_sources(&self.registries, &self.state)
                .collect::<Vec<_>>();
        let index = parse_index(index, sources.len())?;
        self.advance_one(PlayerDirectControlInput::NEUTRAL.with_secondary(
            SecondaryControlIntent::drink_source(sources[index - 1].store()),
        ))
    }
}
