//! Text-play intent routing. All consequences come from public production operations.

mod activity;
mod crafting;
mod equipment;
mod fire;
mod geology;
mod ore;
mod save_file;
mod storage;
mod view;

#[cfg(test)]
mod geology_tests;
#[cfg(test)]
mod tests;

use std::fmt::Write as _;

use deep_hearth::content::BLOCK_STATE_STONE;
use deep_hearth::core::quantity::Mass;
use deep_hearth::logistics::{
    HorizontalControlInput, PlayerActiveSelection, PlayerControlInput, PlayerQuickAccessSlot,
    project_player_inventory, validate_set_active_selection,
};
use deep_hearth::persistence::{LoadedSaveEnvelope, SaveEnvelope, validate_persistable_state};
use deep_hearth::player::{validate_admit_player, validate_respawn_player};
use deep_hearth::simulation::{
    PlayerDirectControlInput, PrimaryControlIntent, SecondaryControlIntent,
    advance_control_tick_with_direct_input,
};
use deep_hearth::spatial::{ViewDirection, VoxelCoord};
use deep_hearth::surface::player_surface_gathering_options;
use deep_hearth::survival::Vitality;
use deep_hearth::world::{
    BlockStateId, ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, WorldGenerationSeed,
    validate_generate_chunk, validate_generate_player_neighborhood,
};
use deep_hearth::world_generation::validate_generate_initial_wilderness;
use deep_hearth::{AppState, Registries, build_registries};

use save_file::{publish_new_save, slot_path};

const MAX_MOVEMENT_TICKS: u64 = 240;
const MAX_WAIT_TICKS: u64 = 7_200;

pub struct Session {
    registries: Registries,
    state: AppState,
    seed: Option<u64>,
}

#[derive(Clone, Copy)]
enum Direction {
    North,
    South,
    East,
    West,
    Up,
    Down,
    NorthDown,
    SouthDown,
    EastDown,
    WestDown,
}

impl Direction {
    fn parse(text: &str) -> Result<Self, String> {
        match text {
            "n" | "north" => Ok(Self::North),
            "s" | "south" => Ok(Self::South),
            "e" | "east" => Ok(Self::East),
            "w" | "west" => Ok(Self::West),
            "up" => Ok(Self::Up),
            "down" => Ok(Self::Down),
            "n-down" => Ok(Self::NorthDown),
            "s-down" => Ok(Self::SouthDown),
            "e-down" => Ok(Self::EastDown),
            "w-down" => Ok(Self::WestDown),
            _ => Err(format!(
                "unknown direction {text}; use n, s, e, w, up, down, or cardinal-down"
            )),
        }
    }

    fn aim(self) -> ViewDirection {
        let (x, y, z) = match self {
            Self::North => (0, 0, -1_000_000),
            Self::South => (0, 0, 1_000_000),
            Self::East => (1_000_000, 0, 0),
            Self::West => (-1_000_000, 0, 0),
            Self::Up => (0, 1_000_000, 0),
            Self::Down => (0, -1_000_000, 0),
            Self::NorthDown => (0, -800_000, -600_000),
            Self::SouthDown => (0, -800_000, 600_000),
            Self::EastDown => (600_000, -800_000, 0),
            Self::WestDown => (-600_000, -800_000, 0),
        };
        ViewDirection::new(x, y, z)
            .unwrap_or_else(|error| panic!("axis direction must be normalized: {error}"))
    }

    fn movement(self) -> Result<PlayerControlInput, String> {
        let (x, z) = match self {
            Self::North => (0, -1_000_000),
            Self::South => (0, 1_000_000),
            Self::East => (1_000_000, 0),
            Self::West => (-1_000_000, 0),
            Self::Up
            | Self::Down
            | Self::NorthDown
            | Self::SouthDown
            | Self::EastDown
            | Self::WestDown => {
                return Err(
                    "vertical or diagonal-down direction is for aiming, not walking".to_owned(),
                );
            }
        };
        let vector = HorizontalControlInput::new(x, z).map_err(|error| error.to_string())?;
        Ok(PlayerControlInput::new(vector))
    }
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

