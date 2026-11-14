//! Text-play command parsing and dispatch; session setup and stepping remain in the parent.

use super::{Session, crafting, equipment, fire, geology, ore, storage, view};
use deep_hearth::logistics::{HorizontalControlInput, PlayerControlInput};
use deep_hearth::spatial::ViewDirection;

mod control;

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
    NorthShallow,
    SouthShallow,
    EastShallow,
    WestShallow,
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
            "n-shallow" => Ok(Self::NorthShallow),
            "s-shallow" => Ok(Self::SouthShallow),
            "e-shallow" => Ok(Self::EastShallow),
            "w-shallow" => Ok(Self::WestShallow),
            _ => Err(format!(
                "unknown direction {text}; use n, s, e, w, up, down, cardinal-down, or cardinal-shallow"
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
            Self::NorthShallow => (0, -500_000, -866_025),
            Self::SouthShallow => (0, -500_000, 866_025),
            Self::EastShallow => (866_025, -500_000, 0),
            Self::WestShallow => (-866_025, -500_000, 0),
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
            | Self::WestDown
            | Self::NorthShallow
            | Self::SouthShallow
            | Self::EastShallow
            | Self::WestShallow => {
                return Err("vertical or angled direction is for aiming, not walking".to_owned());
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
        if let Some(output) = self.control_command(verb, args)? {
            return Ok((output, true));
        }
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
            "gather" if matches!(args, [_, "repeat", _] | [_, _, "repeat", _]) => {
                self.gather_repeated(args)?
            }
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
                let result = geology::start(&self.registries, &mut self.state, args)?;
                result + &view::status(&self.registries, &self.state)
            }
            "mine" => {
                let result = geology::mine(&self.registries, &mut self.state, args)?;
                result + &view::status(&self.registries, &self.state)
            }
            "ore" if args.is_empty() => ore::catalog(&self.registries, &self.state)?,
            "dress" => {
                let result = ore::start(&self.registries, &mut self.state, args)?;
                result + &view::status(&self.registries, &self.state)
            }
            "recipes" if args.is_empty() || args == ["all"] => {
                crafting::catalog(&self.registries, &self.state, args == ["all"])?
            }
            "craft" => {
                let started = crafting::start(&self.registries, &mut self.state, args)?;
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
