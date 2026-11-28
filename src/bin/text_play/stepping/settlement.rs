//! Player-visible slow-tick settlement receipts and interruption policy.

use std::fmt::Write as _;

use super::{BatchCompletion, BatchReceipt, Session, activity, feedback, note_text, view};
use deep_hearth::labor::PlayerWork;
use deep_hearth::production::ProductionAvailabilityChange;
use deep_hearth::simulation::TickOutcome;
use deep_hearth::spatial::VoxelCoord;

#[path = "production_feedback.rs"]
mod production_feedback;

impl Session {
    pub(super) fn note_simulation_events(
        &self,
        simulation: &TickOutcome,
        observer: VoxelCoord,
        previous_work_label: Option<&str>,
        output: &mut String,
    ) -> (bool, Option<BatchCompletion>) {
        let released_work = simulation.released_player_work();
        let mut interrupt = false;
        let mut batch = None;
        if let Some(gathered) = simulation.surface_gathering() {
            let _ = writeln!(
                output,
                "Gathered: {} g collected; {} g remains at the source.",
                view::grams(gathered.gathered_mass()),
                view::grams(gathered.remaining_mass()),
            );
            batch = Some(BatchCompletion {
                receipt: BatchReceipt::Gathered(gathered.resource()),
                surface: Some(gathered.resource()),
            });
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
                .map(|_| match released_work {
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
        // These receipts belong to exclusive player work. An unrelated
        // mining settlement or scout event must not suppress a release.
        let work_reported = interrupt || simulation.block_excavation().is_some();
        if let Some(mining) = simulation.mining_output()
            && local_output_destination(&self.state, observer, mining.destination()).is_some()
        {
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
            if matches!(released_work, Some(PlayerWork::Mining { job }) if job == mining.job()) {
                batch = Some(BatchCompletion {
                    receipt: BatchReceipt::Mined(mining.job()),
                    surface: None,
                });
            } else {
                // Settlement of an earlier job is independent of the
                // current batch; do not overwrite its completion receipt.
                interrupt = true;
            }
        }
        for completion in simulation.production_completions() {
            if let Some(feedback) =
                production_feedback::completion(&self.registries, &self.state, observer, completion)
            {
                output.push_str(&feedback.message);
                if feedback.manual {
                    batch = Some(BatchCompletion {
                        receipt: BatchReceipt::Produced(completion.job()),
                        surface: None,
                    });
                } else {
                    // Unattended local settlement is independent of any current
                    // hand-work receipt, even when both land on the same tick.
                    interrupt = true;
                }
            }
        }
        // A resume with remaining active time installs a new attention hold.
        // Its scheduling event must interrupt a wait even before any output
        // lands. A final-tick resume has no hold and is reported by the
        // completion receipt above.
        if let Some(work @ PlayerWork::ManualProduction { job }) = self.state.player_work().active()
            && simulation.production_availability_changes().iter().any(|change| {
                matches!(change, ProductionAvailabilityChange::Resumed { job: resumed, .. } if *resumed == job)
            })
        {
            let _ = writeln!(
                output,
                "Work resumed: {}.",
                activity::remaining_duration(&self.registries, &self.state, work)
            );
            interrupt = true;
        }
        if released_work.is_some() && !work_reported && batch.is_none() {
            let label = previous_work_label.expect("released work had a pre-boundary label");
            let _ = writeln!(output, "Work ended: {label}");
            interrupt = true;
        }
        if let Some(scout) = simulation.kobold_scout() {
            interrupt |= note_text(output, "Scout", feedback::scout(scout));
        }
        for pulse in simulation.kobold_alarm_pulses() {
            interrupt |= note_text(output, "Heard alarm", Some(feedback::alarm(*pulse)));
        }
        (interrupt, batch)
    }
}

/// Output receipts reveal only carried or exact-local custody at the
/// observation frame. Use the same rule for every material settlement.
fn local_output_destination(
    state: &deep_hearth::AppState,
    observer: VoxelCoord,
    destination: deep_hearth::inventory::StockpileId,
) -> Option<&'static str> {
    let player = state.logistics().player()?;
    if destination == player.carried_stockpile() {
        Some("carried")
    } else if state.logistics().stationary_stockpile_position(destination) == Some(observer) {
        Some("local storage")
    } else {
        None
    }
}
