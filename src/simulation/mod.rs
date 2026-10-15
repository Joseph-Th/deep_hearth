//! Canonical synchronous simulation tick pipeline with active subsystem phases wired in visible order.

mod control;
mod direct_actions;
mod error;
mod outcome;
mod planning;
mod player_death;

pub use control::{PlayerDirectControlInput, PrimaryControlIntent, SecondaryControlIntent};
pub use error::TickError;
pub use outcome::{
    ActiveSelectionDropOutcome, ControlTickOutcome, PrimaryActionOutcome, SecondaryActionOutcome,
    TickOutcome,
};

use crate::core::state::{
    AppState, apply_clock_advance, apply_control_clock_advance, debug_assert_runtime_invariants,
};
use crate::core::time::SimulationTick;
use crate::energy::{apply_passive_energy_dissipation, decide_passive_energy_dissipation};
use crate::equipment::{apply_equipment_maintenance_tick, decide_equipment_maintenance_tick};
use crate::geology::{apply_field_prospecting_tick, decide_field_prospecting_tick};
use crate::inventory::apply_storage_enclosure_dismantling_tick;
use crate::labor::{
    PlayerWork, apply_manual_power_tick, apply_player_work_tick, decide_manual_power_tick,
    decide_player_work_control_tick, decide_player_work_tick, player_work_exertion,
};
use crate::logistics::{
    PlayerControlInput, apply_player_body_motion, apply_player_step_tick,
    decide_player_body_motion, decide_player_step_tick, player_body_uses_generated_world,
};
use crate::mining::{apply_mining_tick, decide_mining_tick};
use crate::production::{CompletionApplication, apply_completion_plan, decide_due_completions};
use crate::registry::Registries;
use crate::structural::apply_structural_construction_tick;
use crate::surface::{
    apply_automatic_surface_drop_pickup, apply_surface_gathering_tick,
    decide_automatic_surface_drop_pickup,
};
use crate::survival::{
    SurvivalExertion, apply_survival_tick, assess_survival, control_exertion_between,
    decide_control_exertion_settlement, decide_survival_tick_with_local_heat,
};
use crate::world::{
    BlockExcavationTickError, apply_block_excavation_tick, decide_block_excavation_completion,
    decide_block_excavation_tick,
};

use direct_actions::attempt_direct_actions;
use planning::{
    decide_automatic_mining_output, decide_storage_dismantling_after_completions,
    decide_structural_construction_after_completions, decide_surface_gathering_after_completions,
    require_revision_capacity,
};
use player_death::{apply_player_death_effects, decide_player_death_effects};

/// Advances one canonical 20 Hz direct-control tick.
///
/// Each control tick advances the high-frequency clock and, when the admitted player's footprint
/// is backed by generated chunks, integrates authoritative body gravity/collision. Every 72nd
/// control tick also executes one complete slow-simulation quantum. Future horizontal movement,
/// direct block interaction, and combat phases belong on this surface rather than forcing slow
/// industrial/survival accounting to 20 Hz.
pub fn advance_control_tick(
    registries: &Registries,
    state: &mut AppState,
) -> Result<ControlTickOutcome, TickError> {
    advance_control_tick_with_direct_input(registries, state, PlayerDirectControlInput::NEUTRAL)
}

/// Advances one canonical 20 Hz direct-control tick with the sampled player input for this step.
pub fn advance_control_tick_with_input(
    registries: &Registries,
    state: &mut AppState,
    input: PlayerControlInput,
) -> Result<ControlTickOutcome, TickError> {
    advance_control_tick_with_direct_input(registries, state, PlayerDirectControlInput::new(input))
}

