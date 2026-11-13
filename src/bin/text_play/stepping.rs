//! Continuous text-play stepping and event observation over the canonical simulation boundary.

use std::fmt::Write as _;

use super::{Session, activity, crafting, view};
use deep_hearth::simulation::{
    PlayerDirectControlInput, SecondaryActionOutcome, advance_control_tick_with_direct_input,
};
use deep_hearth::spatial::VoxelCoord;
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
        let mut interrupt = note(output, "Primary", outcome.primary_action());
        interrupt |= note_secondary(output, outcome.secondary_action());
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

/// Project only visible intake and gathering facts into the player transcript.
/// Debug formatting the canonical eating receipt would expose private lot IDs,
/// internal energy units and hidden consumption accounting.
fn note_secondary(output: &mut String, event: Option<&SecondaryActionOutcome>) -> bool {
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
        Some(other) => note(output, "Use", Some(other)),
        None => false,
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