    pub fn command(&mut self, line: &str) -> Result<(String, bool), String> {
        let tokens = line.split_ascii_whitespace().collect::<Vec<_>>();
        let Some(&verb) = tokens.first() else {
            return Ok((String::new(), true));
        };
        let args = &tokens[1..];
        let output = match verb {
            "help" if args.is_empty() => super::usage().to_owned() + "\n",
            "look" if args.len() <= 1 => {
                let aim = args.first().map(|arg| Direction::parse(arg)).transpose()?;
                view::look(&self.registries, &self.state, aim.map(Direction::aim))
            }
            "status" if args.is_empty() => view::status(&self.registries, &self.state),
            "inventory" | "inv" if args.is_empty() => {
                view::inventory(&self.registries, &self.state)?
            }
            "gather" if args.len() <= 2 => self.gather(args)?,
            "surveys" if args.len() <= 1 => {
                let width = args.first().map_or(Ok(1_usize), |value| {
                    value
                        .parse::<usize>()
                        .map_err(|_| "survey width must be a number")
                })?;
                geology::catalog(&self.registries, &self.state, width)?
            }
            "knowledge" if args.len() <= 1 => {
                let width = args.first().map_or(Ok(1_usize), |value| {
                    value
                        .parse::<usize>()
                        .map_err(|_| "knowledge width must be a number")
                })?;
                geology::knowledge(&self.registries, &self.state, width)?
            }
            "prospect" => {
                let result = geology::start(&self.registries, &mut self.state, args)
                    .map_err(work_start_error)?;
                result + &view::status(&self.registries, &self.state)
            }
            "mine" => {
                let result = geology::mine(&self.registries, &mut self.state, args)
                    .map_err(work_start_error)?;
                result + &view::status(&self.registries, &self.state)
            }
            "ore" if args.is_empty() => ore::catalog(&self.registries, &self.state)?,
            "dress" => {
                let result = ore::start(&self.registries, &mut self.state, args)
                    .map_err(work_start_error)?;
                result + &view::status(&self.registries, &self.state)
            }
            "recipes" if args.is_empty() || args == ["all"] => {
                crafting::catalog(&self.registries, &self.state, args == ["all"])?
            }
            "craft" => {
                let started = crafting::start(&self.registries, &mut self.state, args)
                    .map_err(work_start_error)?;
                started + &view::status(&self.registries, &self.state)
            }
            "gear" if args.is_empty() || args == ["all"] => {
                equipment::catalog(&self.registries, &self.state, args == ["all"])?
            }
            "assemble" | "take" | "equip" | "recover" if args.len() == 1 => {
                let index = parse_index(args[0], usize::MAX)?;
                let report = match verb {
                    "assemble" => equipment::assemble(&self.registries, &mut self.state, index),
                    "take" => equipment::take(&self.registries, &mut self.state, index),
                    "equip" => equipment::equip(&self.registries, &mut self.state, index),
                    _ => equipment::recover(&self.registries, &mut self.state, index),
                }?;
                report + &view::status(&self.registries, &self.state)
            }
            "storage" if args.is_empty() || args == ["all"] => {
                storage::catalog(&self.registries, &self.state, args == ["all"])?
            }
            "place-store" if args.len() == 1 => {
                let id = args[0]
                    .parse::<u32>()
                    .map_err(|_| "storage definition ID must be a positive integer")?;
                let report = storage::place(&self.registries, &mut self.state, id)?;
                report + &storage::catalog(&self.registries, &self.state, false)?
            }
            "stash" | "withdraw" if args.len() == 2 => {
                let container = parse_index(args[0], usize::MAX)?;
                let stack = parse_index(args[1], usize::MAX)?;
                let report = storage::move_stack(
                    &self.registries,
                    &mut self.state,
                    container,
                    stack,
                    verb == "withdraw",
                )?;
                report + &view::inventory(&self.registries, &self.state)?
            }
            "fire" if args.is_empty() => fire::catalog(&self.registries, &self.state)?,
            "build-fire" if args.is_empty() => {
                let report = fire::build(&self.registries, &mut self.state)?;
                report + &fire::catalog(&self.registries, &self.state)?
            }
            "feed-fire" if args.len() == 2 => {
                let store = parse_index(args[0], usize::MAX)?;
                let stack = parse_index(args[1], usize::MAX)?;
                let report = fire::fuel(&self.registries, &mut self.state, store, stack)?;
                report + &fire::catalog(&self.registries, &self.state)?
            }
            "n" | "s" | "e" | "w" if args.len() <= 1 => {
                let direction = Direction::parse(verb)?;
                let count = parse_count(args.first().copied(), 20, MAX_MOVEMENT_TICKS)?;
                self.advance_many(
                    PlayerDirectControlInput::new(direction.movement()?),
                    count,
                    true,
                )?
            }
            "sprint" | "sneak" if (1..=2).contains(&args.len()) => {
                let direction = Direction::parse(args[0])?;
                let count = parse_count(args.get(1).copied(), 20, MAX_MOVEMENT_TICKS)?;
                let move_input = direction.movement()?;
                let move_input = if verb == "sprint" {
                    move_input.with_sprint(true)
                } else {
                    move_input.with_crouch(true)
                };
                self.advance_many(PlayerDirectControlInput::new(move_input), count, true)?
            }
            "crouch" if args.len() <= 1 => {
                let count = parse_count(args.first().copied(), 1, MAX_MOVEMENT_TICKS)?;
                self.advance_many(
                    PlayerDirectControlInput::new(PlayerControlInput::NEUTRAL.with_crouch(true)),
                    count,
                    false,
                )?
            }
            "jump" if args.is_empty() => self.advance_many(
                PlayerDirectControlInput::new(PlayerControlInput::NEUTRAL.with_jump(true)),
                1,
                false,
            )?,
            "jump" if (1..=2).contains(&args.len()) => {
                let direction = Direction::parse(args[0])?;
                let count = parse_count(args.get(1).copied(), 20, MAX_MOVEMENT_TICKS)?;
                self.advance_many(
                    PlayerDirectControlInput::new(direction.movement()?.with_jump(true)),
                    count,
                    true,
                )?
            }
            "wait" if args == ["boundary"] => {
                let scale = deep_hearth::core::time::CONTROL_TICKS_PER_SIMULATION_TICK;
                let remainder = self.state.control_tick().value() % scale;
                if remainder == 0 {
                    "Already on a simulation boundary; scheduled work can start now.\n".to_owned()
                } else {
                    self.advance_many(PlayerDirectControlInput::NEUTRAL, scale - remainder, false)?
                }
            }
            "wait" if args == ["work"] => {
                let count = activity::remaining_work_control_ticks(&self.state)
                    .ok_or("no active work with a scheduled completion; use wait [ticks]")?
                    .min(MAX_WAIT_TICKS);
                self.advance_many_with_work_wait(
                    PlayerDirectControlInput::NEUTRAL,
                    count,
                    false,
                    true,
                )?
            }
            "wait" if args.len() <= 1 => {
                let count = parse_count(args.first().copied(), 72, MAX_WAIT_TICKS)?;
                self.advance_many(PlayerDirectControlInput::NEUTRAL, count, false)?
            }
            "use" if args.len() <= 1 => {
                let aim = aim_or_default(args)?;
                self.advance_one(
                    PlayerDirectControlInput::NEUTRAL
                        .with_secondary(SecondaryControlIntent::use_active(aim)),
                )?
            }
            "break" | "shove" | "strike" | "prepare" | "release" if args.len() <= 1 => {
                let aim = aim_or_default(args)?;
                let primary = match verb {
                    "break" => PrimaryControlIntent::break_target(aim),
                    "shove" => PrimaryControlIntent::shove(aim),
                    "strike" => PrimaryControlIntent::strike(aim),
                    "prepare" => PrimaryControlIntent::prepare_strike(aim),
                    _ => PrimaryControlIntent::release_strike(aim),
                };
                self.advance_one(PlayerDirectControlInput::NEUTRAL.with_primary(primary))?
            }
            "cancel" if args.is_empty() => self.advance_one(
                PlayerDirectControlInput::NEUTRAL
                    .with_primary(PrimaryControlIntent::cancel_strike()),
            )?,
            "drop" if args.is_empty() => {
                self.advance_one(PlayerDirectControlInput::NEUTRAL.with_drop_active_selection())?
            }
            "slot" if args.len() == 1 => {
                let number = parse_index(args[0], 9)?;
                let slot = PlayerQuickAccessSlot::new((number - 1) as u8)
                    .ok_or("slot is outside the quick-access bar")?;
                self.advance_one(PlayerDirectControlInput::NEUTRAL.with_quick_access_slot(slot))?
            }
            "select" if args.len() == 1 => self.select(args[0])?,
            "empty" if args.is_empty() => self.set_selection(None)?,
            "respawn" if args.is_empty() => self.respawn()?,
            "save" if args.len() == 1 => {
                self.save(args[0])?;
                format!("Saved current validated campaign as {}.\n", args[0])
            }
            "quit" if args.is_empty() => return Ok(("Session ended.\n".to_owned(), false)),
            _ => return Err(format!("unknown command or arguments: {line}; type help")),
        };
        Ok((output, true))
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
            let grams = grams
                .parse::<u64>()
                .map_err(|_| "gather quantity must be whole grams")?;
            Mass::from_milligrams(
                grams
                    .checked_mul(1_000)
                    .ok_or("gather quantity is too large")?,
            )
        } else {
            option
                .source_mass()
                .min(option.maximum_batch_mass())
                .min(option.direct_batch_mass())
        };
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

