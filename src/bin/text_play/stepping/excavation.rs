//! Bounded repeated terrain cuts. Each batch separately authorizes current
//! tool condition, physical reserves, target mass, and output custody.

use std::fmt::Write as _;

use super::{BatchReceipt, Session, view};
use deep_hearth::simulation::validate_pointed_terrain_excavation;
use deep_hearth::spatial::ViewDirection;

impl Session {
    pub(in crate::text_play) fn repeat_excavation(
        &mut self,
        aim: ViewDirection,
        count: u64,
    ) -> Result<String, String> {
        // Do not turn an older queued structural gesture into a second
        // operation at the next simulation boundary.
        if self.state.logistics().has_pending_direct_action() {
            return Err("finish the queued interaction before repeating excavation".to_owned());
        }
        let mut next = validate_pointed_terrain_excavation(&self.registries, &self.state, aim)
            .map_err(|error| error.to_string())?
            .ok_or("repeat break requires a terrain target; use ordinary break for trees and structures")?;
        let original = next.work();
        let mut output = String::new();
        let mut completed = 0;
        for index in 0..count {
            let work = next.work();
            let _ = writeln!(
                output,
                "Excavation started: {} g.",
                view::grams(work.excavated_mass())
            );
            let started = next
                .commit(&mut self.state)
                .map_err(|error| error.to_string())?;
            let wait = work
                .duration_control_ticks()
                .expect("authorized duration")
                .min(72_000);
            let (received, interrupted) =
                self.await_batch(BatchReceipt::Excavated(started.target()), wait, &mut output)?;
            completed += u64::from(received);
            if !received || interrupted {
                output.push_str("Excavation repeat interrupted by work or a condition change.\n");
                break;
            }
            if work.excavated_mass() == work.source_mass_before() {
                output.push_str("Excavation repeat: the original block has been cleared.\n");
                break;
            }
            if index + 1 == count {
                break;
            }
            next = match validate_pointed_terrain_excavation(&self.registries, &self.state, aim) {
                Ok(Some(plan))
                    if plan.work().target() == original.target()
                        && plan.work().state() == original.state()
                        && plan.work().equipment() == original.equipment() =>
                {
                    plan
                }
                Ok(_) => {
                    output.push_str("Excavation repeat: original target or held tool changed.\n");
                    break;
                }
                Err(error) => {
                    let _ = writeln!(output, "Excavation repeat stopped: {error}");
                    break;
                }
            };
        }
        let _ = writeln!(output, "Excavation repeat: {completed} batches completed.");
        output.push_str(&view::look(&self.registries, &self.state, Some(aim)));
        Ok(output)
    }
}
