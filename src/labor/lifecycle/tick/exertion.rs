//! Read-only physiological exertion projection for current or same-tick resumed player work.

use crate::core::state::AppState;
use crate::core::time::{CONTROL_TICKS_PER_SIMULATION_TICK, ControlTick};
use crate::labor::PlayerWork;
use crate::production::{ProductionAvailabilityChange, find_availability_change};
use crate::registry::Registries;
use crate::survival::{SurvivalExertion, control_exertion_between};

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
        PlayerWork::Step { .. } => {
            let carrying = crate::logistics::assess_player_carrying(state).unwrap_or_else(|| {
                panic!("runtime invariant broken: player step has no carrying assessment")
            });
            registries
                .survival()
                .physiology()
                .locomotion()
                .exertion_for_load(carrying.stored(), carrying.capacity())
        }
        PlayerWork::SurfaceGathering { work } => registries
            .labor()
            .get_surface_gathering(work.method())
            .unwrap_or_else(|| {
                panic!("runtime invariant broken: surface gathering has no authored method")
            })
            .exertion(),
        PlayerWork::BlockExcavation { work } => {
            let exertion = registries
                .mining()
                .get_method(work.method())
                .unwrap_or_else(|| {
                    panic!(
                        "runtime invariant broken: block excavation has no mining method definition"
                    )
                })
                .exertion();
            let next_boundary = ControlTick::from_simulation_tick(
                state
                    .tick()
                    .checked_add_span(crate::core::time::TickSpan::new(1))
                    .unwrap_or_else(|| unreachable!("active work cannot outlive simulation time")),
            )
            .unwrap_or_else(|| unreachable!("next slow boundary must be representable"));
            let previous_boundary =
                ControlTick::new(next_boundary.value() - CONTROL_TICKS_PER_SIMULATION_TICK);
            let interval_start = std::cmp::max(previous_boundary, work.started_at());
            let interval_end = std::cmp::min(next_boundary, work.completes_at());
            if interval_end <= interval_start {
                SurvivalExertion::REST
            } else {
                let elapsed_before = interval_start
                    .checked_steps_since(work.started_at())
                    .unwrap_or_else(|| unreachable!("excavation interval cannot precede start"));
                let elapsed_after = interval_end
                    .checked_steps_since(work.started_at())
                    .unwrap_or_else(|| unreachable!("excavation interval cannot precede start"));
                control_exertion_between(exertion, elapsed_before, elapsed_after)
                    .unwrap_or_else(|| panic!("excavation control-rate exertion overflowed"))
            }
        }
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
        PlayerWork::ManualPower { work } => work.exertion(),
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
        PlayerWork::StructuralConstruction { work } => registries
            .labor()
            .get_structural_construction(work.method())
            .unwrap_or_else(|| {
                panic!("runtime invariant broken: structural construction has no authored method")
            })
            .exertion(),
        PlayerWork::StructuralDismantling { work } => registries
            .labor()
            .get_structural_construction(work.method())
            .unwrap_or_else(|| {
                panic!("runtime invariant broken: structural dismantling has no authored method")
            })
            .exertion(),
        PlayerWork::Eating { work: _ } | PlayerWork::Drinking { work: _ } => SurvivalExertion::REST,
    }
}
