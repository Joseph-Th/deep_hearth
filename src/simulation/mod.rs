//! Canonical synchronous simulation tick pipeline with active subsystem phases wired in visible order.

mod error;
mod outcome;
mod planning;
mod player_death;

pub use error::TickError;
pub use outcome::TickOutcome;

use crate::core::state::{AppState, apply_clock_advance, debug_assert_runtime_invariants};
use crate::core::time::SimulationTick;
use crate::energy::{apply_passive_energy_dissipation, decide_passive_energy_dissipation};
use crate::equipment::{apply_equipment_maintenance_tick, decide_equipment_maintenance_tick};
use crate::geology::{apply_field_prospecting_tick, decide_field_prospecting_tick};
use crate::inventory::apply_storage_enclosure_dismantling_tick;
use crate::labor::{
    apply_manual_power_tick, apply_player_work_tick, decide_manual_power_tick,
    decide_player_work_tick, player_work_exertion,
};
use crate::logistics::{apply_player_step_tick, decide_player_step_tick};
use crate::mining::{apply_mining_tick, decide_mining_tick};
use crate::production::{CompletionApplication, apply_completion_plan, decide_due_completions};
use crate::registry::Registries;
use crate::structural::apply_structural_construction_tick;
use crate::surface::apply_surface_gathering_tick;
use crate::survival::{apply_survival_tick, assess_survival, decide_survival_tick};

use planning::{
    decide_automatic_mining_output, decide_storage_dismantling_after_completions,
    decide_structural_construction_after_completions, decide_surface_gathering_after_completions,
    require_revision_capacity,
};
use player_death::{apply_player_death_effects, decide_player_death_effects};

