//! Continuous text-play stepping and event observation over the canonical simulation boundary.

use std::fmt::Write as _;

use super::{Session, activity, crafting, view};
mod feedback;
use deep_hearth::energy::project_player_nearby_warmth_sources;
use deep_hearth::labor::PlayerWork;
use deep_hearth::logistics::{
    HorizontalControlInput, PlayerControlInput, project_player_standing_space,
};
use deep_hearth::mining::MiningJobId;
use deep_hearth::simulation::{
    PlayerDirectControlInput, SecondaryActionOutcome, advance_control_tick_with_direct_input,
};
use deep_hearth::spatial::{HorizontalVelocity, VerticalVelocity, VoxelCoord};
use deep_hearth::surface::{SurfaceGatheringError, SurfaceResourceId};
use deep_hearth::survival::Vitality;
use deep_hearth::world::{DayPhase, assess_player_weather};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BatchReceipt {
    Gathered(SurfaceResourceId),
    Mined(MiningJobId),
}

pub(super) struct StepReport {
    noteworthy: bool,
    other_event: bool,
    batch_receipt: Option<BatchReceipt>,
    pub(super) stale_gathering_option: bool,
}

impl Session {
    pub(super) fn build_at(
        &mut self,
        index: usize,
        position: VoxelCoord,
        preview: bool,
    ) -> Result<String, String> {
        let inventory =
            deep_hearth::logistics::project_player_inventory(&self.registries, &self.state)
                .map_err(|error| error.to_string())?;
        let stack = inventory
            .carried()
            .stacks()
            .get(index - 1)
            .ok_or("no carried stack at that index")?;
        let validated = deep_hearth::simulation::validate_start_player_supported_timber_at(
            &self.registries,
            &self.state,
            stack,
            position,
        )
        .map_err(|outcome| match outcome {
            SecondaryActionOutcome::NoStructuralPlacementTarget => {
                "destination is not a clear cell within pointing reach".to_owned()
            }
            SecondaryActionOutcome::MaterialNotContextual => {
                "build-at requires a timber log or timber board stack".to_owned()
            }
            _ => feedback::secondary(&self.registries, &outcome),
        })?;
        if preview {
            return Ok(format!(
                "Ready: supported timber at ({},{},{}); current material, support and work admission passed.\n",
                position.x(),
                position.y(),
                position.z()
            ));
        }
        validated
            .commit(&mut self.state)
            .map_err(|error| error.to_string())?;
        Ok(format!(
            "Structural construction started at ({},{},{}).\n{}",
            position.x(),
            position.y(),
            position.z(),
            view::status(&self.registries, &self.state)
        ))
    }

