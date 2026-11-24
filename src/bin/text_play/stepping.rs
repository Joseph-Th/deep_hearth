//! Continuous text-play stepping and event observation over the canonical simulation boundary.

use std::fmt::Write as _;

use super::{Session, activity, crafting, view};
use deep_hearth::content::FORM_BOARD;
#[path = "stepping/excavation.rs"]
mod excavation;
mod feedback;
#[path = "stepping/travel.rs"]
mod travel;
use deep_hearth::energy::project_player_nearby_warmth_sources;
use deep_hearth::labor::PlayerWork;
use deep_hearth::logistics::{
    HorizontalControlInput, PlayerControlInput, project_player_overhead_cover_footprint,
    project_player_standing_space,
};
use deep_hearth::mining::MiningJobId;
use deep_hearth::production::ProductionJobId;
use deep_hearth::simulation::{
    PlayerDirectControlInput, SecondaryActionOutcome, advance_control_tick_with_direct_input,
    advance_neutral_control_span,
};
use deep_hearth::spatial::{HorizontalVelocity, VerticalVelocity, VoxelCoord};
use deep_hearth::surface::{SurfaceGatheringError, SurfaceResourceId};
use deep_hearth::survival::Vitality;
use deep_hearth::world::{DayPhase, assess_player_weather};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BatchReceipt {
    Gathered(SurfaceResourceId),
    Mined(MiningJobId),
    Produced(ProductionJobId),
    Excavated(VoxelCoord),
}

pub(super) struct StepReport {
    noteworthy: bool,
    travel_interrupt: bool,
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
            let mut report = format!(
                "Ready: supported timber at ({},{},{}); current material, support and work admission passed.\n",
                position.x(),
                position.y(),
                position.z()
            );
            if stack.commodity().form() == FORM_BOARD
                && let Some(footprint) =
                    project_player_overhead_cover_footprint(&self.registries, &self.state)
            {
                if !footprint.intersects_column(position) {
                    report.push_str("Shelter preview: this panel is not over your current standing position; it will not provide overhead cover here.\n");
                } else if position.y() < footprint.minimum_y() {
                    let _ = writeln!(
                        report,
                        "Shelter preview: panel is below head clearance; overhead cover here requires a panel at height {} or higher. This placement may obstruct movement.",
                        footprint.minimum_y(),
                    );
                } else if !footprint.fits_single_column() {
                    report.push_str("Shelter preview: the player straddles multiple cells; one panel alone will not cover the complete standing footprint. Move toward the cell center or cover the other cells.\n");
                } else {
                    report.push_str("Shelter preview: this panel is above the entire current standing footprint and is positioned to provide overhead cover when completed. It does not block wind from the sides.\n");
                }
            }
            return Ok(report);
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