/// Advances the full authoritative simulation by exactly one base tick.
///
/// Every authoritative subsystem phase is sequenced here. Decisions read the pre-tick state or an
/// explicit same-tick projection when an earlier completion changes a downstream admission fact,
/// then mutations apply in one deterministic order.
pub fn advance_tick(
    registries: &Registries,
    state: &mut AppState,
) -> Result<TickOutcome, TickError> {
    let current = state.tick();
    let Some(next_value) = current.value().checked_add(1) else {
        return Err(TickError::ClockExhausted { current });
    };
    let next_tick = SimulationTick::new(next_value);

    // Due jobs are indexed by exact authoritative tick. Downstream enclosure dismantling and
    // structural activation use projected owner state because production completion commits first.
    // Snapshot isolation is intentional: every other phase decides against the pre-tick snapshot
    // and defers same-tick completion effects by one tick. Revision-budget prechecks make that
    // deferral fail-closed (a newly freed store, tool, or calorie is denied this tick rather than
    // admitted against stale facts), so no additional projection is needed for those phases.
    let mut completion_plan = decide_due_completions(registries, state, next_tick)?;
    let structural_construction_plan = decide_structural_construction_after_completions(
        registries,
        state,
        &completion_plan,
        next_tick,
    );
    let surface_gathering_plan =
        decide_surface_gathering_after_completions(registries, state, &completion_plan, next_tick)?;
    let storage_enclosure_dismantling_plan = decide_storage_dismantling_after_completions(
        registries,
        state,
        &completion_plan,
        next_tick,
    )?;
    let equipment_maintenance_plan = decide_equipment_maintenance_tick(state, next_tick);
    let field_prospecting_plan = decide_field_prospecting_tick(registries, state, next_tick)?;
    let manual_power_plan = decide_manual_power_tick(state, next_tick);
    let player_step_plan = decide_player_step_tick(state, next_tick)
        .map_err(|_| TickError::LogisticsRevisionExhausted)?;
    let mining_plan = decide_mining_tick(state, next_tick);
    require_revision_capacity(
        state.mining().revision(),
        [mining_plan
            .as_ref()
            .map_or(0, |plan| plan.mining_revision_steps())],
        TickError::MiningRevisionExhausted,
    )?;
    require_revision_capacity(
        state.geology().revision(),
        [mining_plan
            .as_ref()
            .map_or(0, |plan| plan.geology_revision_steps())],
        TickError::GeologyRevisionExhausted,
    )?;
    let passive_energy_plan = decide_passive_energy_dissipation(registries, state);
    let passive_energy_revision_steps = passive_energy_plan.energy_revision_steps();
    // Production releases and manual-power completion already own future energy revisions through
    // durable resident state. Passive loss is an incidental same-tick mutation, so it must fit in
    // addition to those obligations rather than consuming capacity promised to them.
    if passive_energy_revision_steps != 0
        && !state.can_spend_energy_revisions(passive_energy_revision_steps)
    {
        return Err(TickError::EnergyRevisionExhausted);
    }
    require_revision_capacity(
        state.energy().revision(),
        [
            completion_plan.energy_revision_steps(),
            manual_power_plan
                .as_ref()
                .map_or(0, |plan| plan.energy_revision_steps()),
            passive_energy_revision_steps,
        ],
        TickError::EnergyRevisionExhausted,
    )?;
    let exertion = player_work_exertion(registries, state, completion_plan.availability_changes());
    let survival_plan = decide_survival_tick(registries, state, exertion, next_tick)?;
    // Player attention uses the projected survival result so fatality, work release, and direct-
    // consumption cancellation become authoritative on the same tick without reading partially
    // mutated state.
    let player_dead_after_tick = survival_plan
        .as_ref()
        .is_some_and(|plan| plan.player_dead_after_tick());
    let player_death_effects = decide_player_death_effects(
        registries,
        state,
        next_tick,
        player_dead_after_tick,
        &mut completion_plan,
    )?;
    let inventory_owner_busy = completion_plan.has_inventory_deposits()
        || surface_gathering_plan.is_some()
        || storage_enclosure_dismantling_plan.is_some()
        || player_death_effects
            .as_ref()
            .is_some_and(|plan| plan.mutates_inventory());
    let automatic_mining_output = decide_automatic_mining_output(
        registries,
        state,
        next_tick,
        mining_plan,
        inventory_owner_busy,
        structural_construction_plan.is_some(),
    )?;
    require_revision_capacity(
        state.equipment().revision(),
        [
            completion_plan.equipment_revision_steps(),
            mining_plan
                .as_ref()
                .map_or(0, |plan| plan.equipment_revision_steps()),
            manual_power_plan
                .as_ref()
                .map_or(0, |plan| plan.equipment_revision_steps()),
            equipment_maintenance_plan
                .as_ref()
                .map_or(0, |plan| plan.equipment_revision_steps()),
            field_prospecting_plan
                .as_ref()
                .map_or(0, |plan| plan.equipment_revision_steps()),
            player_death_effects
                .as_ref()
                .map_or(0, |plan| plan.equipment_revision_steps()),
        ],
        TickError::EquipmentRevisionExhausted,
    )?;
    let player_work_plan = decide_player_work_tick(
        registries,
        state,
        next_tick,
        completion_plan.availability_changes(),
        player_dead_after_tick,
    )
    .map_err(|_error| TickError::PlayerWorkRevisionExhausted)?;
    let CompletionApplication {
        completions: production_completions,
        availability_changes: production_availability_changes,
    } = apply_completion_plan(state, completion_plan)?;
    let structural_construction =
        apply_structural_construction_tick(state, structural_construction_plan);
    let _completed_mining_job = apply_mining_tick(state, mining_plan);
    let mining_output =
        automatic_mining_output.map(|settlement| settlement.apply_prechecked(state));
    let surface_gathering = apply_surface_gathering_tick(state, surface_gathering_plan);
    let player_step = apply_player_step_tick(state, player_step_plan);
    let manual_power = apply_manual_power_tick(state, manual_power_plan);
    let equipment_maintenance = apply_equipment_maintenance_tick(state, equipment_maintenance_plan);
    apply_passive_energy_dissipation(state, passive_energy_plan);
    let field_prospecting = apply_field_prospecting_tick(state, field_prospecting_plan);
    let storage_enclosure_dismantling =
        apply_storage_enclosure_dismantling_tick(state, storage_enclosure_dismantling_plan);
    apply_player_death_effects(state, player_death_effects);
    apply_player_work_tick(state, player_work_plan);
    let survival =
        apply_survival_tick(state, survival_plan).or_else(|| assess_survival(registries, state));
    apply_clock_advance(state, next_tick);

    debug_assert_runtime_invariants(registries, state);
    Ok(TickOutcome {
        tick: next_tick,
        production_availability_changes,
        production_completions,
        mining_output,
        player_step,
        surface_gathering,
        manual_power,
        equipment_maintenance,
        storage_enclosure_dismantling,
        structural_construction,
        field_prospecting,
        survival,
    })
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