    /// Move toward the centre of the current grounded cell with sampled
    /// low-amplitude input. Never snap, teleport, or bypass the motion owner.
    pub(super) fn center_in_cell(&mut self) -> Result<String, String> {
        if self
            .state
            .survival()
            .player()
            .is_none_or(|player| player.vitality() == Vitality::ZERO)
        {
            return Err("a living admitted player is required to center".to_owned());
        }
        let origin = self.player_position()?;
        let body = self
            .state
            .logistics()
            .player()
            .ok_or("no player admitted")?
            .body();
        if self.state.player_work().active().is_some()
            || body.horizontal_velocity() != HorizontalVelocity::ZERO
            || body.vertical_velocity() != VerticalVelocity::ZERO
        {
            return Err("finish work and settle movement before centering".to_owned());
        }
        let center = deep_hearth::spatial::WorldPoint::centered_on_voxel_floor(origin)
            .ok_or("cell centre is outside the continuous world")?;
        if center.y_nanometers() != body.foot_position().y_nanometers()
            || !project_player_standing_space(&self.registries, &self.state, origin)
        {
            return Err("no grounded standing space at the centre of this cell".to_owned());
        }
        let initial = view::urgency(&self.registries, &self.state);
        let mut output = String::new();
        for _ in 0..240 {
            let body = self
                .state
                .logistics()
                .player()
                .ok_or("no player admitted")?
                .body();
            let foot = body.foot_position();
            let dx = center.x_nanometers() - foot.x_nanometers();
            let dz = center.z_nanometers() - foot.z_nanometers();
            let settled = body.horizontal_velocity() == HorizontalVelocity::ZERO;
            if dx.abs() <= 30_000_000 && dz.abs() <= 30_000_000 && settled {
                output.push_str("Centred in the current cell (within 3 cm).\n");
                output.push_str(&view::look(&self.registries, &self.state, None));
                return Ok(output);
            }
            let component = |delta: i64| {
                if delta.abs() <= 30_000_000 {
                    0
                } else {
                    (delta / 1_000).clamp(-150_000, 150_000) as i32
                }
            };
            let horizontal = HorizontalControlInput::new(component(dx), component(dz))
                .map_err(|error| error.to_string())?;
            let input = PlayerDirectControlInput::new(PlayerControlInput::new(horizontal));
            let report = self.step(input, &mut output)?;
            if report.noteworthy
                || view::urgency(&self.registries, &self.state) != initial
                || self.player_position()? != origin
            {
                output.push_str("Centering interrupted by a physical event or condition change.\n");
                output.push_str(&view::look(&self.registries, &self.state, None));
                return Ok(output);
            }
            let next = self
                .state
                .logistics()
                .player()
                .ok_or("no player admitted")?
                .body();
            if !horizontal.is_neutral()
                && next.foot_position() == foot
                && next.horizontal_velocity() == HorizontalVelocity::ZERO
            {
                break;
            }
        }
        output.push_str(
            "Centering stopped: could not reach the centre with the current clearance.\n",
        );
        output.push_str(&view::look(&self.registries, &self.state, None));
        Ok(output)
    }

