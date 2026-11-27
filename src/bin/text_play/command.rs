//! Text-play command parsing and dispatch; session setup and stepping remain in the parent.

use super::{
    Session, crafting, equipment, fire, gathering::GatheringOrder, geology, ore, storage, thermal,
    view,
};
use deep_hearth::logistics::{HorizontalControlInput, PlayerControlInput};
use deep_hearth::simulation::PlayerDirectControlInput;
use deep_hearth::spatial::{ViewDirection, VoxelCoord};
use deep_hearth::world::aim_player_at_voxel;

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
    NorthUp,
    SouthUp,
    EastUp,
    WestUp,
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
            "n-up" => Ok(Self::NorthUp),
            "s-up" => Ok(Self::SouthUp),
            "e-up" => Ok(Self::EastUp),
            "w-up" => Ok(Self::WestUp),
            _ => Err(format!(
                "unknown direction {text}; use n, s, e, w, up, down, cardinal-down, cardinal-shallow, or cardinal-up"
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
            Self::NorthUp => (0, 500_000, -866_025),
            Self::SouthUp => (0, 500_000, 866_025),
            Self::EastUp => (866_025, 500_000, 0),
            Self::WestUp => (-866_025, 500_000, 0),
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
            | Self::WestShallow
            | Self::NorthUp
            | Self::SouthUp
            | Self::EastUp
            | Self::WestUp => {
                return Err("vertical or angled direction is for aiming, not walking".to_owned());
            }
        };
        let vector = HorizontalControlInput::new(x, z).map_err(|error| error.to_string())?;
        Ok(PlayerControlInput::new(vector))
    }
}

impl Session {
    fn parse_aim(&self, args: &[&str]) -> Result<ViewDirection, String> {
        match args {
            [] => Ok(Direction::Down.aim()),
            [direction] => Ok(Direction::parse(direction)?.aim()),
            ["at", x, y, z] => {
                let target = parse_voxel(x, y, z)?;
                aim_player_at_voxel(&self.registries, &self.state, target)
                    .map_err(|error| error.to_string())
            }
            _ => Err("use a direction or 'at X Y Z' to aim at a voxel centre".to_owned()),
        }
    }