    /// Wait awake for the next requested daylight phase, eliding only neutral
    /// control intervals proven inert by simulation. Events, changing local threats, expired heat and new
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
        if urgency.is_none_or(view::Urgency::requires_provisions_or_warmth) {
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
        let mut residents =
            view::ResidentWatch::capture(&self.registries, &self.state, self.player_position()?);
        let initial_lit_fires = lit_nearby_fires(&self.registries, &self.state);
        let mut output = String::new();
        let mut actual = 0_u64;
        while actual < maximum {
            let previous_tick = self.state.tick();
            let (report, elapsed) = self.step_bounded(
                PlayerDirectControlInput::NEUTRAL,
                maximum - actual,
                &mut output,
            )?;
            let noteworthy = report.noteworthy;
            actual += elapsed;
            if noteworthy {
                output.push_str("Daylight watch interrupted by an event.\n");
                break;
            }
            if view::urgency(&self.registries, &self.state) != urgency {
                output.push_str("Daylight watch interrupted by a changed survival condition.\n");
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
                if let Some(description) = residents.observe(view::ResidentWatch::capture(
                    &self.registries,
                    &self.state,
                    self.player_position()?,
                )) {
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
        let mut visible = view::local_cues(&self.registries, &self.state);
        let mut actual = 0;
        let mut completed = false;
        let mut interrupted = false;
        while actual < limit {
            let previous = self.state.tick();
            let (report, elapsed) =
                self.step_bounded(PlayerDirectControlInput::NEUTRAL, limit - actual, output)?;
            actual += elapsed;
            completed = report.batch_receipt == Some(expected);
            interrupted =
                report.other_event || view::urgency(&self.registries, &self.state) != initial;
            // The expected receipt can itself change the local observation
            // (e.g. consuming the final fruit). Re-admission checks the source.
            if !completed
                && !interrupted
                && self.state.tick() != previous
                && let Some(description) = self.wait_attention(&mut visible, true)?
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
        let mut visible = view::local_cues(&self.registries, &self.state);
        // A directional jump is one impulse followed by continued horizontal
        // control. Do not hold the jump button across a landing: the motion
        // owner would legitimately launch another jump on the next grounded
        // sample. Nearby changes must not interrupt the first jump mid-ledge.
        let directional_jump = walking && input.movement().jump();
        let mut actual = 0;
        while actual < limit {
            let body_before = walking
                .then(|| self.state.logistics().player().map(|player| player.body()))
                .flatten();
            let slow_tick_before = self.state.tick();
            let sampled_input = if directional_jump && actual != 0 {
                PlayerDirectControlInput::new(input.movement().with_jump(false))
            } else {
                input
            };
            let (report, elapsed) =
                self.step_bounded(sampled_input, limit - actual, &mut output)?;
            let noteworthy = report.noteworthy;
            actual += elapsed;
            if noteworthy || view::urgency(&self.registries, &self.state) != before {
                break;
            }
            if !directional_jump
                && self.state.tick() != slow_tick_before
                && let Some(description) = self.wait_attention(&mut visible, work_wait)?
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
        previous: &mut view::LocalCues,
        work_wait: bool,
    ) -> Result<Option<&'static str>, String> {
        let current = view::local_cues(&self.registries, &self.state);
        let attention = if work_wait {
            previous.work_attention(&current, self.player_position()?)
        } else {
            previous.changed(&current)
        };
        *previous = current;
        Ok(attention)
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
        self.step_bounded(input, 1, output)
            .map(|(report, _)| report)
    }

    /// The same transcript path handles both one physical control sample and
    /// one guarded neutral span. Every caller supplies its remaining horizon.
    fn step_bounded(
        &mut self,
        input: PlayerDirectControlInput,
        limit: u64,
        output: &mut String,
    ) -> Result<(StepReport, u64), String> {
        let before = self.player_position()?;
        let previous_work = self.state.player_work().active();
        let control_before = self.state.control_tick().value();
        let outcome = if input == PlayerDirectControlInput::NEUTRAL {
            let budget =
                std::num::NonZeroU64::new(limit).expect("positive remaining control limit");
            advance_neutral_control_span(&self.registries, &mut self.state, budget)
        } else {
            advance_control_tick_with_direct_input(&self.registries, &mut self.state, input)
        }
        .map_err(|error| format!("simulation tick failed: {error}"))?;
        let elapsed = self.state.control_tick().value() - control_before;
        let stale_gathering_option = matches!(
            outcome.secondary_action(),
            Some(SecondaryActionOutcome::SurfaceGatheringRejected(
                SurfaceGatheringError::StalePlayerOption { .. }
            ))
        );
        // Physical motion and automatic custody transfer precede direct actions
        // in the control owner. Preserve that order when both occur in one frame.
        let mut batch_receipt = outcome
            .block_excavation()
            .map(|receipt| BatchReceipt::Excavated(receipt.target()));
        let _ = note_text(
            output,
            "Excavated",
            outcome.block_excavation().map(feedback::excavation),
        );
        let mut interrupt = false;
        let fall = outcome
            .body_motion()
            .and_then(|motion| motion.fall_impact());
        // Ordinary downhill travel makes frequent low-speed landings. Keep
        // those real physics receipts, but don't flood the transcript with
        // identical harmless impacts. A damaging landing still reports and
        // interrupts immediately before its vitality cost settles.
        // Harmless impacts stay observable to movement and work scheduling:
        // suppress only their repetitive transcript, not the physical event.
        let landed = fall.is_some();
        let _ = note_text(
            output,
            "Fall impact",
            fall.filter(|impact| impact.vitality_loss_ppm() > 0)
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
                    .map(|element| feedback::construction_finished(&self.state, element)),
            );
            interrupt |= note_text(
                output,
                "Construction interrupted",
                simulation.structural_construction_interrupted().map(|_| {
                    "The member lost its active support before activation; its material remains recoverable."
                        .to_owned()
                }),
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
                    batch_receipt = Some(BatchReceipt::Produced(job));
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
            && !landed
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
        Ok((
            StepReport {
                noteworthy: interrupt || landed || batch_receipt.is_some(),
                travel_interrupt: interrupt
                    || batch_receipt.is_some()
                    || fall.is_some_and(|impact| impact.vitality_loss_ppm() > 0),
                other_event: interrupt || landed,
                batch_receipt,
                stale_gathering_option,
            },
            elapsed,
        ))
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
