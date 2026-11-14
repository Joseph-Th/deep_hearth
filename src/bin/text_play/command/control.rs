//! Direct-control locomotion, interaction, and time-step commands.
//! These operations share the authoritative control-clock path.

use super::super::activity;
use super::{Direction, Session, parse_index};
use deep_hearth::logistics::{PlayerControlInput, PlayerQuickAccessSlot};
use deep_hearth::simulation::{
    PlayerDirectControlInput, PrimaryControlIntent, SecondaryControlIntent,
};
use deep_hearth::spatial::ViewDirection;
use deep_hearth::world::DayPhase;

const MAX_MOVEMENT_TICKS: u64 = 240;
const MAX_WAIT_TICKS: u64 = 7_200;

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
            _ => return Ok(None),
        };
        Ok(Some(output))
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