    fn advance_one(&mut self, input: PlayerDirectControlInput) -> Result<String, String> {
        let mut output = String::new();
        let _ = self.step(input, &mut output)?;
        output.push_str(&view::status(&self.registries, &self.state));
        Ok(output)
    }

    fn advance_many(
        &mut self,
        input: PlayerDirectControlInput,
        limit: u64,
        walking: bool,
    ) -> Result<String, String> {
        self.advance_many_with_work_wait(input, limit, walking, false)
    }

    fn advance_many_with_work_wait(
        &mut self,
        input: PlayerDirectControlInput,
        limit: u64,
        walking: bool,
        work_wait: bool,
    ) -> Result<String, String> {
        let mut output = String::new();
        let starting_position = self.player_position()?;
        let before = view::urgency(&self.registries, &self.state);
        let visible_before = view::local_cues(&self.registries, &self.state);
        let mut actual = 0;
        for _ in 0..limit {
            let slow_tick_before = self.state.tick();
            let noteworthy = self.step(input, &mut output)?;
            actual += 1;
            if noteworthy || view::urgency(&self.registries, &self.state) != before {
                break;
            }
            if self.state.tick() != slow_tick_before
                && let Some((description, requires_attention)) =
                    visible_before.changed(&view::local_cues(&self.registries, &self.state))
                && (requires_attention || !work_wait)
            {
                let _ = writeln!(output, "{description}");
                break;
            }
            let position = self.player_position()?;
            // A directional jump crosses the vertical voxel before its horizontal
            // destination. Do not release movement while the player is still overhead.
            if walking
                && (position.x() != starting_position.x() || position.z() != starting_position.z())
            {
                break;
            }
            if self
                .state
                .survival()
                .player()
                .is_some_and(|s| s.vitality() == Vitality::ZERO)
            {
                break;
            }
        }
        let _ = writeln!(
            output,
            "Advanced {actual} control ticks ({} ms).",
            actual * 50
        );
        output.push_str(&view::look(&self.registries, &self.state, None));
        Ok(output)
    }