/// Advances one canonical 20 Hz control tick with locomotion and held primary-action intent.
pub fn advance_control_tick_with_direct_input(
    registries: &Registries,
    state: &mut AppState,
    input: PlayerDirectControlInput,
) -> Result<ControlTickOutcome, TickError> {
    let current = state.control_tick();
    let next_control = current
        .checked_next()
        .ok_or(TickError::ControlClockExhausted { current })?;
    if next_control.simulation_tick() == state.tick() {
        let body_plan = decide_player_body_motion(registries, state, input.movement(), false)
            .map_err(TickError::PlayerBodyMotion)?;
        let control_excavation = match state.player_work().active() {
            Some(PlayerWork::BlockExcavation { work }) if work.completes_at() == next_control => {
                let boundary = crate::core::time::ControlTick::from_simulation_tick(state.tick())
                    .unwrap_or_else(|| unreachable!("current slow tick must map to control time"));
                let elapsed_before = boundary
                    .checked_steps_since(work.started_at())
                    .unwrap_or_else(|| unreachable!("excavation completion cannot precede start"));
                let elapsed_after = next_control
                    .checked_steps_since(work.started_at())
                    .unwrap_or_else(|| unreachable!("excavation completion cannot precede start"));
                let method = registries
                    .mining()
                    .get_method(work.method())
                    .unwrap_or_else(|| {
                        panic!("runtime invariant broken: excavation method disappeared")
                    });
                let tail_exertion =
                    control_exertion_between(method.exertion(), elapsed_before, elapsed_after)
                        .unwrap_or_else(|| panic!("excavation control-rate exertion overflowed"));
                let survival_plan =
                    decide_control_exertion_settlement(registries, state, tail_exertion).map_err(
                        |error| match error {
                            crate::survival::SurvivalTickError::RevisionExhausted => {
                                TickError::SurvivalRevisionExhausted
                            }
                            crate::survival::SurvivalTickError::EnergyCostOverflow => {
                                TickError::SurvivalEnergyCostOverflow
                            }
                            crate::survival::SurvivalTickError::HydrationCostOverflow => {
                                TickError::SurvivalHydrationCostOverflow
                            }
                        },
                    )?;
                let excavation_plan = decide_block_excavation_completion(
                    registries,
                    state,
                    state.inventory(),
                    next_control,
                    state.tick(),
                )
                .map_err(map_block_excavation_tick_error)?
                .unwrap_or_else(|| {
                    panic!("runtime invariant broken: due control-rate excavation produced no completion plan")
                });
                let player_work_plan = decide_player_work_control_tick(state, next_control)
                    .map_err(|_| TickError::PlayerWorkRevisionExhausted)?;
                Some((excavation_plan, survival_plan, player_work_plan))
            }
            _ => None,
        };
        let body_motion = apply_player_body_motion(state, body_plan);
        let block_excavation =
            if let Some((excavation_plan, survival_plan, player_work_plan)) = control_excavation {
                let outcome = apply_block_excavation_tick(registries, state, Some(excavation_plan));
                let _ = apply_survival_tick(state, survival_plan);
                apply_player_work_tick(state, player_work_plan);
                outcome
            } else {
                None
            };
        let surface_drop_pickup = decide_automatic_surface_drop_pickup(registries, state)
            .map(|plan| apply_automatic_surface_drop_pickup(state, plan));
        apply_control_clock_advance(state, next_control);
        let direct_actions = attempt_direct_actions(registries, state, input);
        debug_assert_runtime_invariants(registries, state);
        return Ok(ControlTickOutcome::new(
            next_control,
            None,
            body_motion,
            surface_drop_pickup,
            block_excavation,
            direct_actions,
        ));
    }
    debug_assert!(next_control.is_simulation_boundary());
    let settlement_plan =
        decide_player_body_motion(registries, state, PlayerControlInput::NEUTRAL, true)
            .map_err(TickError::PlayerBodyMotion)?;
    let locomotion_exertion = settlement_plan
        .as_ref()
        .map_or(SurvivalExertion::REST, |plan| {
            plan.settled_locomotion_exertion()
        });
    let simulation = advance_simulation_tick(
        registries,
        state,
        next_control.simulation_tick(),
        locomotion_exertion,
    )?;
    let body_plan = decide_player_body_motion(registries, state, input.movement(), true)
        .map_err(TickError::PlayerBodyMotion)?;
    let body_motion = apply_player_body_motion(state, body_plan);
    let surface_drop_pickup = decide_automatic_surface_drop_pickup(registries, state)
        .map(|plan| apply_automatic_surface_drop_pickup(state, plan));
    let direct_actions = attempt_direct_actions(registries, state, input);
    let block_excavation = simulation.block_excavation();
    debug_assert_runtime_invariants(registries, state);
    debug_assert_eq!(state.control_tick(), next_control);
    Ok(ControlTickOutcome::new(
        next_control,
        Some(simulation),
        body_motion,
        surface_drop_pickup,
        block_excavation,
        direct_actions,
    ))
}

