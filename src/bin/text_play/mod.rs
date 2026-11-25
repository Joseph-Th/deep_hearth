//! Text-play intent routing. All consequences come from public production operations.

mod activity;
mod command;
mod consumption;
mod crafting;
mod equipment;
mod errors;
mod fire;
mod gathering;
mod geology;
mod ore;
mod quantity;
mod save_file;
mod stepping;
mod storage;
mod thermal;
mod view;

#[cfg(test)]
mod geology_tests;
#[cfg(test)]
mod tests;

use std::fmt::Write as _;

use deep_hearth::content::BLOCK_STATE_STONE;
use deep_hearth::logistics::{
    PlayerActiveSelection, project_player_inventory, validate_set_active_selection,
};
use deep_hearth::persistence::{LoadedSaveEnvelope, SaveEnvelope, validate_persistable_state};
use deep_hearth::player::{validate_admit_player, validate_respawn_player};
use deep_hearth::spatial::VoxelCoord;
use deep_hearth::world::{
    BlockStateId, ChunkCoord, HabitatKind, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, WorldGenerationSeed,
    select_natural_world_spawn, validate_generate_chunk, validate_generate_natural_chunk,
    validate_generate_player_neighborhood,
};
use deep_hearth::world_generation::validate_generate_initial_wilderness;
use deep_hearth::{AppState, Registries, build_registries};

use command::parse_index;
use save_file::{publish_new_save, slot_path};

pub struct Session {
    registries: Registries,
    state: AppState,
    seed: Option<u64>,
}

impl Session {
    /// Begin in unmodified seeded terrain, rather than the controlled tutorial
    /// clearing. Optional habitat selection searches *the same world seed* for
    /// a real standing site without creating or granting local resources.
    pub fn new_natural(seed: u64) -> Result<Self, String> {
        Self::new_natural_in_habitat(seed, None)
    }

    pub fn new_natural_in_habitat(seed: u64, desired: Option<HabitatKind>) -> Result<Self, String> {
        let registries = build_registries();
        // The seed selects a different district for each attempt. Habitat
        // targeting is a world-creation choice, not a player-visible oracle.
        // Only the accepted district enters the persisted world; later chunks
        // use exactly the same seed without any special biome overrides.
        for attempt in 0..64_u64 {
            let site_salt = attempt.wrapping_mul(0xD1B5_4A32_D192_ED03);
            let x_hash = natural_site_hash(seed ^ 0x4E41_5455_5241_4C58 ^ site_salt);
            let z_hash = natural_site_hash(seed ^ 0x4E41_5455_5241_4C5A ^ site_salt);
            let chunk = ChunkCoord::new((x_hash % 49) as i64 - 24, (z_hash % 49) as i64 - 24);
            let mut state = AppState::new();
            validate_generate_natural_chunk(
                &registries,
                &state,
                WorldGenerationSeed::new(seed),
                chunk,
                BLOCK_STATE_STONE,
            )
            .map_err(|error| format!("natural terrain generation failed: {error}"))?
            .commit(&mut state)
            .map_err(|error| format!("natural terrain commit failed: {error}"))?;
            let position_hash = natural_site_hash(seed ^ 0x4E41_5455_5241_4C50 ^ site_salt);
            let spawn =
                select_natural_world_spawn(&registries, &state, chunk, position_hash, desired);
            let Some(spawn) = spawn else { continue };
            validate_admit_player(&registries, &state, spawn)
                .map_err(|error| format!("natural player admission failed: {error}"))?
                .commit(&mut state)
                .map_err(|error| format!("natural player admission commit failed: {error}"))?;
            Self::expand_neighborhood(&registries, &mut state)?;
            return Ok(Self {
                registries,
                state,
                seed: Some(seed),
            });
        }
        Err("no safely occupiable natural terrain in 64 seeded districts matching the requested habitat".to_owned())
    }

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

    fn mine_repeated(&mut self, args: &[&str], count: &str) -> Result<String, String> {
        let count = count
            .parse::<usize>()
            .map_err(|_| "repeat count must be a whole number")?;
        if !(1..=32).contains(&count) {
            return Err("repeat count must be between 1 and 32".to_owned());
        }
        let order = geology::mining_order(&self.registries, &self.state, args)?;
        let mut output = String::new();
        let mut completed = 0;
        for _ in 0..count {
            let (job, started) = match order.start(&self.registries, &mut self.state) {
                Ok(started) => started,
                Err(error) if completed == 0 => return Err(error),
                Err(error) => {
                    let _ = writeln!(output, "Mining repeat stopped: {error}");
                    break;
                }
            };
            output.push_str(&started);
            let wait = activity::remaining_work_control_ticks(&self.state)
                .ok_or("mining started without a scheduled completion")?
                .min(activity::COMPLETION_WAIT_LIMIT);
            let (received, interrupted) =
                self.await_batch(stepping::BatchReceipt::Mined(job), wait, &mut output)?;
            completed += usize::from(received);
            if !received || interrupted {
                output.push_str("Mining repeat interrupted by work or a condition change.\n");
                break;
            }
        }
        let _ = writeln!(output, "Mining repeat: {completed} batches completed.");
        output.push_str(&view::look(&self.registries, &self.state, None));
        Ok(output)
    }
}

/// Deterministic seed diffusion for world-creation site selection only. The
/// world owners remain authoritative for terrain and resource generation.
fn natural_site_hash(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}