    fn player_position(&self) -> Result<VoxelCoord, String> {
        self.state
            .logistics()
            .player()
            .map(|player| player.position())
            .ok_or("player not admitted".to_owned())
    }

    /// Returns true when an observable event should interrupt a timed command.
    fn step(
        &mut self,
        input: PlayerDirectControlInput,
        output: &mut String,
    ) -> Result<bool, String> {
        let before = self.player_position()?;
        let previous_work = self.state.player_work().active();
        let outcome =
            advance_control_tick_with_direct_input(&self.registries, &mut self.state, input)
                .map_err(|error| format!("simulation tick failed: {error}"))?;
        let mut interrupt = note(output, "Primary", outcome.primary_action());
        interrupt |= note(output, "Use", outcome.secondary_action());
        interrupt |= note(output, "Slot", outcome.quick_access_selection());
        interrupt |= note(output, "Drop", outcome.active_selection_drop());
        interrupt |= note(output, "Excavated", outcome.block_excavation());
        interrupt |= note(output, "Picked up", outcome.surface_drop_pickup());
        interrupt |= note(
            output,
            "Fall impact",
            outcome
                .body_motion()
                .and_then(|motion| motion.fall_impact()),
        );
        if let Some(simulation) = outcome.simulation() {
            if let Some(gathered) = simulation.surface_gathering() {
                let _ = writeln!(
                    output,
                    "Gathered: {} g collected; {} g remains at the source.",
                    view::grams(gathered.gathered_mass()),
                    view::grams(gathered.remaining_mass()),
                );
                interrupt = true;
            }
            interrupt |= note(output, "Tree felled", simulation.tree_felling());
            interrupt |= note(
                output,
                "Construction finished",
                simulation.structural_construction(),
            );
            interrupt |= note(
                output,
                "Dismantling finished",
                simulation.structural_dismantling(),
            );
            interrupt |= note(
                output,
                "Storage recovered",
                simulation.storage_enclosure_dismantling(),
            );
            if let Some(survey) = simulation.field_prospecting() {
                let material = self
                    .registries
                    .materials()
                    .get_material(survey.material())
                    .map_or("unknown material", |definition| definition.name());
                let _ = writeln!(
                    output,
                    "Prospecting finished: {} {:?} observation(s) of {material}. Use 'knowledge' or 'knowledge 2' to inspect acquired evidence.",
                    survey.observation_count(),
                    survey.evidence(),
                );
                interrupt = true;
            }
            interrupt |= note(
                output,
                "Maintenance finished",
                simulation.equipment_maintenance(),
            );
            interrupt |= note(output, "Manual power produced", simulation.manual_power());
            if let Some(mining) = simulation.mining_output() {
                let material = self
                    .registries
                    .materials()
                    .commodity_name(mining.output().commodity())
                    .unwrap_or("unidentified ore");
                let _ = writeln!(
                    output,
                    "Mining produced: {} g {material} entered inventory.",
                    view::grams(mining.output().mass()),
                );
                interrupt = true;
            }
            if let Some(deep_hearth::labor::PlayerWork::ManualProduction { job }) = previous_work {
                for completion in simulation
                    .production_completions()
                    .iter()
                    .filter(|receipt| receipt.job() == job)
                {
                    output.push_str(&crafting::completion(
                        &self.registries,
                        &self.state,
                        completion,
                    ));
                    interrupt = true;
                }
            }
            for pulse in simulation.kobold_alarm_pulses() {
                interrupt |= note(output, "Heard alarm", Some(pulse));
            }
        }
        if !interrupt
            && let Some(work) = previous_work
            && self.state.player_work().active().is_none()
        {
            let _ = writeln!(
                output,
                "Work ended: {}",
                activity::label(&self.registries, &self.state, work)
            );
            interrupt = true;
        }
        if self.player_position()? != before {
            Self::expand_neighborhood(&self.registries, &mut self.state)?;
        }
        Ok(interrupt)
    }
}

