//! Direct-control locomotion, interaction, and time-step commands.
//! These operations share the authoritative control-clock path.

use super::super::{
    activity,
    stepping::{TimedStop, TimedWaitMode},
};
use super::{Direction, Session, parse_index};
use deep_hearth::core::time::DayPhase;
use deep_hearth::logistics::{PlayerControlInput, PlayerQuickAccessSlot};
use deep_hearth::simulation::{
    PlayerDirectControlInput, PrimaryControlIntent, SecondaryControlIntent,
};

const MAX_MOVEMENT_TICKS: u64 = 240;

impl Session {
    /// Only recognized valid-shape verbs are executed here; other command shapes
    /// retain the main dispatcher's unchanged error and do not advance the clock.
    pub(super) fn control_command(
        &mut self,
        verb: &str,
        args: &[&str],
    ) -> Result<Option<String>, String> {
        let output = match verb {
            "n" | "s" | "e" | "w" if args.len() <= 1 => {
                let direction = Direction::parse(verb)?;
                let count = parse_count(args.first().copied(), 20, MAX_MOVEMENT_TICKS)?;
                self.advance_many(
                    PlayerDirectControlInput::new(direction.movement()?),
                    count,
                    true,
                )?
            }
            "trek" => {
                let (args, steady) = if let Some(("steady", rest)) =
                    args.split_last().map(|(last, rest)| (*last, rest))
                {
                    (rest, true)
                } else {
                    (args, false)
                };
                if !(1..=2).contains(&args.len()) {
                    return Err("usage: trek DIRECTION [cells] [steady]".to_owned());
                }
                let direction = Direction::parse(args[0])?;
                let cells = parse_count(args.get(1).copied(), 8, 32)?;
                self.trek(direction.movement()?, cells, steady, false)?
            }
            "descend" if args.len() == 1 => {
                self.trek(Direction::parse(args[0])?.movement()?, 1, true, true)?
            }
            "climb" if args.len() == 1 => self.trek(
                Direction::parse(args[0])?.movement()?.with_climb(true),
                1,
                true,
                false,
            )?,
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
            "wait" if let ["until", phase] = args => {
                let target = match *phase {
                    "morning" => DayPhase::Morning,
                    "afternoon" => DayPhase::Afternoon,
                    "evening" => DayPhase::Evening,
                    "night" => DayPhase::Night,
                    _ => {
                        return Err(
                            "wait until accepts morning, afternoon, evening, or night".to_owned()
                        );
                    }
                };
                self.advance_until_phase(target)?
            }
            "center" if args.is_empty() => self.center_in_cell()?,
            "wait"
                if (args == ["work"] || args == ["finish"])
                    && self.state.player_work().active().is_none()
                    && self.state.logistics().has_pending_direct_action() =>
            {
                self.wait_queued_action(args == ["finish"])?
            }
            "wait" if args == ["work"] || args == ["finish"] => {
                let count = activity::remaining_work_control_ticks(&self.state)
                    .ok_or("no active work with a scheduled completion; use wait [ticks]")?
                    .min(if args == ["finish"] {
                        activity::COMPLETION_WAIT_LIMIT
                    } else {
                        activity::SHORT_WAIT_LIMIT
                    });
                self.advance_many_with_report(
                    PlayerDirectControlInput::NEUTRAL,
                    count,
                    false,
                    TimedWaitMode::Work,
                )?
                .output
            }
            "wait" if args.len() <= 1 => {
                let count = parse_count(args.first().copied(), 72, activity::SHORT_WAIT_LIMIT)?;
                self.advance_many(PlayerDirectControlInput::NEUTRAL, count, false)?
            }
            "use" => {
                let aim = self.parse_aim(args)?;
                self.advance_one(
                    PlayerDirectControlInput::NEUTRAL
                        .with_secondary(SecondaryControlIntent::use_active(aim)),
                )?
            }
            "break" if args.len() >= 2 && args[args.len() - 2] == "repeat" => {
                let (aim, repeat) = args.split_at(args.len() - 2);
                self.repeat_excavation(
                    self.parse_aim(aim)?,
                    parse_count(repeat.get(1).copied(), 1, 32)?,
                )?
            }
            "break" | "shove" | "strike" | "prepare" | "release" => {
                let aim = self.parse_aim(args)?;
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
            _ => return Ok(None),
        };
        Ok(Some(output))
    }

    /// Resolve buffered physical input at its real admission boundary. A work
    /// wait stops there; a finish wait continues through the scheduled job,
    /// unless admission failed or a distinct interruption occurred, including
    /// on the admission frame itself.
    fn wait_queued_action(&mut self, finish: bool) -> Result<String, String> {
        let scale = deep_hearth::core::time::CONTROL_TICKS_PER_SIMULATION_TICK;
        let remaining = scale - self.state.control_tick().value() % scale;
        let admission = self.advance_many_with_report(
            PlayerDirectControlInput::NEUTRAL,
            remaining,
            false,
            TimedWaitMode::QueuedAdmission,
        )?;
        let mut output = admission.output;
        if finish
            && admission.stop == TimedStop::WorkAdmitted
            && let Some(work_ticks) = activity::remaining_work_control_ticks(&self.state)
        {
            output.push_str(
                &self
                    .advance_many_with_report(
                        PlayerDirectControlInput::NEUTRAL,
                        work_ticks.min(activity::COMPLETION_WAIT_LIMIT),
                        false,
                        TimedWaitMode::Work,
                    )?
                    .output,
            );
        }
        Ok(output)
    }
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
