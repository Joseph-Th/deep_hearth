//! Read-only physiological exertion projection for current or same-tick resumed player work.

use crate::core::state::AppState;
use crate::labor::PlayerWork;
use crate::labor::power_physics::resolve_manual_power_exertion;
use crate::production::{ProductionAvailabilityChange, find_availability_change};
use crate::registry::Registries;
use crate::survival::SurvivalExertion;

/// Resolves the incremental physiological cost of the currently active player-owned job.
#[must_use]
pub(crate) fn player_work_exertion(
    registries: &Registries,
    state: &AppState,
    production_availability: &[ProductionAvailabilityChange],
) -> SurvivalExertion {
    let Some(work) = state.player_work().active() else {
        let mut resumed_manual_work = production_availability.iter().filter_map(|change| {
            let ProductionAvailabilityChange::Resumed { job, .. } = *change else {
                return None;
            };
            let record = state.production().get_job(job).unwrap_or_else(|| {
                panic!("runtime invariant broken: resumed production job is missing")
            });
            registries.manual_process_exertion(record.process())
        });
        let exertion = resumed_manual_work.next().unwrap_or(SurvivalExertion::REST);
        assert!(
            resumed_manual_work.next().is_none(),
            "runtime invariant broken: more than one manual production job resumed in one tick"
        );
        return exertion;
    };
    match work {
        PlayerWork::ManualProduction { job } => {
            let record = state.production().get_job(job).unwrap_or_else(|| {
                panic!(
                    "runtime invariant broken: player work references missing manual production job"
                )
            });
            let active_this_tick = find_availability_change(production_availability, job)
                .map_or(!record.is_suspended(), |change| {
                    matches!(change, ProductionAvailabilityChange::Resumed { .. })
                });
            if !active_this_tick {
                return SurvivalExertion::REST;
            }
            registries
                .manual_process_exertion(record.process())
                .unwrap_or_else(|| {
                    panic!(
                        "runtime invariant broken: player production job has no manual definition"
                    )
                })
        }
        PlayerWork::Mining { job } => {
            let record = state.mining().get_job(job).unwrap_or_else(|| {
                panic!("runtime invariant broken: player work references missing mining job")
            });
            registries
                .mining()
                .get_method(record.method())
                .unwrap_or_else(|| {
                    panic!("runtime invariant broken: player mining job has no method definition")
                })
                .exertion()
        }
        PlayerWork::ManualPower { work } => {
            let definition = registries
                .labor()
                .get_manual_power(work.method())
                .copied()
                .unwrap_or_else(|| {
                    panic!("runtime invariant broken: player power work has no method definition")
                });
            let duration = work
                .completes_at()
                .checked_duration_since(work.started_at())
                .unwrap_or_else(|| {
                    panic!("runtime invariant broken: manual power completes before it starts")
                });
            resolve_manual_power_exertion(
                work.output().energy(),
                duration,
                definition.maximum_exertion(),
                definition.metabolic_efficiency_ppm(),
            )
            .unwrap_or_else(|error| {
                panic!("runtime invariant broken: manual power exertion is invalid: {error:?}")
            })
        }
        PlayerWork::Prospecting { work } => registries
            .labor()
            .get_prospecting(work.method())
            .unwrap_or_else(|| {
                panic!("runtime invariant broken: player prospecting work has no method definition")
            })
            .exertion(),
        PlayerWork::EquipmentMaintenance { work } => {
            let record = state
                .equipment()
                .get_equipment(work.equipment())
                .unwrap_or_else(|| {
                    panic!(
                        "runtime invariant broken: maintenance work references missing equipment"
                    )
                });
            registries
                .equipment()
                .get_equipment(record.definition())
                .and_then(|definition| definition.maintenance_profile())
                .unwrap_or_else(|| {
                    panic!(
                        "runtime invariant broken: maintenance work has no authored service profile"
                    )
                })
                .exertion()
        }
        PlayerWork::StorageEnclosureDismantling { work } => registries
            .storage()
            .get(work.definition())
            .unwrap_or_else(|| {
                panic!("runtime invariant broken: storage dismantling has no authored definition")
            })
            .dismantle_exertion(),
        PlayerWork::Eating { work: _ } | PlayerWork::Drinking { work: _ } => SurvivalExertion::REST,
    }
}