fn note<T: std::fmt::Debug>(output: &mut String, label: &str, event: Option<T>) -> bool {
    if let Some(value) = event {
        let _ = writeln!(output, "{label}: {value:?}");
        true
    } else {
        false
    }
}

fn work_start_error(error: String) -> String {
    if error.contains("slow-scheduled work cannot begin") {
        "This task must start on a simulation boundary. Run 'wait boundary', then retry.".to_owned()
    } else {
        error
    }
}

fn aim_or_default(args: &[&str]) -> Result<ViewDirection, String> {
    let direction = args
        .first()
        .map_or(Ok(Direction::Down), |arg| Direction::parse(arg))?;
    Ok(direction.aim())
}

fn parse_count(text: Option<&str>, default: u64, max: u64) -> Result<u64, String> {
    let count = match text {
        Some(text) => text
            .parse()
            .map_err(|_| "tick count must be a whole number")?,
        None => default,
    };
    if count == 0 || count > max {
        return Err(format!("tick count must be between 1 and {max}"));
    }
    Ok(count)
}

fn parse_index(text: &str, maximum: usize) -> Result<usize, String> {
    let index: usize = text
        .parse()
        .map_err(|_| "selection must be a 1-based number")?;
    if index == 0 || index > maximum {
        return Err(format!(
            "selection {index} is not available (1..={maximum})"
        ));
    }
    Ok(index)
}
