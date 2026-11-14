//! Continuous text-play stepping and event observation over the canonical simulation boundary.

use std::fmt::Write as _;

use super::{Session, activity, crafting, view};
mod feedback;
use deep_hearth::simulation::{
    PlayerDirectControlInput, SecondaryActionOutcome, advance_control_tick_with_direct_input,
};
use deep_hearth::spatial::{HorizontalVelocity, VoxelCoord};
use deep_hearth::survival::Vitality;

impl Session {
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
        self.advance_many_with_report(input, limit, walking, work_wait, true)
    }

    /// The repeated-gathering adapter reports each physical completion without
    /// flooding the transcript with an identical local look after every batch.
    pub(super) fn advance_many_compact(
        &mut self,
        input: PlayerDirectControlInput,
        limit: u64,
        work_wait: bool,
    ) -> Result<String, String> {
        self.advance_many_with_report(input, limit, false, work_wait, false)
    }

    fn advance_many_with_report(
        &mut self,
        input: PlayerDirectControlInput,
        limit: u64,
        walking: bool,
        work_wait: bool,
        show_look: bool,
    ) -> Result<String, String> {
        let mut output = String::new();
        let starting_position = self.player_position()?;
        let before = view::urgency(&self.registries, &self.state);
        let visible_before = view::local_cues(&self.registries, &self.state);
        // A directional jump is one short continuous maneuver. A changed
        // spring/track at the next slow boundary must not release horizontal
        // input while the character is still clearing a ledge. Direct injury,
        // explicit events and survival warning transitions still interrupt it.
        let directional_jump = walking && input.movement().jump();
        let mut actual = 0;
        for _ in 0..limit {
            let body_before = walking
                .then(|| self.state.logistics().player().map(|player| player.body()))
                .flatten();
            let slow_tick_before = self.state.tick();
            let noteworthy = self.step(input, &mut output)?;
            actual += 1;
            if noteworthy || view::urgency(&self.registries, &self.state) != before {
                break;
            }
            if !directional_jump
                && self.state.tick() != slow_tick_before
                && let Some((description, requires_attention)) =
                    visible_before.changed(&view::local_cues(&self.registries, &self.state))
                && (requires_attention || !work_wait)
            {
                let _ = writeln!(output, "{description}");
                break;
            }
            // A held walk into a wall can make no continuous progress for the entire
            // requested interval. Stop once the real motion owner has settled at
            // zero velocity and an unchanged horizontal position. A mere lack of
            // voxel crossing is not enough: acceleration moves within a voxel.
            // Directional jumps remain held while ascending to clear a ledge.
            if !directional_jump
                && let (Some(previous), Some(current)) = (
                    body_before,
                    self.state.logistics().player().map(|player| player.body()),
                )
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
        if show_look {
            output.push_str(&view::look(&self.registries, &self.state, None));
        }
        Ok(output)
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
    ) -> Result<bool, String> {
        let before = self.player_position()?;
        let previous_work = self.state.player_work().active();
        let outcome =
            advance_control_tick_with_direct_input(&self.registries, &mut self.state, input)
                .map_err(|error| format!("simulation tick failed: {error}"))?;
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
                    .map(|_| "Structural materials recovered into storage.".to_owned()),
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
            if let Some(scout) = simulation.kobold_scout() {
                interrupt |= note_text(output, "Scout", feedback::scout(scout));
            }
            for pulse in simulation.kobold_alarm_pulses() {
                interrupt |= note_text(output, "Heard alarm", Some(feedback::alarm(*pulse)));
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

fn note_text(output: &mut String, label: &str, event: Option<String>) -> bool {
    if let Some(value) = event {
        let _ = writeln!(output, "{label}: {value}");
        true
    } else {
        false
    }
}
