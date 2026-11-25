//! Exact-local gathering commands and bounded repeat admission.

use std::fmt::Write as _;

use super::{
    Session, activity,
    command::parse_index,
    quantity::parse_decimal_grams,
    stepping::{AdmissionWatch, BatchReceipt},
    view,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::labor::PlayerWork;
use deep_hearth::simulation::{PlayerDirectControlInput, SecondaryControlIntent};
use deep_hearth::surface::{
    PlayerSurfaceGatheringOption, SurfaceGatheringError, assess_affordable_player_gathering_mass,
    player_surface_gathering_options,
};
use deep_hearth::{AppState, Registries};

/// A single read-only assessment shared by the gathering catalog and commands.
pub(super) struct DefaultGatheringBatch {
    pub(super) suggested: Mass,
    pub(super) affordable: Result<Mass, SurfaceGatheringError>,
}

impl DefaultGatheringBatch {
    pub(super) fn assess(
        registries: &Registries,
        state: &AppState,
        option: PlayerSurfaceGatheringOption,
    ) -> Self {
        let suggested = option.suggested_mass_at(state);
        let affordable = if suggested.is_zero() {
            Ok(suggested)
        } else {
            assess_affordable_player_gathering_mass(registries, state, option, suggested)
        };
        Self {
            suggested,
            affordable,
        }
    }

    /// Retain the suggestion on rejection so canonical admission reports the
    /// actual blocker rather than attempting to start a zero-mass job.
    pub(super) fn request_mass(&self) -> Mass {
        match &self.affordable {
            Ok(mass) if !mass.is_zero() => *mass,
            _ => self.suggested,
        }
    }
}

struct GatheringAdmission {
    output: String,
    interrupted: bool,
}

impl Session {
    pub(super) fn gather(&mut self, args: &[&str]) -> Result<String, String> {
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
            parse_decimal_grams(grams, "gather quantity")?
        } else {
            DefaultGatheringBatch::assess(&self.registries, &self.state, option).request_mass()
        };
        if args.len() == 1 && mass.is_zero() {
            return Err("no carrying space for the suggested gathering batch".to_owned());
        }
        let limit = option.maximum_mass_at(&self.state);
        if mass.is_zero() || mass > limit {
            return Err(format!(
                "gather quantity must be positive and at most {} g",
                view::grams(limit)
            ));
        }
        let admission = self.start_gathering(option, mass)?;
        let mut output = admission.output;
        if admission.interrupted {
            output.push_str("Gathering command stopped after an event or changed condition.\n");
        }
        output.push_str(&view::status(&self.registries, &self.state));
        Ok(output)
    }

    /// Automatic physical pickup precedes secondary use within one control
    /// frame. It can invalidate a previously listed gather option even though
    /// the intended rooted source has not changed. Reproject only the *same*
    /// source and method after that real pickup; never retarget by row number
    /// or weaken the surface owner's stale-revision admission.
    fn start_gathering(
        &mut self,
        original: PlayerSurfaceGatheringOption,
        mass: Mass,
    ) -> Result<GatheringAdmission, String> {
        let mut output = String::new();
        let mut option = original;
        for _ in 0..8 {
            let watch = AdmissionWatch::capture(self);
            let mut frame = String::new();
            let report = self.step(
                PlayerDirectControlInput::NEUTRAL
                    .with_secondary(SecondaryControlIntent::gather_surface_option(option, mass)),
                &mut frame,
            )?;
            let interrupted = report.interrupts_gathering_admission()
                || watch.changed(self, report.gathering_retry_pickup(), &mut frame)?;
            if interrupted || !report.stale_gathering_option {
                output.push_str(&frame);
                return Ok(GatheringAdmission {
                    output,
                    interrupted,
                });
            }
            // Preserve actual pickups and events, but not the provisional
            // stale-option rejection that the adapter is about to recover.
            for line in frame.lines().filter(|line| !line.starts_with("Use: ")) {
                let _ = writeln!(output, "{line}");
            }
            let refreshed = original
                .refresh(&self.registries, &self.state)
                .map_err(|error| error.to_string())?;
            match refreshed {
                Some(next) if mass <= next.maximum_mass_at(&self.state) => {
                    option = next;
                }
                _ => {
                    output.push_str("Gathering stopped: the original source or carrying space changed after a nearby pickup.\n");
                    return Ok(GatheringAdmission {
                        output,
                        interrupted: false,
                    });
                }
            }
        }
        output.push_str(
            "Gathering stopped: nearby pickups kept changing the original source selection.\n",
        );
        Ok(GatheringAdmission {
            output,
            interrupted: false,
        })
    }

    /// Repeat exact-local hand gathering without skipping any authoritative work
    /// or retargeting an indexed choice when a source is exhausted. A visible
    /// interruption leaves the active job for the player to resume explicitly.
    pub(super) fn gather_repeated(&mut self, args: &[&str]) -> Result<String, String> {
        let (index_text, mass_text, repetitions_text) = match args {
            [index, "repeat", count] => (*index, None, *count),
            [index, mass, "repeat", count] => (*index, Some(*mass), *count),
            _ => return Err("use gather N [grams] repeat COUNT".to_owned()),
        };
        let count = repetitions_text
            .parse::<usize>()
            .map_err(|_| "repeat count must be a whole number")?;
        if !(1..=32).contains(&count) {
            return Err("repeat count must be between 1 and 32".to_owned());
        }
        let requested_mass = mass_text
            .map(|grams| parse_decimal_grams(grams, "gather quantity"))
            .transpose()?;
        if requested_mass.is_some_and(|mass| mass.is_zero()) {
            return Err("gather quantity must be positive".to_owned());
        }
        if self.state.player_work().active().is_some() {
            return Err("finish the current work before starting repeated gathering".to_owned());
        }
        let options = player_surface_gathering_options(&self.registries, &self.state)
            .map_err(|error| error.to_string())?;
        let index = parse_index(index_text, options.len())?;
        let original = options[index - 1];
        let mut output = String::new();
        let mut completed = 0;
        for _ in 0..count {
            let Some(option) = original
                .refresh(&self.registries, &self.state)
                .map_err(|error| error.to_string())?
            else {
                let _ = writeln!(
                    output,
                    "Gather repeat: source depleted after {completed} batches."
                );
                break;
            };
            let mass = requested_mass.unwrap_or_else(|| {
                DefaultGatheringBatch::assess(&self.registries, &self.state, option).request_mass()
            });
            if mass.is_zero() || mass > option.maximum_mass_at(&self.state) {
                if completed == 0 {
                    return Err(
                        "gather quantity exceeds the available source, method, or carrying space"
                            .to_owned(),
                    );
                }
                let _ = writeln!(
                    output,
                    "Gather repeat: insufficient remaining source or carrying space after {completed} batches."
                );
                break;
            }
            let admission = self.start_gathering(option, mass)?;
            output.push_str(&admission.output);
            if admission.interrupted {
                let _ = writeln!(
                    output,
                    "Gather repeat: interrupted after {completed} completed batches."
                );
                break;
            }
            if !matches!(
                self.state.player_work().active(),
                Some(PlayerWork::SurfaceGathering { .. })
            ) {
                let _ = writeln!(
                    output,
                    "Gather repeat: admission rejected after {completed} completed batches."
                );
                break;
            }
            let wait = activity::remaining_work_control_ticks(&self.state)
                .ok_or("gathering started without a scheduled completion")?
                .min(activity::COMPLETION_WAIT_LIMIT);
            let (received, interrupted) = self.await_batch(
                BatchReceipt::Gathered(original.resource()),
                wait,
                &mut output,
            )?;
            completed += usize::from(received);
            if !received || interrupted {
                let _ = writeln!(
                    output,
                    "Gather repeat: interrupted after {completed} completed batches."
                );
                break;
            }
        }
        let _ = writeln!(output, "Gather repeat: {completed} batches completed.");
        output.push_str(&view::look(&self.registries, &self.state, None));
        Ok(output)
    }
}
