//! Text-play command parsing and dispatch; session setup and stepping remain in the parent.

use super::{Session, activity, crafting, equipment, fire, geology, ore, storage, view};
use deep_hearth::core::quantity::Mass;
use deep_hearth::logistics::{HorizontalControlInput, PlayerControlInput, PlayerQuickAccessSlot};
use deep_hearth::simulation::{
    PlayerDirectControlInput, PrimaryControlIntent, SecondaryControlIntent,
};
use deep_hearth::spatial::ViewDirection;

const MAX_MOVEMENT_TICKS: u64 = 240;
const MAX_WAIT_TICKS: u64 = 7_200;

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
    pub fn command(&mut self, line: &str) -> Result<(String, bool), String> {
        let tokens = line.split_ascii_whitespace().collect::<Vec<_>>();
        let Some(&verb) = tokens.first() else {
            return Ok((String::new(), true));
        };
        let args = &tokens[1..];
        let output = match verb {
            "help" if args.is_empty() => super::super::usage().to_owned() + "\n",
            "look" if args.len() <= 1 => {
                let aim = args.first().map(|arg| Direction::parse(arg)).transpose()?;
                view::look(&self.registries, &self.state, aim.map(Direction::aim))
            }
            "status" if args.is_empty() => view::status(&self.registries, &self.state),
            "inventory" | "inv" if args.is_empty() => {
                view::inventory(&self.registries, &self.state)?
            }
            "gather" if args.len() <= 2 => self.gather(args)?,
            "eat" if args.len() == 1 => self.eat(args[0])?,
            "drink" if args.is_empty() => view::drink_options(&self.registries, &self.state),
            "drink" if args.len() == 1 => self.drink(args[0])?,
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

pub(super) fn parse_index(text: &str, maximum: usize) -> Result<usize, String> {
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

/// Parse the same exact milligram precision displayed to the player; never
/// round through floating point or silently truncate a source remainder.
pub(super) fn parse_gather_grams(text: &str) -> Result<Mass, String> {
    const INVALID: &str = "gather quantity must be decimal grams (up to three fractional digits)";
    let (whole, fractional) = match text.split_once('.') {
        Some((whole, fractional)) if (1..=3).contains(&fractional.len()) => {
            (whole, Some(fractional))
        }
        Some(_) => return Err(INVALID.to_owned()),
        None => (text, None),
    };
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || fractional.is_some_and(|fraction| !fraction.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err(INVALID.to_owned());
    }
    let grams = whole
        .parse::<u64>()
        .map_err(|_| "gather quantity is too large")?;
    let fractional_milligrams = match fractional {
        Some(fraction) => {
            let digits = fraction.parse::<u64>().map_err(|_| INVALID)?;
            digits
                * match fraction.len() {
                    1 => 100,
                    2 => 10,
                    3 => 1,
                    _ => unreachable!("fractional digits already validated"),
                }
        }
        None => 0,
    };
    let milligrams = grams
        .checked_mul(1_000)
        .and_then(|amount| amount.checked_add(fractional_milligrams))
        .ok_or("gather quantity is too large")?;
    Ok(Mass::from_milligrams(milligrams))
}