    pub fn command(&mut self, line: &str) -> Result<(String, bool), String> {
        let tokens = line.split_ascii_whitespace().collect::<Vec<_>>();
        let Some(&verb) = tokens.first() else {
            return Ok((String::new(), true));
        };
        let args = &tokens[1..];
        if verb == "begin" {
            return self.begin_aligned(args);
        }
        if let Some(output) = self.control_command(verb, args)? {
            return Ok((output, true));
        }
        let output = match verb {
            "help" if args.is_empty() => super::super::usage().to_owned() + "\n",
            "look" => {
                let aim = if args.is_empty() {
                    None
                } else {
                    Some(self.parse_aim(args)?)
                };
                view::look(&self.registries, &self.state, aim)
            }
            "status" if args.is_empty() => view::status(&self.registries, &self.state),
            "inventory" | "inv" if args.is_empty() => {
                view::inventory(&self.registries, &self.state)?
            }
            "gather" => self.gather(args)?,
            "eat" if !args.is_empty() => self.eat(&args.join(" "))?,
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
            "mine" if let [order @ .., "repeat", count] = args => {
                self.mine_repeated(order, count)?
            }
            "mine" => {
                let result = geology::mine(&self.registries, &mut self.state, args)?;
                result + &view::status(&self.registries, &self.state)
            }
            "ore" if args.is_empty() => ore::catalog(&self.registries, &self.state)?,
            "dress" if let [order @ .., "repeat", count] = args => {
                self.dress_repeated(order, count)?
            }
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
            "service" if args.len() == 2 => {
                let index = parse_index(args[1], usize::MAX)?;
                let report = equipment::service(&self.registries, &mut self.state, args[0], index)?;
                report + &view::status(&self.registries, &self.state)
            }
            "upgrade" if args.len() == 2 => {
                let index = parse_index(args[0], usize::MAX)?;
                let target = args[1]
                    .parse::<u32>()
                    .map_err(|_| "equipment definition ID must be a positive integer")?;
                let report = equipment::upgrade(&self.registries, &mut self.state, index, target)?;
                report + &view::status(&self.registries, &self.state)
            }
            "plan-at" | "build-at" if args.len() == 4 => {
                let stack = parse_index(args[0], usize::MAX)?;
                let target = parse_voxel(args[1], args[2], args[3])?;
                self.build_at(stack, target, verb == "plan-at")?
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
            "stash" | "withdraw" if (2..=3).contains(&args.len()) => {
                let container = parse_index(args[0], usize::MAX)?;
                let stack = parse_index(args[1], usize::MAX)?;
                let report = storage::move_stack(
                    &self.registries,
                    &mut self.state,
                    container,
                    stack,
                    verb == "withdraw",
                    args.get(2)
                        .map(|value| super::quantity::parse_decimal_grams(value, "transfer mass"))
                        .transpose()?,
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
            "thermal" if args.is_empty() => thermal::catalog(&self.registries, &self.state)?,
            "build-heat" if args.len() == 1 => {
                thermal::build(&self.registries, &mut self.state, args[0])?
            }
            "feed-heat" if args.len() == 2 => {
                thermal::fuel(&self.registries, &mut self.state, args)?
            }
            "heat" if args.len() == 4 || args.len() == 5 => {
                thermal::start(&self.registries, &mut self.state, args)?
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

    /// Opt in to advancing the real clock to a scheduled-work boundary before
    /// using the same validation and commitment path as the ordinary command.
    /// A rejected command may still have spent up to one simulation tick.
    fn begin_aligned(&mut self, args: &[&str]) -> Result<(String, bool), String> {
        let Some((&verb, operands)) = args.split_first() else {
            return Err(
                "usage: begin craft|gather|prospect|mine|dress|build-at|heat ...".to_owned(),
            );
        };
        if operands.is_empty()
            || !matches!(
                verb,
                "craft" | "gather" | "prospect" | "mine" | "dress" | "build-at" | "heat"
            )
        {
            return Err("begin accepts a scheduled craft, gather, prospect, mine, dress, build-at or heat request with arguments".to_owned());
        }
        if self.state.player_work().active().is_some()
            || self.state.logistics().has_pending_direct_action()
        {
            return Err(
                "finish existing work or a queued interaction before beginning a new task"
                    .to_owned(),
            );
        }
        let clock_before = self.state.control_tick().value();
        let scale = deep_hearth::core::time::CONTROL_TICKS_PER_SIMULATION_TICK;
        let remainder = clock_before % scale;
        // Keep the exact source and technique selected before the clock
        // moves. Defer parse/selection errors until the normal alignment
        // finishes, so independent interruptions still take precedence.
        let gathering = (verb == "gather" && remainder != 0)
            .then(|| GatheringOrder::resolve(&self.registries, &self.state, operands));
        let mut report = String::new();
        if remainder != 0 {
            let work_location = self.player_position()?;
            let alignment = self.advance_many_with_report(
                PlayerDirectControlInput::NEUTRAL,
                scale - remainder,
                false,
                super::stepping::TimedWaitMode::Ordinary,
            )?;
            report = alignment.output;
            if alignment.stop != super::stepping::TimedStop::HorizonReached {
                report.push_str(
                    "Begin interrupted during boundary alignment; requested work was not started.\n",
                );
                return Ok((report, true));
            }
            if self.player_position()? != work_location {
                report.push_str(
                    "Begin interrupted: your location changed during boundary alignment; requested work was not started.\n",
                );
                return Ok((report, true));
            }
        }
        let aligned = self.state.control_tick().value() - clock_before;
        let started = match gathering {
            Some(Ok(mut order)) => match order.refresh(&self.registries, &self.state) {
                Ok(true) => self
                    .execute_gathering_order(order)
                    .map(|output| (output, true)),
                Ok(false) => {
                    report.push_str("Begin interrupted: the selected gathering source or method is no longer available; requested work was not started.\n");
                    return Ok((report, true));
                }
                Err(error) => Err(error),
            },
            Some(Err(error)) => Err(error),
            None => self.command(&args.join(" ")),
        };
        match started {
            Ok((started, _)) => {
                if aligned != 0 {
                    report.push_str(&format!(
                        "Begin: advanced {aligned} control ticks to the next work boundary.\n"
                    ));
                }
                report.push_str(&started);
                Ok((report, true))
            }
            Err(error) if aligned == 0 => Err(error),
            Err(error) => Err(format!(
                "begin {verb} rejected after advancing {aligned} control ticks to the next boundary: {error}"
            )),
        }
    }
}

fn parse_voxel(x: &str, y: &str, z: &str) -> Result<VoxelCoord, String> {
    let coordinate = |value: &str| {
        value
            .parse::<i64>()
            .map_err(|_| "coordinates must be whole voxel coordinates".to_owned())
    };
    Ok(VoxelCoord::new(
        coordinate(x)?,
        coordinate(y)?,
        coordinate(z)?,
    ))
}

pub(super) fn parse_index(text: &str, maximum: usize) -> Result<usize, String> {
    let index: usize = text
        .parse()
        .map_err(|_| "selection must be a 1-based number")?;
    if maximum == 0 {
        return Err("no selections are currently available".to_owned());
    }
    if index == 0 || index > maximum {
        return Err(format!(
            "selection {index} is not available (1..={maximum})"
        ));
    }
    Ok(index)
}

pub(super) fn parse_repeat_count(text: &str) -> Result<usize, String> {
    let count = text
        .parse::<usize>()
        .map_err(|_| "repeat count must be a whole number")?;
    if !(1..=32).contains(&count) {
        return Err("repeat count must be between 1 and 32".to_owned());
    }
    Ok(count)
}
