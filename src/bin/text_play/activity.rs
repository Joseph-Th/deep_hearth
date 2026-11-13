//! Text-only activity labels and completion estimates from persisted work schedules.

use deep_hearth::core::time::CONTROL_TICKS_PER_SIMULATION_TICK;
use deep_hearth::labor::PlayerWork;
use deep_hearth::surface::player_surface_gathering_options;
use deep_hearth::{AppState, Registries};

pub fn label(registries: &Registries, state: &AppState, work: PlayerWork) -> String {
    match work {
        PlayerWork::TreeFelling { .. } => "Felling a tree".to_owned(),
        PlayerWork::SurfaceGathering { work } => {
            let commodity = player_surface_gathering_options(registries, state)
                .ok()
                .and_then(|options| {
                    options
                        .into_iter()
                        .find(|option| option.resource() == work.resource())
                        .map(|option| option.commodity())
                });
            let name = commodity
                .and_then(|key| registries.materials().commodity_name(key))
                .unwrap_or("loose material");
            format!(
                "Gathering {} g of {name}",
                work.gathered_mass().milligrams() / 1_000
            )
        }
        PlayerWork::BlockExcavation { .. } => "Excavating a block".to_owned(),
        PlayerWork::ManualProduction { job } => state
            .production()
            .get_job(job)
            .and_then(|record| registries.production().get_process(record.process()))
            .map_or_else(
                || "Manual production".to_owned(),
                |process| process.name().to_owned(),
            ),
        PlayerWork::Mining { job } => state
            .mining()
            .get_job(job)
            .and_then(|record| registries.mining().get_method(record.method()))
            .map_or_else(|| "Mining".to_owned(), |method| method.name().to_owned()),
        PlayerWork::ManualPower { .. } => "Generating manual power".to_owned(),
        PlayerWork::Prospecting { .. } => "Prospecting".to_owned(),
        PlayerWork::Eating { work } => {
            format!("Eating {} g", work.mass().milligrams() / 1_000)
        }
        PlayerWork::Drinking { work } => {
            format!("Drinking {} mL", work.volume().microliters() / 1_000)
        }
        PlayerWork::EquipmentMaintenance { .. } => "Maintaining equipment".to_owned(),
        PlayerWork::StorageEnclosureDismantling { .. } => "Dismantling storage".to_owned(),
        PlayerWork::StructuralConstruction { .. } => "Building".to_owned(),
        PlayerWork::StructuralDismantling { .. } => "Dismantling a structure".to_owned(),
    }
}

/// 20 Hz deadlines for player work; manual jobs and mining are scheduled in their own owners.
fn due_at_control_ticks(state: &AppState, work: PlayerWork) -> Option<u128> {
    let slow_tick = match work {
        PlayerWork::TreeFelling { work } => work.completes_at().value(),
        PlayerWork::SurfaceGathering { work } => work.completes_at().value(),
        PlayerWork::ManualPower { work } => work.completes_at().value(),
        PlayerWork::Prospecting { work } => work.completes_at().value(),
        PlayerWork::Eating { work } => work.completes_at().value(),
        PlayerWork::Drinking { work } => work.completes_at().value(),
        PlayerWork::EquipmentMaintenance { work } => work.completes_at().value(),
        PlayerWork::StorageEnclosureDismantling { work } => work.completes_at().value(),
        PlayerWork::StructuralConstruction { work } => work.completes_at().value(),
        PlayerWork::StructuralDismantling { work } => work.completes_at().value(),
        PlayerWork::BlockExcavation { work } => {
            return Some(u128::from(work.completes_at().value()));
        }
        PlayerWork::ManualProduction { job } => state
            .production()
            .get_job(job)
            .filter(|job| !job.is_suspended())?
            .completes_at()
            .value(),
        PlayerWork::Mining { job } => state
            .mining()
            .get_job(job)
            .filter(|record| record.is_working())?
            .completes_at()
            .value(),
    };
    Some(u128::from(slow_tick) * u128::from(CONTROL_TICKS_PER_SIMULATION_TICK))
}

fn time_label(control_ticks: u128) -> String {
    // Round upward rather than claiming a completion before its scheduled boundary.
    let seconds = (control_ticks * 50).div_ceil(1_000);
    if seconds >= 60 {
        format!("{}m {}s", seconds / 60, seconds % 60)
    } else {
        format!("{seconds}s")
    }
}

pub fn remaining_duration(registries: &Registries, state: &AppState, work: PlayerWork) -> String {
    let label = label(registries, state, work);
    if let PlayerWork::ManualProduction { job } = work
        && state
            .production()
            .get_job(job)
            .is_some_and(|job| job.is_suspended())
    {
        return format!("{label}; suspended pending physical recovery");
    }
    match due_at_control_ticks(state, work) {
        Some(due) => {
            let remaining = due.saturating_sub(u128::from(state.control_tick().value()));
            format!("{label}; scheduled completion in {}", time_label(remaining))
        }
        None => label,
    }
}

/// Bounded clients still step every control frame and interrupt on visible events.
pub fn remaining_work_control_ticks(state: &AppState) -> Option<u64> {
    let work = state.player_work().active()?;
    let due = due_at_control_ticks(state, work)?;
    let remaining = due.saturating_sub(u128::from(state.control_tick().value()));
    Some(u64::try_from(remaining).unwrap_or(u64::MAX).max(1))
}

pub fn gathering_duration(slow_ticks: u64) -> String {
    time_label(u128::from(slow_ticks) * u128::from(CONTROL_TICKS_PER_SIMULATION_TICK))
}