    /// Wait awake for the next requested daylight phase, never skipping physical
    /// control steps. Events, changing local threats, expired heat and new
    /// survival warnings end the watch before its requested horizon.
    pub(super) fn advance_until_phase(&mut self, target: DayPhase) -> Result<String, String> {
        let phase_name = match target {
            DayPhase::Morning => "morning",
            DayPhase::Afternoon => "afternoon",
            DayPhase::Evening => "evening",
            DayPhase::Night => "night",
        };
        let current = assess_player_weather(&self.registries, &self.state)
            .ok_or("current local daylight phase is unavailable")?;
        if current.phase() == target {
            return Ok(format!(
                "It is already {phase_name}.\n{}",
                view::status(&self.registries, &self.state),
            ));
        }
        if self.state.player_work().active().is_some() {
            return Err("finish active work with 'wait work' before a daylight watch".to_owned());
        }
        let urgency = view::urgency(&self.registries, &self.state);
        if urgency != (false, false, false, false) {
            return Err(
                "resolve current survival warnings before a long daylight watch".to_owned(),
            );
        }
        let calendar = self.registries.core().calendar();
        let ticks_per_day = calendar.ticks_per_day();
        let day_tick = calendar.date_at(self.state.tick()).day_tick();
        let target_quarter = match target {
            DayPhase::Morning => 0_u128,
            DayPhase::Afternoon => 1,
            DayPhase::Evening => 2,
            DayPhase::Night => 3,
        };
        let target_day_tick =
            u64::try_from((u128::from(ticks_per_day) * target_quarter).div_ceil(4))
                .map_err(|_| "daylight wait target exceeds the clock range")?;
        let slow_ticks = if target_day_tick > day_tick {
            target_day_tick - day_tick
        } else {
            (ticks_per_day - day_tick)
                .checked_add(target_day_tick)
                .ok_or("daylight wait target exceeds the clock range")?
        };
        let scale = deep_hearth::core::time::CONTROL_TICKS_PER_SIMULATION_TICK;
        let maximum = slow_ticks
            .checked_mul(scale)
            .and_then(|ticks| ticks.checked_sub(self.state.control_tick().value() % scale))
            .ok_or("daylight wait exceeds the control-clock range")?;
        let observed_residents = view::local_resident_sightings(&self.registries, &self.state);
        let initial_lit_fires = lit_nearby_fires(&self.registries, &self.state);
        let mut output = String::new();
        let mut actual = 0_u64;
        for _ in 0..maximum {
            let previous_tick = self.state.tick();
            let noteworthy = self
                .step(PlayerDirectControlInput::NEUTRAL, &mut output)?
                .noteworthy;
            actual += 1;
            if noteworthy {
                output.push_str("Daylight watch interrupted by an event.\n");
                break;
            }
            if view::urgency(&self.registries, &self.state) != urgency {
                output.push_str("Daylight watch interrupted by a survival warning.\n");
                break;
            }
            if self.state.tick() != previous_tick {
                if assess_player_weather(&self.registries, &self.state)
                    .is_some_and(|weather| weather.phase() == target)
                {
                    let _ = writeln!(output, "Reached {phase_name}.");
                    break;
                }
                if !initial_lit_fires.is_empty() && {
                    let still_lit = lit_nearby_fires(&self.registries, &self.state);
                    initial_lit_fires
                        .iter()
                        .any(|fire| !still_lit.contains(fire))
                } {
                    output.push_str(
                        "Daylight watch interrupted: a nearby fire stopped providing warmth.\n",
                    );
                    break;
                }
                if let Some(description) = view::resident_watch_attention(
                    &observed_residents,
                    &view::local_resident_sightings(&self.registries, &self.state),
                    self.player_position()?,
                ) {
                    let _ = writeln!(output, "{description}");
                    output.push_str("Daylight watch interrupted by a local change.\n");
                    break;
                }
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

    pub(super) fn advance_one(
        &mut self,
        input: PlayerDirectControlInput,
    ) -> Result<String, String> {
        let mut output = String::new();
        let _ = self.step(input, &mut output)?;
        output.push_str(&view::status(&self.registries, &self.state));
        Ok(output)
    }

    pub(super) fn advance_many(
        &mut self,
        input: PlayerDirectControlInput,
        limit: u64,
        walking: bool,
    ) -> Result<String, String> {
        self.advance_many_with_work_wait(input, limit, walking, false)
    }

    pub(super) fn advance_many_with_work_wait(
        &mut self,
        input: PlayerDirectControlInput,
        limit: u64,
        walking: bool,
        work_wait: bool,
    ) -> Result<String, String> {
        self.advance_many_with_report(input, limit, walking, work_wait)
    }

    /// Wait for the exact completion receipt. Work disappearing through
    /// cancellation (including death) is not a successful batch. Other events
    /// on the completion frame still prohibit automatic admission of another.
    pub(super) fn await_batch(
        &mut self,
        expected: BatchReceipt,
        limit: u64,
        output: &mut String,
    ) -> Result<(bool, bool), String> {
        let initial = view::urgency(&self.registries, &self.state);
        let visible = view::local_cues(&self.registries, &self.state);
        let mut actual = 0;
        let mut completed = false;
        let mut interrupted = false;
        for _ in 0..limit {
            let previous = self.state.tick();
            let report = self.step(PlayerDirectControlInput::NEUTRAL, output)?;
            actual += 1;
            completed = report.batch_receipt == Some(expected);
            interrupted =
                report.other_event || view::urgency(&self.registries, &self.state) != initial;
            // The expected receipt can itself change the local observation
            // (e.g. consuming the final fruit). Re-admission checks the source.
            if !completed
                && !interrupted
                && self.state.tick() != previous
                && let Some(description) = self.wait_attention(&visible, true)?
            {
                let _ = writeln!(output, "{description}");
                interrupted = true;
            }
            if completed
                || interrupted
                || report.noteworthy
                || self.state.player_work().active().is_none()
            {
                break;
            }
        }
        let _ = writeln!(
            output,
            "Advanced {actual} control ticks ({} ms).",
            actual * 50
        );
        Ok((completed, interrupted))
    }

    fn advance_many_with_report(
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
        // A directional jump is one impulse followed by continued horizontal
        // control. Do not hold the jump button across a landing: the motion
        // owner would legitimately launch another jump on the next grounded
        // sample. Nearby changes must not interrupt the first jump mid-ledge.
        let directional_jump = walking && input.movement().jump();
        let mut actual = 0;
        for _ in 0..limit {
            let body_before = walking
                .then(|| self.state.logistics().player().map(|player| player.body()))
                .flatten();
            let slow_tick_before = self.state.tick();
            let sampled_input = if directional_jump && actual != 0 {
                PlayerDirectControlInput::new(input.movement().with_jump(false))
            } else {
                input
            };
            let noteworthy = self.step(sampled_input, &mut output)?.noteworthy;
            actual += 1;
            if noteworthy || view::urgency(&self.registries, &self.state) != before {
                break;
            }
            if !directional_jump
                && self.state.tick() != slow_tick_before
                && let Some(description) = self.wait_attention(&visible_before, work_wait)?
            {
                let _ = writeln!(output, "{description}");
                break;
            }
            // A held walk into a wall can make no continuous progress for the entire
            // requested interval. Stop once the real motion owner has settled at
            // zero velocity and an unchanged horizontal position. A mere lack of
            // voxel crossing is not enough: acceleration moves within a voxel.
            // A single jump may need to push against a ledge while ascending.
            // Once its vertical motion ends, a blocked horizontal input can
            // terminate normally rather than wasting the remaining command.
            if let (Some(previous), Some(current)) = (
                body_before,
                self.state.logistics().player().map(|player| player.body()),
            ) && (!directional_jump || current.vertical_velocity() == VerticalVelocity::ZERO)
                && previous.foot_position().x_nanometers() == current.foot_position().x_nanometers()
                && previous.foot_position().z_nanometers() == current.foot_position().z_nanometers()
                && current.horizontal_velocity() == HorizontalVelocity::ZERO
            {
                output.push_str("Movement stopped: no horizontal progress.\n");
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

    fn wait_attention(
        &self,
        previous: &view::LocalCues,
        work_wait: bool,
    ) -> Result<Option<&'static str>, String> {
        let current = view::local_cues(&self.registries, &self.state);
        if work_wait {
            Ok(previous.work_attention(&current, self.player_position()?))
        } else {
            Ok(previous.changed(&current))
        }
    }

    pub(super) fn player_position(&self) -> Result<VoxelCoord, String> {
        self.state
            .logistics()
            .player()
            .map(|player| player.position())
            .ok_or("player not admitted".to_owned())
    }

    /// Returns true when an observable event should interrupt a timed command.
    pub(super) fn step(
        &mut self,
        input: PlayerDirectControlInput,
        output: &mut String,
    ) -> Result<StepReport, String> {
        let before = self.player_position()?;
        let previous_work = self.state.player_work().active();
        let outcome =
            advance_control_tick_with_direct_input(&self.registries, &mut self.state, input)
                .map_err(|error| format!("simulation tick failed: {error}"))?;
        let stale_gathering_option = matches!(
            outcome.secondary_action(),
            Some(SecondaryActionOutcome::SurfaceGatheringRejected(
                SurfaceGatheringError::StalePlayerOption { .. }
            ))
        );
        // Physical motion and automatic custody transfer precede direct actions
        // in the control owner. Preserve that order when both occur in one frame.
        let mut interrupt = note_text(
            output,
            "Excavated",
            outcome.block_excavation().map(feedback::excavation),
        );
        interrupt |= note_text(
            output,
            "Fall impact",
            outcome
                .body_motion()
                .and_then(|motion| motion.fall_impact())
                .map(feedback::landing),
        );
        interrupt |= note_text(
            output,
            "Picked up",
            outcome
                .surface_drop_pickup()
                .map(|event| feedback::pickup(&self.registries, event)),
        );
        interrupt |= note_text(
            output,
            "Primary",
            outcome.primary_action().map(feedback::primary),
        );
        interrupt |= note_secondary(output, &self.registries, outcome.secondary_action());
        interrupt |= note_text(
            output,
            "Slot",
            outcome
                .quick_access_selection()
                .map(|event| feedback::slot(&self.registries, &self.state, event)),
        );
        interrupt |= note_text(
            output,
            "Drop",
            outcome
                .active_selection_drop()
                .map(|event| feedback::drop(&self.registries, event)),
        );
        let mut batch_receipt = None;
        if let Some(simulation) = outcome.simulation() {
            if let Some(gathered) = simulation.surface_gathering() {
                let _ = writeln!(
                    output,
                    "Gathered: {} g collected; {} g remains at the source.",
                    view::grams(gathered.gathered_mass()),
                    view::grams(gathered.remaining_mass()),
                );
                batch_receipt = Some(BatchReceipt::Gathered(gathered.resource()));
            }
            interrupt |= note_text(
                output,
                "Tree felled",
                simulation.tree_felling().map(feedback::tree_felled),
            );
            interrupt |= note_text(
                output,
                "Construction finished",
                simulation
                    .structural_construction()
                    .map(|_| "The built member is now load-bearing.".to_owned()),
            );
            interrupt |= note_text(
                output,
                "Dismantling finished",
                simulation
                    .structural_dismantling()
                    .map(|_| match previous_work {
                        Some(PlayerWork::StructuralDismantling { work }) => format!(
                            "Recovered {} g of structural materials into your pack.",
                            view::grams(work.recovered_mass()),
                        ),
                        _ => "Structural materials recovered into your pack.".to_owned(),
                    }),
            );
            interrupt |= note_text(
                output,
                "Storage recovered",
                simulation
                    .storage_enclosure_dismantling()
                    .map(|_| "Enclosure materials recovered into storage.".to_owned()),
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
            interrupt |= note_text(
                output,
                "Maintenance finished",
                simulation
                    .equipment_maintenance()
                    .map(|event| feedback::maintenance(&self.registries, &self.state, event)),
            );
            interrupt |= note_text(
                output,
                "Manual power produced",
                simulation.manual_power().map(feedback::power),
            );
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
                batch_receipt = Some(BatchReceipt::Mined(mining.job()));
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
            if let Some(scout) = simulation.kobold_scout() {
                interrupt |= note_text(output, "Scout", feedback::scout(scout));
            }
            for pulse in simulation.kobold_alarm_pulses() {
                interrupt |= note_text(output, "Heard alarm", Some(feedback::alarm(*pulse)));
            }
        }
        if !interrupt
            && batch_receipt.is_none()
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
        Ok(StepReport {
            noteworthy: interrupt || batch_receipt.is_some(),
            other_event: interrupt,
            batch_receipt,
            stale_gathering_option,
        })
    }
}

/// Project only visible intake and gathering facts into the player transcript.
/// Debug formatting the canonical eating receipt would expose private lot IDs,
/// internal energy units and hidden consumption accounting.
fn note_secondary(
    output: &mut String,
    registries: &deep_hearth::Registries,
    event: Option<&SecondaryActionOutcome>,
) -> bool {
    match event {
        Some(SecondaryActionOutcome::EatingStarted(meal)) => {
            let _ = writeln!(
                output,
                "Eating started: {} g of food; finishes at simulation tick {}.",
                view::grams(meal.total_mass()),
                meal.completes_at().value(),
            );
            true
        }
        Some(SecondaryActionOutcome::DrinkingStarted(drink)) => {
            let _ = writeln!(
                output,
                "Drinking started: {} mL; finishes at simulation tick {}.",
                view::milliliters(drink.volume()),
                drink.completes_at().value(),
            );
            true
        }
        Some(SecondaryActionOutcome::SurfaceGatheringStarted(work)) => {
            let _ = writeln!(
                output,
                "Gathering started: {} g; finishes at simulation tick {}.",
                view::grams(work.gathered_mass()),
                work.completes_at().value(),
            );
            true
        }
        Some(other) => note_text(output, "Use", Some(feedback::secondary(registries, other))),
        None => false,
    }
}

fn lit_nearby_fires(
    registries: &deep_hearth::Registries,
    state: &deep_hearth::AppState,
) -> Vec<deep_hearth::energy::EnergyStoreId> {
    project_player_nearby_warmth_sources(registries, state)
        .into_iter()
        .map(|source| source.store())
        .collect()
}

fn note_text(output: &mut String, label: &str, event: Option<String>) -> bool {
    if let Some(value) = event {
        let _ = writeln!(output, "{label}: {value}");
        true
    } else {
        false
    }
}
