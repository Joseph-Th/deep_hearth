//! Exact carried-cohort food and exact-local water commands.

use std::fmt::Write as _;

use super::{Session, command::parse_index, stepping::AdmissionWatch, view};
use deep_hearth::logistics::project_player_inventory;
use deep_hearth::simulation::{PlayerDirectControlInput, SecondaryControlIntent};

impl Session {
    pub(super) fn eat(&mut self, selection: &str) -> Result<String, String> {
        let inventory = project_player_inventory(&self.registries, &self.state)
            .map_err(|error| error.to_string())?;
        let stacks = inventory.carried().stacks();
        // A food name is stable across inventory reordering, whereas row
        // numbers are not. Require an unambiguous whole displayed name when
        // there are multiple carried cohorts; never choose one arbitrarily.
        let index = if selection.bytes().all(|byte| byte.is_ascii_digit()) {
            parse_index(selection, stacks.len())?
        } else {
            let matching = stacks
                .iter()
                .enumerate()
                .filter(|(_, stack)| {
                    self.registries
                        .materials()
                        .commodity_name(stack.commodity())
                        .is_some_and(|name| name.eq_ignore_ascii_case(selection))
                })
                .map(|(index, _)| index + 1)
                .collect::<Vec<_>>();
            match matching.as_slice() {
                [index] => *index,
                [] => {
                    return Err(format!(
                        "no carried stack named {selection:?}; use inventory to inspect exact food names"
                    ));
                }
                _ => {
                    return Err(format!(
                        "multiple carried stacks named {selection:?} at rows {matching:?}; use eat N to select the intended cohort"
                    ));
                }
            }
        };
        let stack = &stacks[index - 1];
        if self
            .registries
            .survival()
            .get_food(stack.commodity())
            .is_none()
        {
            let name = self
                .registries
                .materials()
                .commodity_name(stack.commodity())
                .unwrap_or("unidentified material");
            return Err(format!(
                "carried stack {index} ({name}) is not edible; inspect inventory and select a food stack"
            ));
        }
        let mut output = String::new();
        // A boundary frame settles the slow owners before consuming input.
        // Advance that frame first, then reobserve the exact selected cohort.
        // Never reinterpret its row number after other inventory changes.
        let choice = if self
            .state
            .control_tick()
            .checked_next()
            .is_some_and(|tick| tick.is_simulation_boundary())
        {
            let watch = AdmissionWatch::capture(self);
            let report = self.step(PlayerDirectControlInput::NEUTRAL, &mut output)?;
            if report.interrupts_wait(false) || watch.changed(self, None, &mut output)? {
                output.push_str(
                    "Eating not started: alignment interrupted by an event or changed condition.\n",
                );
                output.push_str(&view::look(&self.registries, &self.state, None));
                return Ok(output);
            }
            match stack.refreshed(&self.registries, &self.state) {
                Ok(current) => current.choice(),
                Err(error) => {
                    let _ = writeln!(
                        output,
                        "Food selection changed: {error}. Inspect inventory and select again."
                    );
                    return Ok(output);
                }
            }
        } else {
            stack.choice()
        };
        output.push_str(
            &self.advance_one(
                PlayerDirectControlInput::NEUTRAL
                    .with_secondary(SecondaryControlIntent::eat_stack(choice)),
            )?,
        );
        Ok(output)
    }

    pub(super) fn drink(&mut self, index: &str) -> Result<String, String> {
        let sources =
            deep_hearth::survival::available_local_drink_sources(&self.registries, &self.state)
                .collect::<Vec<_>>();
        let index = parse_index(index, sources.len())?;
        self.advance_one(PlayerDirectControlInput::NEUTRAL.with_secondary(
            SecondaryControlIntent::drink_source(sources[index - 1].store()),
        ))
    }
}