fn map_block_excavation_tick_error(error: BlockExcavationTickError) -> TickError {
    match error {
        BlockExcavationTickError::MaterialLotId => TickError::MaterialLotIdExhausted,
        BlockExcavationTickError::InventoryRevision => TickError::InventoryRevisionExhausted,
        BlockExcavationTickError::SurfaceResourceId => TickError::SurfaceResourceIdExhausted,
        BlockExcavationTickError::SurfaceRevision => TickError::SurfaceRevisionExhausted,
        BlockExcavationTickError::WorldRevision => TickError::WorldRevisionExhausted,
        BlockExcavationTickError::EquipmentRevision => TickError::EquipmentRevisionExhausted,
    }
}

/// Advances atomically to the next slow-simulation boundary when no control-rate body physics
/// needs intervening steps.
///
/// This helper remains valid for headless/process-only states and players outside generated body
/// truth. It returns `TickError::ControlSteppingRequired` when generated-world body physics is
/// active rather than skipping authoritative 50 ms gravity/collision effects.
pub fn advance_tick(
    registries: &Registries,
    state: &mut AppState,
) -> Result<TickOutcome, TickError> {
    if player_body_uses_generated_world(state, registries.survival().physiology().locomotion()) {
        return Err(TickError::ControlSteppingRequired {
            current: state.control_tick(),
        });
    }
    let current = state.tick();
    let next_tick = SimulationTick::new(current.value().checked_add(1).ok_or(
        TickError::ControlClockExhausted {
            current: state.control_tick(),
        },
    )?);
    if crate::core::time::ControlTick::from_simulation_tick(next_tick).is_none() {
        return Err(TickError::ControlClockExhausted {
            current: state.control_tick(),
        });
    }
    advance_simulation_tick(registries, state, next_tick, SurvivalExertion::REST)
}

/// Advances the full slow simulation by one 3.6-second physical/accounting quantum.
///
/// Every authoritative subsystem phase is sequenced here. Decisions read the pre-tick state or an
/// explicit same-tick projection when an earlier completion changes a downstream admission fact,
/// then mutations apply in one deterministic order.
fn advance_simulation_tick(
    registries: &Registries,
    state: &mut AppState,
    next_tick: SimulationTick,
    locomotion_exertion: SurvivalExertion,
) -> Result<TickOutcome, TickError> {
    debug_assert_eq!(next_tick.value(), state.tick().value() + 1);

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
    let projected_inventory_after_completions =
        completion_plan.project_inventory_after_deposits(state.inventory());
    let block_excavation_plan = decide_block_excavation_tick(
        registries,
        state,
        projected_inventory_after_completions.as_ref(),
        next_tick,
    )
    .map_err(map_block_excavation_tick_error)?;
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
    let local_warmth = state
        .available_local_energy_stores()
        .filter(|store| {
            registries
                .energy()
                .get_store(store.definition())
                .is_some_and(|definition| definition.is_local_warmth_source())
        })
        .try_fold(crate::core::quantity::Energy::ZERO, |total, store| {
            total.checked_add(passive_energy_plan.dissipated_for_store(store.id()))
        })
        .unwrap_or_else(|| panic!("local passive warmth overflowed energy range"));
    let work_exertion =
        player_work_exertion(registries, state, completion_plan.availability_changes());
    let exertion = SurvivalExertion::new(
        work_exertion
            .energy_cost_per_tick()
            .checked_add(locomotion_exertion.energy_cost_per_tick())
            .ok_or(TickError::SurvivalEnergyCostOverflow)?,
        work_exertion
            .hydration_loss_per_tick()
            .checked_add(locomotion_exertion.hydration_loss_per_tick())
            .ok_or(TickError::SurvivalHydrationCostOverflow)?,
    );
    let survival_plan =
        decide_survival_tick_with_local_heat(registries, state, exertion, next_tick, local_warmth)?;
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
        || block_excavation_plan
            .as_ref()
            .is_some_and(|plan| plan.mutates_inventory())
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
            u64::from(block_excavation_plan.is_some()),
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
    let block_excavation = apply_block_excavation_tick(registries, state, block_excavation_plan);
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
        block_excavation,
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
