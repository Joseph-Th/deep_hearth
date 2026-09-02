//! Canonical actor-facing stepping for direct equipment maintenance.

use deep_hearth::core::state::AppState;
use deep_hearth::equipment::EquipmentMaintenanceOutcome;
use deep_hearth::labor::{EquipmentMaintenanceWork, PlayerWork};
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;

/// Advances active maintenance to one boundary no later than its scheduled completion.
pub(super) fn advance_equipment_maintenance_to(
    registries: &Registries,
    state: &mut AppState,
    work: EquipmentMaintenanceWork,
    target_tick: u64,
    context: &'static str,
) -> Option<EquipmentMaintenanceOutcome> {
    let ticks = target_tick
        .checked_sub(state.tick().value())
        .unwrap_or_else(|| panic!("gameplay harness {context} service target precedes now"));
    assert!(
        ticks > 0,
        "gameplay harness {context} service advance must occupy time"
    );
    assert!(
        target_tick <= work.completes_at().value(),
        "gameplay harness {context} service advance passed its scheduled completion"
    );

    for elapsed in 1..=ticks {
        let outcome = advance_tick(registries, state).unwrap_or_else(|error| {
            panic!("gameplay harness {context} service tick failed: {error}")
        });
        assert!(
            outcome.production_availability_changes().is_empty()
                && outcome.production_completions().is_empty()
                && outcome.ready_mining_jobs().is_empty()
                && outcome.manual_power().is_none()
                && outcome.storage_enclosure_dismantling().is_none()
                && outcome.field_prospecting().is_none(),
            "gameplay harness {context} crossed unrelated observable work during maintenance"
        );

        if outcome.tick() < work.completes_at() {
            assert_eq!(
                outcome.equipment_maintenance(),
                None,
                "gameplay harness {context} recovered equipment before service completion"
            );
            match state.player_work().active() {
                Some(PlayerWork::EquipmentMaintenance { work: active }) => {
                    assert_eq!(active, work);
                }
                other => panic!(
                    "gameplay harness {context} lost maintenance attention before completion: {other:?}"
                ),
            }
            assert_eq!(
                state
                    .equipment()
                    .get_equipment(work.equipment())
                    .map(|record| record.condition()),
                Some(work.condition_before()),
                "gameplay harness {context} changed equipment condition before service completion"
            );
            continue;
        }

        let completed = outcome.equipment_maintenance().unwrap_or_else(|| {
            panic!("gameplay harness {context} produced no maintenance completion receipt")
        });
        assert_eq!(completed.equipment(), work.equipment());
        assert_eq!(completed.condition_before(), work.condition_before());
        assert_eq!(completed.condition_after(), work.condition_after());
        assert_eq!(state.player_work().active(), None);
        assert_eq!(
            state
                .equipment()
                .get_equipment(work.equipment())
                .map(|record| record.condition()),
            Some(work.condition_after()),
            "gameplay harness {context} completion receipt must match durable condition recovery"
        );
        assert_eq!(elapsed, ticks);
        return Some(completed);
    }

    assert!(target_tick < work.completes_at().value());
    None
}

/// Completes the currently active equipment-maintenance interval.
pub(super) fn finish_active_equipment_maintenance(
    registries: &Registries,
    state: &mut AppState,
    context: &'static str,
) -> (u64, EquipmentMaintenanceOutcome) {
    let work = match state.player_work().active() {
        Some(PlayerWork::EquipmentMaintenance { work }) => work,
        other => panic!(
            "gameplay harness {context} expected active maintenance before finishing: {other:?}"
        ),
    };
    let ticks = work
        .completes_at()
        .value()
        .checked_sub(state.tick().value())
        .unwrap_or_else(|| panic!("gameplay harness {context} service completion precedes now"));
    let completion = advance_equipment_maintenance_to(
        registries,
        state,
        work,
        work.completes_at().value(),
        context,
    )
    .unwrap_or_else(|| panic!("gameplay harness {context} service produced no completion"));
    (ticks, completion)
}
